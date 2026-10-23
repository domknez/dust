//! One playing track: its byte stream (from any source), container reader and
//! decoder.

use super::playback::{Decoded, OpenError, Playback};
use crate::model::{Format, TrackId};
use crate::output::RATE;
use crate::output::resample::Resampler;
use std::io::Read;
use std::sync::Arc;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::TimeBase;

/// A track's bytes, opened at (about) a position.
pub struct Opened {
    pub bytes: Box<dyn Read + Send + Sync>,
    /// Track time (s) the bytes start at (non-zero after a jump into the file).
    pub base: f64,
    /// Discard audio before this track time (when the bytes start earlier than asked).
    pub skip_until: f64,
}

/// Where a track's bytes come from; opened again for seeks.
pub trait Opener: Send + Sync {
    fn open(&self, at: f64) -> Result<Opened, OpenError>;
}

/// What the decoder needs to know about a stream besides its bytes.
#[derive(Clone)]
pub struct StreamInfo {
    /// Song id reported for listens.
    pub song_id: TrackId,
    pub format: Format,
    /// File extension, a hint for picking the container reader.
    pub extension: String,
    /// Files at another sample rate are converted (otherwise refused).
    pub any_rate: bool,
    /// Seeking jumps into the file (opened again at the new time); otherwise it skips
    /// forward in place and only starts over to go back.
    pub jumps: bool,
}

pub struct Stream {
    opener: Arc<dyn Opener>,
    info: StreamInfo,
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    audio_track: u32,
    time_base: TimeBase,
    /// Track time (s) at which this byte stream starts (non-zero after a jump).
    base: f64,
    /// Discard packets before this track time (seeks that couldn't jump).
    skip_until: f64,
    /// Track time (s) at the end of the audio handed to the sink.
    written: f64,
    /// Decoded interleaved samples, reused across packets.
    samples: Vec<i16>,
    /// For files at another sample rate than the output's (podcasts, server files).
    resampler: Option<Resampler<i16>>,
    /// Stereo samples waiting to be resampled, reused across packets.
    stereo: Vec<i16>,
    eof: bool,
}

impl Stream {
    /// Open the bytes from `opener` at track time `at` (seconds).
    pub fn open(opener: Arc<dyn Opener>, info: StreamInfo, at: f64) -> Result<Stream, OpenError> {
        // Anything that goes wrong after the bytes arrive is the file's: skip it.
        let bad = OpenError::Unavailable;
        let Opened { bytes, base, skip_until } = opener.open(at)?;
        let mss = MediaSourceStream::new(Box::new(ReadOnlySource::new(bytes)), Default::default());
        let mut hint = Hint::new();
        hint.with_extension(&info.extension);
        let reader = symphonia::default::get_probe().probe(&hint, mss, FormatOptions::default(), MetadataOptions::default()).map_err(
            |e| match e {
                // The download stalled or broke off while the container was being
                // read (a file that just ends early is the file's fault).
                DecodeError::IoError(e) if e.kind() != std::io::ErrorKind::UnexpectedEof => OpenError::Transient(format!("probe: {e}")),
                e => bad(format!("probe: {e}")),
            },
        )?;
        let track = reader.default_track(TrackType::Audio).ok_or_else(|| bad("No audio track".into()))?;
        let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or_else(|| bad("No audio codec".into()))?;
        let resampler = match params.sample_rate {
            Some(rate) if rate != RATE && info.any_rate => Some(Resampler::new(rate, RATE)),
            Some(rate) if rate != RATE => return Err(bad(format!("Unsupported sample rate {rate}"))),
            _ => None,
        };
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(|e| bad(format!("decoder: {e}")))?;
        let time_base = track.time_base.or_else(|| TimeBase::try_new(1, RATE)).ok_or_else(|| bad("No time base".into()))?;
        Ok(Stream {
            audio_track: track.id,
            time_base,
            opener,
            info,
            reader,
            decoder,
            base,
            skip_until,
            written: base.max(skip_until),
            samples: Vec::new(),
            resampler,
            stereo: Vec::new(),
            eof: false,
        })
    }

