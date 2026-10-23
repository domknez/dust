//! Playing a Deezer song or podcast episode: its bytes, opened again for seeks.

use super::Deezer;
use super::models::{Format, Quality, Track};
use super::streaming::StreamSource;
use crate::media::{OpenError, Opened, Opener, Stream, StreamInfo};
use std::sync::Arc;

/// Bytes per second per kbps (1000 / 8).
const BYTES_PER_KBPS: f64 = 125.0;

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

impl Deezer {
    /// Start `track` (a song or podcast episode) at track time `at` (seconds), in
    /// `quality` or the best below it that Deezer has.
    pub fn open_track(&self, track: &Track, quality: Quality, at: f64) -> Result<Stream, OpenError> {
        let source = self.stream_source(track, quality)?;
        let info = StreamInfo {
            song_id: source.song_id,
            format: source.format,
            extension: if source.format == Format::Flac { "flac" } else { "mp3" }.into(),
            any_rate: source.format == Format::Podcast,
            jumps: matches!(source.format, Format::Mp3(_) | Format::Podcast),
        };
        Stream::open(Arc::new(DeezerBytes { client: self.clone(), source }), info, at)
    }
}
