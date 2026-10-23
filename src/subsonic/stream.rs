//! Song files from the server. Formats dust decodes are streamed as they are;
//! others (Opus, WMA, ...) are converted to MP3 by the server as they play.

use super::{Subsonic, api, http};
use crate::media::{OpenError, Opened, Opener, Prefetch, Stream, StreamInfo};
use crate::model::{Error, Format, Origin, Result, ServerSong, Track};
use std::sync::Arc;
use std::time::Duration;

/// How long a stalled download may wait for data before giving up.
const STALL: Duration = Duration::from_secs(30);
/// Bitrate the server converts other formats to.
const CONVERTED_KBPS: &str = "320";

/// How a file is fetched and decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delivery {
    /// The original file; MP3 can be jumped into (by an estimated byte offset).
    Raw { jumps: bool },
    /// Converted to MP3 by the server, which can also start it at a time.
    Converted,
}

/// What dust does with a file of this type, and the decoder hint for it.
fn delivery(suffix: &str) -> (Delivery, &'static str) {
    match suffix {
        "mp3" => (Delivery::Raw { jumps: true }, "mp3"),
        "flac" => (Delivery::Raw { jumps: false }, "flac"),
        "m4a" | "mp4" | "m4b" | "alac" => (Delivery::Raw { jumps: false }, "m4a"),
        "aac" => (Delivery::Raw { jumps: false }, "aac"),
        "ogg" | "oga" => (Delivery::Raw { jumps: false }, "ogg"),
        "wav" | "wave" => (Delivery::Raw { jumps: false }, "wav"),
        _ => (Delivery::Converted, "mp3"),
    }
}

struct ServerBytes {
    client: Subsonic,
    song: ServerSong,
    delivery: Delivery,
    duration: u32,
}

impl Opener for ServerBytes {
    fn open(&self, at: f64) -> std::result::Result<Opened, OpenError> {
        Ok(self.open_at(at)?)
    }
}

impl ServerBytes {
    fn open_at(&self, at: f64) -> Result<Opened> {
        let id = self.song.id.as_str();
        match self.delivery {
            Delivery::Converted => {
                // Whole seconds: the server starts the conversion there.
                let start = at.max(0.0).floor();
                let offset = start.to_string();
                let url =
                    self.client.url("stream", &[("id", id), ("format", "mp3"), ("maxBitRate", CONVERTED_KBPS), ("timeOffset", &offset)]);
                let resp = self.get(&url)?;
                Ok(Opened { bytes: resp, base: start, skip_until: at })
            }
            Delivery::Raw { jumps } => {
                let url = self.client.url("stream", &[("id", id), ("format", "raw")]);
                if !(jumps && at > 0.0 && self.duration > 0) {
                    return Ok(Opened { bytes: self.get(&url)?, base: 0.0, skip_until: at });
                }
                // Jump to about the right place; the decoder finds the next frame.
                let size = self.client.file_size(&url);
                let Some(size) = size else {
                    return Ok(Opened { bytes: self.get(&url)?, base: 0.0, skip_until: at });
                };
                let offset = (size as f64 * (at / self.duration as f64).min(1.0)) as u64;
                let (bytes, partial) = self.get_range(&url, offset)?;
                let base = if partial { offset as f64 / size as f64 * self.duration as f64 } else { 0.0 };
                Ok(Opened { bytes, base, skip_until: if partial { 0.0 } else { at } })
            }
        }
    }

    fn get(&self, url: &str) -> Result<Box<dyn std::io::Read + Send + Sync>> {
        let req = self.client.0.agent.get(url).config().timeout_global(None).timeout_recv_response(Some(STALL)).build();
        let resp = req.call().map_err(http)?;
        check_audio(&resp)?;
        Ok(Box::new(Prefetch::new(resp.into_body().into_reader(), STALL)))
    }

    /// Bytes from `offset`; whether the server honoured the range.
    fn get_range(&self, url: &str, offset: u64) -> Result<(Box<dyn std::io::Read + Send + Sync>, bool)> {
        let req = self.client.0.agent.get(url).config().timeout_global(None).timeout_recv_response(Some(STALL)).build();
        let resp = req.header("Range", &format!("bytes={offset}-")).call().map_err(http)?;
        check_audio(&resp)?;
        let partial = resp.status() == 206;
        Ok((Box::new(Prefetch::new(resp.into_body().into_reader(), STALL)), partial))
    }
}

/// A failed `stream` call answers with a JSON (or XML) error instead of audio.
fn check_audio(resp: &ureq::http::Response<ureq::Body>) -> Result<()> {
    let ctype = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if ctype.contains("json") || ctype.contains("xml") {
        return Err(api("the server sent no audio for this song"));
    }
    Ok(())
}

impl Subsonic {
    /// Start a song at `at` seconds. A file dust can't decode after all (say, Opus
    /// in an `.ogg`) is asked for again as MP3.
    pub fn open_track(&self, track: &Track, at: f64) -> std::result::Result<Stream, OpenError> {
        let Origin::Server(song) = &track.origin else {
            return Err(Error::Other("not a song from this server".into()).into());
        };
        self.remember(&song.id);
        let (delivery, extension) = delivery(&song.suffix);
        match self.open_as(track, song, delivery, extension, at) {
            // Only a file problem: a network one would fail the same way again.
            Err(OpenError::Unavailable(e)) if delivery != Delivery::Converted => {
                log_info!("{}: {e}; asking the server for MP3", song.suffix);
                self.open_as(track, song, Delivery::Converted, "mp3", at)
            }
            result => result,
        }
    }

    fn open_as(
        &self,
        track: &Track,
        song: &ServerSong,
        delivery: Delivery,
        extension: &str,
        at: f64,
    ) -> std::result::Result<Stream, OpenError> {
        let info = StreamInfo {
            song_id: track.id,
            format: Format::File,
            extension: extension.into(),
            any_rate: true,
            jumps: !matches!(delivery, Delivery::Raw { jumps: false }),
        };
        let opener = ServerBytes { client: self.clone(), song: song.clone(), delivery, duration: track.duration };
        Stream::open(Arc::new(opener), info, at)
    }

    /// Size of a file on the server, if it says.
    fn file_size(&self, url: &str) -> Option<u64> {
        let resp = self.0.agent.head(url).call().ok()?;
        resp.headers().get("content-length")?.to_str().ok()?.parse().ok().filter(|&n| n > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodable_files_stream_as_they_are() {
        assert_eq!(delivery("mp3"), (Delivery::Raw { jumps: true }, "mp3"));
        assert_eq!(delivery("flac"), (Delivery::Raw { jumps: false }, "flac"));
        assert_eq!(delivery("m4a").1, "m4a");
        assert_eq!(delivery("ogg").1, "ogg");
        assert_eq!(delivery("wav").1, "wav");
        assert_eq!(delivery("opus"), (Delivery::Converted, "mp3"));
        assert_eq!(delivery("wma").0, Delivery::Converted);
    }
}