    fn decode_packet(&mut self, out: &mut Vec<i16>) -> Decoded {
        let packet = match self.reader.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => return Decoded::End,
            Err(DecodeError::ResetRequired) => {
                self.decoder.reset();
                return Decoded::Nothing;
            }
            Err(DecodeError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Decoded::End,
            Err(e) => {
                log_warn!("stream: {e}");
                return Decoded::End;
            }
        };
        if packet.track_id != self.audio_track {
            return Decoded::Nothing;
        }
        let start = self.base + self.time_base.calc_time_saturating(packet.pts).as_secs_f64();
        if start + 0.05 < self.skip_until {
            return Decoded::Nothing;
        }
        let decoded = match self.decoder.decode(&packet) {
            Ok(d) => d,
            Err(DecodeError::DecodeError(_)) => return Decoded::Nothing,
            Err(e) => {
                log_warn!("decode: {e}");
                return Decoded::End;
            }
        };
        let frames = decoded.frames();
        if frames == 0 {
            return Decoded::Nothing;
        }
        let (channels, rate) = (decoded.spec().channels().count(), decoded.spec().rate());
        decoded.copy_to_vec_interleaved(&mut self.samples);
        let samples = &self.samples[..];
        let stereo = if self.resampler.is_some() {
            self.stereo.clear();
            &mut self.stereo
        } else {
            &mut *out
        };
        match channels {
            2 => stereo.extend_from_slice(samples),
            1 => stereo.extend(samples.iter().flat_map(|&x| [x, x])),
            _ => stereo.extend(samples.chunks_exact(channels).flat_map(|f| [f[0], f[1]])),
        }
        if let Some(r) = self.resampler.as_mut() {
            r.process(&self.stereo, out);
        }
        Decoded::Audio { ends_at: start + frames as f64 / rate as f64 }
    }
}

impl Playback for Stream {
    fn song_id(&self) -> TrackId {
        self.info.song_id.clone()
    }

    fn format(&self) -> Format {
        self.info.format
    }

    fn written(&self) -> f64 {
        self.written
    }

    fn set_written(&mut self, t: f64) {
        self.written = t;
    }

    fn finished(&self) -> bool {
        self.eof
    }

    fn decode_next(&mut self, out: &mut Vec<i16>) -> Decoded {
        let result = self.decode_packet(out);
        if matches!(result, Decoded::End) {
            self.eof = true;
        }
        result
    }

    fn needs_reopen_for(&self, t: f64) -> bool {
        // A file that can't be jumped into over HTTP (FLAC, MP4, ...): skip forward,
        // or start over and skip.
        self.info.jumps || t < self.written
    }

    fn skip_to(&mut self, t: f64) {
        self.skip_until = t;
        self.written = t;
    }

