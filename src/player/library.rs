//! The engine's view of where music comes from. Deezer implements it in the app;
//! tests use fakes, so the playback logic runs without network or audio hardware.

use super::stream::Stream;
use crate::deezer::{Deezer, Format, Listen, Quality, Track};
use crate::model::Error;
use std::sync::Arc;

/// Why a track didn't open, which decides what the engine does next.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    /// The track itself can't be played (not licensed, gone from the server, a file
    /// that won't decode): skip to the next one.
    #[error("{0}")]
    Unavailable(String),
    /// Not the track's fault (no network, a timeout, the server busy): skipping would
    /// fail the same way, so playback stops there and play tries again.
    #[error("{0}")]
    Transient(String),
}

impl From<Error> for OpenError {
    fn from(e: Error) -> Self {
        let message = e.to_string();
        match e {
            Error::NotAvailable | Error::NoStreamingLicense | Error::Api { .. } | Error::Unsupported(_) | Error::Other(_) => {
                OpenError::Unavailable(message)
            }
            // Timeouts, rate limits and server trouble may pass; other statuses (404
            // for a deleted song, 403 for a refused one) won't.
            Error::Http { status, .. } if status == 408 || status == 429 || status >= 500 => OpenError::Transient(message),
            Error::Http { .. } => OpenError::Unavailable(message),
            // The same for every track: skipping would only fail again.
            Error::Network { .. } | Error::SessionExpired | Error::InvalidToken => OpenError::Transient(message),
        }
    }
}

/// Opens tracks and Flow, and receives listen reports.
pub trait Library: Send + Sync {
    /// Start decoding `track` at `at` seconds.
    fn open(&self, track: &Track, quality: Quality, at: f64) -> Result<Box<dyn Playback>, OpenError>;
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
    fn reopen(&self, at: f64) -> Result<Box<dyn Playback>, OpenError>;
}

impl Library for Deezer {
    fn open(&self, track: &Track, quality: Quality, at: f64) -> Result<Box<dyn Playback>, OpenError> {
        let source = self.stream_source(track, quality)?;
        Ok(Box::new(Stream::open_deezer(self, source, at)?))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Service;

    fn kind(e: Error) -> &'static str {
        match OpenError::from(e) {
            OpenError::Unavailable(_) => "unavailable",
            OpenError::Transient(_) => "transient",
        }
    }

    #[test]
    fn track_problems_are_unavailable() {
        assert_eq!(kind(Error::NotAvailable), "unavailable");
        assert_eq!(kind(Error::NoStreamingLicense), "unavailable");
        assert_eq!(kind(Service::Subsonic.http(ureq::Error::StatusCode(404))), "unavailable");
        assert_eq!(kind(Service::Deezer.http(ureq::Error::StatusCode(403))), "unavailable");
        assert_eq!(kind(Service::Subsonic.api("the server sent no audio for this song")), "unavailable");
    }

    #[test]
    fn network_problems_are_transient() {
        assert_eq!(kind(Service::Deezer.network("timed out")), "transient");
        assert_eq!(kind(Service::Subsonic.http(ureq::Error::ConnectionFailed)), "transient");
        assert_eq!(kind(Service::Subsonic.http(ureq::Error::StatusCode(503))), "transient");
        assert_eq!(kind(Service::Deezer.http(ureq::Error::StatusCode(429))), "transient");
        assert_eq!(kind(Error::SessionExpired), "transient");
    }

    #[test]
    fn messages_are_kept() {
        assert_eq!(OpenError::from(Service::Subsonic.network("timed out")).to_string(), "Couldn't reach your Navidrome server: timed out");
    }
}
