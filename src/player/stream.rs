//! One playing track: its decrypted byte stream, container reader and decoder.

use crate::deezer::{Deezer, Format, StreamSource};
use crate::output::RATE;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSourceStream, ReadOnlySource};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::TimeBase;

/// Bytes per second per kbps (1000 / 8).
const BYTES_PER_KBPS: f64 = 125.0;

pub struct Stream {
    pub source: StreamSource,
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    audio_track: u32,
    time_base: TimeBase,
    /// Track time (s) at which this byte stream starts (non-zero after an MP3 range seek).
    base: f64,
    /// Discard packets before this track time (FLAC seeks).
    skip_until: f64,
    /// Track time (s) at the end of the audio handed to the sink.
    pub written: f64,
    samples: Option<SampleBuffer<i16>>,
    pub eof: bool,
}

/// Result of decoding one packet.
pub enum Decoded {
    /// Stereo samples were appended; they end at this track time (s).
    Audio { ends_at: f64 },
    /// Nothing usable in this packet (other track, skipped, decode glitch).
    Nothing,
    /// End of stream (or an unrecoverable error).
    End,
}

impl Stream {
    /// Open `source` at track time `at` (seconds).
    pub fn open(client: &Deezer, source: StreamSource, at: f64) -> Result<Stream, String> {
        // CBR MP3 maps time to bytes, so jump with an HTTP range; FLAC must start at 0.
        let (offset, skip_until) = match source.format {
            Format::Mp3(kbps) if at > 0.0 => ((at * kbps as f64 * BYTES_PER_KBPS) as u64, 0.0),
            _ => (0, at),
        };
        let (bytes, offset) = client.open_stream(&source, offset).map_err(|e| e.to_string())?;
        let base = match source.format {
            Format::Mp3(kbps) => offset as f64 / (kbps as f64 * BYTES_PER_KBPS),
            Format::Flac => 0.0,
        };
        let mss = MediaSourceStream::new(Box::new(ReadOnlySource::new(bytes)), Default::default());
        let mut hint = Hint::new();
        hint.with_extension(if source.format == Format::Flac { "flac" } else { "mp3" });
        let reader = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| format!("probe: {e}"))?
            .format;
        let track = reader.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("No audio track")?;
        if track.codec_params.sample_rate.is_some_and(|r| r != RATE) {
            return Err(format!("Unsupported sample rate {:?}", track.codec_params.sample_rate));
        }
        let decoder =
            symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| format!("decoder: {e}"))?;
        Ok(Stream {
            audio_track: track.id,
            time_base: track.codec_params.time_base.unwrap_or(TimeBase::new(1, RATE)),
            source,
            reader,
            decoder,
            base,
            skip_until,
            written: base.max(skip_until),
            samples: None,
            eof: false,
        })
    }

    /// Whether seeking to `t` needs a new HTTP request (vs. skipping forward in place).
    pub fn needs_reopen_for(&self, t: f64) -> bool {
        match self.source.format {
            Format::Mp3(_) => true,
            // FLAC over a non-seekable HTTP stream: skip forward, or restart and skip.
            Format::Flac => t < self.written,
        }
    }

    /// Seek forward without reopening: drop packets until `t`.
    pub fn skip_to(&mut self, t: f64) {
        self.skip_until = t;
        self.written = t;
    }

    /// Decode the next packet, appending interleaved stereo i16 to `out`.
    pub fn decode_next(&mut self, out: &mut Vec<i16>) -> Decoded {
        let packet = match self.reader.next_packet() {
            Ok(p) => p,
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
        if packet.track_id() != self.audio_track {
            return Decoded::Nothing;
        }
        let t = self.time_base.calc_time(packet.ts);
        let start = self.base + t.seconds as f64 + t.frac;
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
        let spec = *decoded.spec();
        let frames = decoded.frames();
        if frames == 0 {
            return Decoded::Nothing;
        }
        let channels = spec.channels.count();
        let buf = self.samples.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
        if buf.capacity() < decoded.capacity() * channels {
            *buf = SampleBuffer::new(decoded.capacity() as u64, spec);
        }
        buf.copy_interleaved_ref(decoded);
        let samples = &buf.samples()[..frames * channels];
        match channels {
            2 => out.extend_from_slice(samples),
            1 => out.extend(samples.iter().flat_map(|&x| [x, x])),
            _ => out.extend(samples.chunks_exact(channels).flat_map(|f| [f[0], f[1]])),
        }
        Decoded::Audio { ends_at: start + frames as f64 / spec.rate as f64 }
    }
}