    fn reopen(&self, at: f64) -> Result<Box<dyn Playback>, OpenError> {
        Ok(Box::new(Stream::open(self.opener.clone(), self.info.clone(), at)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::Mutex;

    /// A 16-bit stereo WAV file of `secs` seconds at `rate`, each frame's left
    /// sample its time in tenths of a second (so a sample tells where it's from).
    fn wav(secs: f64, rate: u32) -> Vec<u8> {
        let frames = (secs * rate as f64) as u32;
        let data = frames * 4;
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((36 + data).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes()); // PCM
        out.extend(2u16.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * 4).to_le_bytes());
        out.extend(4u16.to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend(data.to_le_bytes());
        for i in 0..frames {
            let tenths = (i as f64 / rate as f64 * 10.0) as i16;
            out.extend(tenths.to_le_bytes());
            out.extend(0i16.to_le_bytes());
        }
        out
    }

    /// Serves a WAV file; jumping in (when `jumps`) like an HTTP range request that
    /// starts at a whole second, else from the start with the rest to skip.
    struct WavBytes {
        file: Vec<u8>,
        rate: u32,
        jumps: bool,
        opened_at: Mutex<Vec<f64>>,
    }

    impl Opener for WavBytes {
        fn open(&self, at: f64) -> Result<Opened, OpenError> {
            self.opened_at.lock().unwrap().push(at);
            if !self.jumps || at == 0.0 {
                return Ok(Opened { bytes: Box::new(Cursor::new(self.file.clone())), base: 0.0, skip_until: at });
            }
            // A new file of what's left from the whole second before `at`.
            let base = at.floor();
            let skip = (base * self.rate as f64) as usize * 4;
            let mut file = wav(0.0, self.rate);
            let rest = &self.file[44 + skip..];
            file[4..8].copy_from_slice(&(36 + rest.len() as u32).to_le_bytes());
            file[40..44].copy_from_slice(&(rest.len() as u32).to_le_bytes());
            file.extend(rest);
            Ok(Opened { bytes: Box::new(Cursor::new(file)), base, skip_until: at })
        }
    }

    fn stream(secs: f64, rate: u32, jumps: bool, at: f64) -> (Stream, Arc<WavBytes>) {
        let opener = Arc::new(WavBytes { file: wav(secs, rate), rate, jumps, opened_at: Mutex::default() });
        let info = StreamInfo { song_id: TrackId::from(1), format: Format::File, extension: "wav".into(), any_rate: true, jumps };
        (Stream::open(opener.clone(), info, at).expect("opens"), opener)
    }

    /// Decode the next packet with audio: (where it ends, its first left sample).
    fn next_audio(s: &mut dyn Playback) -> Option<(f64, i16)> {
        loop {
            let mut out = Vec::new();
            match s.decode_next(&mut out) {
                Decoded::Audio { ends_at } => return Some((ends_at, out[0])),
                Decoded::Nothing => {}
                Decoded::End => return None,
            }
        }
    }

    /// Decode to the end: where the last audio ends.
    fn end(s: &mut dyn Playback) -> f64 {
        let mut last = 0.0;
        while let Some((ends_at, _)) = next_audio(s) {
            last = ends_at;
        }
        assert!(s.finished());
        last
    }

    #[test]
    fn plays_from_the_start_to_the_end() {
        let (mut s, _) = stream(3.0, RATE, false, 0.0);
        assert_eq!(s.written(), 0.0);
        let (_, first) = next_audio(&mut s).unwrap();
        assert_eq!(first, 0);
        assert!((end(&mut s) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn opening_part_way_skips_what_comes_before() {
        let (mut s, _) = stream(5.0, RATE, false, 2.5);
        assert_eq!(s.written(), 2.5, "the position starts where asked");
        let (ends_at, first) = next_audio(&mut s).unwrap();
        // Packets starting up to 50 ms early are kept rather than cut.
        assert!(ends_at > 2.45 && ends_at < 3.0, "{ends_at}");
        assert!((24..=25).contains(&first), "audio from about 2.5 s: {first}");
        assert!((end(&mut s) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn jumping_in_counts_from_where_the_bytes_start() {
        let (mut s, opener) = stream(5.0, RATE, true, 3.2);
        assert_eq!(*opener.opened_at.lock().unwrap(), [3.2]);
        assert_eq!(s.written(), 3.2);
        let (ends_at, first) = next_audio(&mut s).unwrap();
        assert!(ends_at > 3.15 && ends_at < 3.5, "track time, not the new file's: {ends_at}");
        assert!((31..=32).contains(&first), "{first}");
        assert!((end(&mut s) - 5.0).abs() < 1e-6, "ends at the track's end");
    }

    #[test]
    fn seeks_skip_forward_in_place_and_reopen_to_go_back() {
        let (mut s, opener) = stream(5.0, RATE, false, 0.0);
        next_audio(&mut s).unwrap();
        s.set_written(0.1);
        assert!(!s.needs_reopen_for(2.0), "forward: skip in place");
        s.skip_to(2.0);
        assert_eq!(s.written(), 2.0);
        let (ends_at, first) = next_audio(&mut s).unwrap();
        assert!(ends_at > 1.95 && ends_at < 2.5, "{ends_at}");
        assert!((19..=20).contains(&first), "{first}");
        s.set_written(ends_at);

        assert!(s.needs_reopen_for(1.0), "back: start over");
        let mut back = s.reopen(1.0).unwrap();
        assert_eq!(*opener.opened_at.lock().unwrap(), [0.0, 1.0]);
        let (_, first) = next_audio(back.as_mut()).unwrap();
        assert!((9..=10).contains(&first), "{first}");

        // A stream that can jump reopens for any seek.
        let (s, _) = stream(5.0, RATE, true, 0.0);
        assert!(s.needs_reopen_for(4.0));
    }

    #[test]
    fn another_sample_rate_keeps_track_time() {
        let (mut s, _) = stream(2.0, 22_050, false, 0.0);
        let mut out = Vec::new();
        let mut last = 0.0;
        loop {
            match s.decode_next(&mut out) {
                Decoded::Audio { ends_at } => last = ends_at,
                Decoded::Nothing => {}
                Decoded::End => break,
            }
        }
        assert!((last - 2.0).abs() < 1e-6, "{last}");
        let frames = out.len() / 2;
        assert!((frames as i64 - 2 * RATE as i64).abs() <= 2, "converted to the output's rate: {frames} frames");
    }
}
