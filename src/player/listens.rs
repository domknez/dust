//! Tracks how long the current track has actually been heard and reports each
//! listen to the source (Deezer's history, Flow and Last.fm, like its own apps; a
//! server's play counts). Every listen also comes back as a [`Played`], for dust's
//! own play history.

use super::library::Library;
use super::{Flow, Played};
use crate::model::{Format, Listen, Track, TrackId};
use crate::source::SourceId;
use std::time::Duration;

/// A listen that played to within this many seconds of the end isn't a skip.
const SKIP_TOLERANCE_SECS: u64 = 3;

struct Current {
    source: &'static str,
    listen: Listen,
    heard: Duration,
    track_secs: u32,
    seeked: bool,
    /// What the play history keeps of it.
    track: Track,
    flow: Option<Flow>,
}

pub struct ListenTracker {
    pub enabled: bool,
    current: Option<Current>,
}

impl ListenTracker {
    pub fn new() -> Self {
        Self { enabled: true, current: None }
    }

    /// A new track of `source` started: report the start and begin counting.
    /// `flow`: the endless Flow it came from, if any.
    pub fn start(
        &mut self,
        library: Option<&dyn Library>,
        source: SourceId,
        song_id: TrackId,
        format: Format,
        track: &Track,
        flow: Option<Flow>,
    ) {
        let track_secs = track.duration;
        let listen = Listen {
            song_id: song_id.clone(),
            format,
            started_unix: unix_now(),
            listened_secs: 0,
            skipped: false,
            stream_id: stream_uuid(),
        };
        if self.enabled
            && let Some(library) = library
        {
            library.report_listen_start(song_id);
        }
        self.current =
            Some(Current { source: source.key, listen, heard: Duration::ZERO, track_secs, seeked: false, track: track.clone(), flow });
    }

    /// Audio played for `elapsed`.
    pub fn heard(&mut self, elapsed: Duration) {
        if let Some(c) = self.current.as_mut() {
            c.heard += elapsed;
        }
    }

    pub fn seeked(&mut self) {
        if let Some(c) = self.current.as_mut() {
            c.seeked = true;
        }
    }

    /// The current track ended or was replaced: report it, and return it for the
    /// play history (also when reports are off). None when none of it was heard.
    pub fn finish(&mut self, library: Option<&dyn Library>) -> Option<Played> {
        let current = self.current.take()?;
        let secs = current.heard.as_secs();
        if secs == 0 {
            return None;
        }
        let track_secs = current.track_secs as u64;
        let ended_early = secs + SKIP_TOLERANCE_SECS < track_secs;
        let played = Played {
            source: current.source,
            track: current.track,
            started_unix: current.listen.started_unix,
            heard_secs: secs,
            // Less than half heard: moved on, rather than seeked around in it.
            skipped: ended_early && secs * 2 < track_secs,
            flow: current.flow,
        };
        if self.enabled
            && let Some(library) = library
        {
            let mut listen = current.listen;
            listen.listened_secs = secs;
            listen.skipped = current.seeked || ended_early;
            library.report_listen(listen);
        }
        Some(played)
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Random RFC 4122 v4-shaped id, as Deezer's clients send per stream.
fn stream_uuid() -> String {
    let h = format!("{:016x}{:016x}", fastrand::u64(..), fastrand::u64(..));
    format!("{}-{}-4{}-a{}-{}", &h[..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32])
}
