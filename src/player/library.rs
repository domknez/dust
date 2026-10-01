//! The engine's view of where music comes from. Deezer implements it in the app;
//! tests use fakes, so the playback logic runs without network or audio hardware.

use super::stream::Stream;
use crate::deezer::{Deezer, Format, Listen, Quality, Track};
use std::sync::Arc;

/// Opens tracks and Flow, and receives listen reports.
pub trait Library: Send + Sync {
    /// Start decoding `track` at `at` seconds.
    fn open(&self, track: &Track, quality: Quality, at: f64) -> Result<Box<dyn Playback>, String>;
    /// Next batch of Flow, optionally tuned to a mood.
    fn flow(&self, mood: Option<&str>) -> Result<Vec<Track>, String>;
    /// Fire-and-forget: a track started.
    fn report_listen_start(&self, song_id: u64);
    /// Fire-and-forget: a track finished or was skipped.
    fn report_listen(&self, listen: Listen);
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

/// One track being decoded.
pub trait Playback {
    /// Song id actually streamed (differs from the track's on regional fallbacks).
    fn song_id(&self) -> u64;
    fn format(&self) -> Format;
    /// Track time (s) at the end of the audio handed to the sink.
    fn written(&self) -> f64;
    fn set_written(&mut self, t: f64);
    /// The decoder returned [`Decoded::End`].
    fn finished(&self) -> bool;
    /// Decode the next packet, appending interleaved stereo i16 to `out`.
    fn decode_next(&mut self, out: &mut Vec<i16>) -> Decoded;
    /// Whether seeking to `t` needs [`Playback::reopen`] (vs. [`Playback::skip_to`]).
    fn needs_reopen_for(&self, t: f64) -> bool;
    /// Seek forward in place by dropping packets until `t`.
    fn skip_to(&mut self, t: f64);
    /// A fresh stream of the same track starting at `at`.
    fn reopen(&self, at: f64) -> Result<Box<dyn Playback>, String>;
}

impl Library for Deezer {
    fn open(&self, track: &Track, quality: Quality, at: f64) -> Result<Box<dyn Playback>, String> {
        let source = self.stream_source(track, quality).map_err(|e| e.to_string())?;
        Ok(Box::new(Stream::open(self, source, at)?))
    }

    fn flow(&self, mood: Option<&str>) -> Result<Vec<Track>, String> {
        Deezer::flow(self, mood).map_err(|e| e.to_string())
    }

    fn report_listen_start(&self, song_id: u64) {
        let client = self.clone();
        std::thread::spawn(move || {
            if let Err(e) = client.report_listen_start(song_id) {
                log_debug!("listen start report failed: {e}");
            }
        });
    }

    fn report_listen(&self, listen: Listen) {
        let client = self.clone();
        std::thread::spawn(move || match client.report_listen(&listen) {
            Ok(()) => log_debug!("reported listen of {} ({} s, skipped: {})", listen.song_id, listen.listened_secs, listen.skipped),
            Err(e) => log_warn!("listen report failed: {e}"),
        });
    }
}

/// Shared handle the engine keeps.
pub type SharedLibrary = Arc<dyn Library>;
