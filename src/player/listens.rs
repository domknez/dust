//! Tracks how long the current track has actually been heard and reports each
//! listen to Deezer (history, Flow, Last.fm), like Deezer's own apps.

use super::debug_enabled;
use crate::deezer::{Deezer, Format, Listen};
use std::time::Duration;

/// A listen that played to within this many seconds of the end isn't a skip.
const SKIP_TOLERANCE_SECS: u64 = 3;

struct Current {
    listen: Listen,
    heard: Duration,
    track_secs: u32,
    seeked: bool,
}

pub struct ListenTracker {
    pub enabled: bool,
    current: Option<Current>,
}

impl ListenTracker {
    pub fn new() -> Self {
        Self { enabled: true, current: None }
    }

    /// A new track started: report the start and begin counting.
    pub fn start(&mut self, client: Option<&Deezer>, song_id: u64, format: Format, track_secs: u32) {
        let listen = Listen { song_id, format, started_unix: unix_now(), listened_secs: 0, skipped: false, stream_id: stream_uuid() };
        if self.enabled
            && let Some(client) = client.cloned()
        {
            std::thread::spawn(move || {
                let _ = client.report_listen_start(song_id);
            });
        }
        self.current = Some(Current { listen, heard: Duration::ZERO, track_secs, seeked: false });
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

    /// The current track ended or was replaced: report it (in the background).
    pub fn finish(&mut self, client: Option<&Deezer>) {
        let Some(current) = self.current.take() else { return };
        let secs = current.heard.as_secs();
        let (true, Some(client), true) = (self.enabled, client.cloned(), secs > 0) else { return };
        let mut listen = current.listen;
        listen.listened_secs = secs;
        listen.skipped = current.seeked || secs + SKIP_TOLERANCE_SECS < current.track_secs as u64;
        std::thread::spawn(move || match client.report_listen(&listen) {
            Err(e) => eprintln!("dust: listen report failed: {e}"),
            Ok(()) if debug_enabled() => {
                eprintln!("dust: reported listen of {} ({} s, skipped: {})", listen.song_id, listen.listened_secs, listen.skipped)
            }
            Ok(()) => {}
        });
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
