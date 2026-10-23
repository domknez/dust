//! One playing track: its byte stream (a decrypted Deezer song, a podcast's own
//! file, or a song file from a music server), container reader and decoder.

use super::library::{Decoded, OpenError, Playback};
use super::resample::Resampler;
use crate::deezer::{Deezer, Format, StreamSource};
use crate::output::RATE;
use std::io::Read;
use std::sync::Arc;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::TimeBase;

/// Bytes per second per kbps (1000 / 8).
const BYTES_PER_KBPS: f64 = 125.0;

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
    pub song_id: u64,
    pub format: Format,
    /// File extension, a hint for picking the container reader.
    pub extension: String,
    /// Files at another sample rate are converted (otherwise refused).
    pub any_rate: bool,
    /// Seeking jumps into the file (opened again at the new time); otherwise it skips
    /// forward in place and only starts over to go back.
    pub jumps: bool,
}

/// A Deezer song or podcast episode.
struct DeezerBytes {
    client: Deezer,
    source: StreamSource,
}

impl Opener for DeezerBytes {
    fn open(&self, at: f64) -> Result<Opened, OpenError> {
        let source = &self.source;
        if source.format == Format::Podcast {
            // Jumps by an estimated byte offset; skips from the start if it can't.
            let (bytes, start) = self.client.open_direct(&source.url, at, source.duration)?;
            return Ok(Opened { bytes, base: start, skip_until: if start > 0.0 { 0.0 } else { at } });
        }
        // CBR MP3 maps time to bytes, so jump with an HTTP range; FLAC must start at 0.
        let (offset, skip_until) = match source.format {
            Format::Mp3(kbps) if at > 0.0 => ((at * kbps as f64 * BYTES_PER_KBPS) as u64, 0.0),
            _ => (0, at),
        };
        let (bytes, offset) = self.client.open_stream(source, offset)?;
        let base = match source.format {
            Format::Mp3(kbps) => offset as f64 / (kbps as f64 * BYTES_PER_KBPS),
            Format::Flac | Format::Podcast | Format::File => 0.0,
        };
        Ok(Opened { bytes, base, skip_until })
    }
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
    resampler: Option<Resampler>,
    /// Stereo samples waiting to be resampled, reused across packets.
    stereo: Vec<i16>,
    eof: bool,
}

impl Stream {
    /// Open a Deezer song or podcast episode at track time `at` (seconds).
    pub fn open_deezer(client: &Deezer, source: StreamSource, at: f64) -> Result<Stream, OpenError> {
        let info = StreamInfo {
            song_id: source.song_id,
            format: source.format,
            extension: if source.format == Format::Flac { "flac" } else { "mp3" }.into(),
            any_rate: source.format == Format::Podcast,
            jumps: matches!(source.format, Format::Mp3(_) | Format::Podcast),
        };
        Stream::open(Arc::new(DeezerBytes { client: client.clone(), source }), info, at)
    }

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
    fn song_id(&self) -> u64 {
        self.info.song_id
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
