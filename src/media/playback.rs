//! What the player needs from a track being decoded, and why one didn't open.

use crate::deezer::Format;
use crate::model::Error;

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
