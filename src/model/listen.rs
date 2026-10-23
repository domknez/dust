//! A listen, as the player reports it to the source (history, scrobbling).

use super::Format;

/// One listen of a track.
#[derive(Clone, Debug)]
pub struct Listen {
    pub song_id: u64,
    pub format: Format,
    pub started_unix: u64,
    pub listened_secs: u64,
    /// Ended before the track did (skip/seek), as opposed to playing through.
    pub skipped: bool,
    /// Identifies this listen to the source (a random UUID).
    pub stream_id: String,
}
