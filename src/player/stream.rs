//! One playing track: its byte stream (a decrypted Deezer song, or a podcast's own
//! file), container reader and decoder.

use super::library::{Decoded, Playback};
use super::resample::Resampler;
use crate::deezer::{Deezer, Format, StreamSource};
use crate::output::RATE;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::TimeBase;

/// Bytes per second per kbps (1000 / 8).
const BYTES_PER_KBPS: f64 = 125.0;

pub struct Stream {
    client: Deezer,
    source: StreamSource,
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    audio_track: u32,
    time_base: TimeBase,
    /// Track time (s) at which this byte stream starts (non-zero after an MP3 range seek).
    base: f64,
    /// Discard packets before this track time (FLAC seeks).
    skip_until: f64,
    /// Track time (s) at the end of the audio handed to the sink.
    written: f64,
    /// Decoded interleaved samples, reused across packets.
    samples: Vec<i16>,
    /// For files at another sample rate than the output's (podcasts).
    resampler: Option<Resampler>,
    /// Stereo samples waiting to be resampled, reused across packets.
    stereo: Vec<i16>,
    eof: bool,
}

impl Stream {
    /// Open `source` at track time `at` (seconds).
    pub fn open(client: &Deezer, source: StreamSource, at: f64) -> Result<Stream, String> {
        let (bytes, base, skip_until) = if source.format == Format::Podcast {
            // Jumps by an estimated byte offset; skips from the start if it can't.
            let (bytes, start) = client.open_direct(&source.url, at, source.duration).map_err(|e| e.to_string())?;
            (bytes, start, if start > 0.0 { 0.0 } else { at })
        } else {
            // CBR MP3 maps time to bytes, so jump with an HTTP range; FLAC must start at 0.
            let (offset, skip_until) = match source.format {
                Format::Mp3(kbps) if at > 0.0 => ((at * kbps as f64 * BYTES_PER_KBPS) as u64, 0.0),
                _ => (0, at),
            };
            let (bytes, offset) = client.open_stream(&source, offset).map_err(|e| e.to_string())?;
            let base = match source.format {
                Format::Mp3(kbps) => offset as f64 / (kbps as f64 * BYTES_PER_KBPS),
                Format::Flac | Format::Podcast => 0.0,
            };
            (bytes, base, skip_until)
        };
        let mss = MediaSourceStream::new(Box::new(ReadOnlySource::new(bytes)), Default::default());
        let mut hint = Hint::new();
        hint.with_extension(if source.format == Format::Flac { "flac" } else { "mp3" });
        let reader = symphonia::default::get_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .map_err(|e| format!("probe: {e}"))?;
        let track = reader.default_track(TrackType::Audio).ok_or("No audio track")?;
        let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or("No audio codec")?;
        let resampler = match params.sample_rate {
            Some(rate) if rate != RATE && source.format == Format::Podcast => Some(Resampler::new(rate, RATE)),
            Some(rate) if rate != RATE => return Err(format!("Unsupported sample rate {rate}")),
            _ => None,
        };
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(|e| format!("decoder: {e}"))?;
        let time_base = track.time_base.or_else(|| TimeBase::try_new(1, RATE)).ok_or("No time base")?;
        Ok(Stream {
            client: client.clone(),
            audio_track: track.id,
            time_base,
            source,
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
    fn song_id(&self) -> u64 {
        self.source.song_id
    }

    fn format(&self) -> Format {
        self.source.format
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
        match self.source.format {
            Format::Mp3(_) | Format::Podcast => true,
            // FLAC over a non-seekable HTTP stream: skip forward, or restart and skip.
            Format::Flac => t < self.written,
        }
    }

    fn skip_to(&mut self, t: f64) {
        self.skip_until = t;
        self.written = t;
    }

    fn reopen(&self, at: f64) -> Result<Box<dyn Playback>, String> {
        Ok(Box::new(Stream::open(&self.client, self.source.clone(), at)?))
    }
}
