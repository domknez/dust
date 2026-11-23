//! Tracks how long the current track has actually been heard and reports each
//! listen to the source (Deezer's history, Flow and Last.fm, like its own apps; a
//! server's play counts). Every listen also comes back as a [`Played`], for dust's
//! own play history.

use super::library::Library;
use super::{Flow, Played};
use crate::model::{Format, Listen, Track, TrackId};
use std::time::Duration;

/// A listen that played to within this many seconds of the end isn't a skip.
const SKIP_TOLERANCE_SECS: u64 = 3;

struct Current {
    source: String,
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

    /// A new track of `source` (its [`data_key`](crate::source::Source::data_key))
    /// started: report the start and begin counting.
    /// `flow`: the endless Flow it came from, if any.
    pub fn start(
        &mut self,
        library: Option<&dyn Library>,
        source: String,
        song_id: TrackId,
        format: Format,
        track: &Track,
        flow: Option<Flow>,
    ) {
        let track_secs = track.duration;
        let listen = Listen { song_id: song_id.clone(), format, started_unix: crate::clock::unix_now(), listened_secs: 0, skipped: false };
        if self.enabled
            && let Some(library) = library
        {
            library.report_listen_start(song_id);
        }
        self.current = Some(Current { source, listen, heard: Duration::ZERO, track_secs, seeked: false, track: track.clone(), flow });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{OpenError, Playback};
    use crate::model::{Error, Quality};
    use std::sync::Mutex;

    /// Keeps the listens reported.
    #[derive(Default)]
    struct Reports(Mutex<Vec<Listen>>);

    impl Library for Reports {
        fn open(&self, _: &Track, _: Quality, _: f64) -> std::result::Result<Box<dyn Playback>, OpenError> {
            Err(OpenError::Unavailable("not here".into()))
        }
        fn flow(&self, _: Option<&str>) -> std::result::Result<Vec<Track>, Error> {
            Ok(Vec::new())
        }
        fn report_listen_start(&self, _: TrackId) {}
        fn report_listen(&self, listen: Listen) {
            self.0.lock().unwrap().push(listen);
        }
    }

    /// A 200 s track heard for `secs` (seeked in it with `seeked`): what the play
    /// history keeps (skipped?) and what the source is told (skipped?).
    fn listen(secs: u64, seeked: bool) -> (Option<bool>, Option<bool>) {
        let reports = Reports::default();
        let mut tracker = ListenTracker::new();
        tracker.start(Some(&reports), crate::deezer::ID.key.into(), TrackId::from(1), Format::Mp3(320), &Track::test(1), None);
        tracker.heard(Duration::from_secs(secs));
        if seeked {
            tracker.seeked();
        }
        let played = tracker.finish(Some(&reports)).map(|p| p.skipped);
        let reported = reports.0.lock().unwrap().first().map(|l| l.skipped);
        (played, reported)
    }

    #[test]
    fn skips_and_listens_through() {
        assert_eq!(listen(0, false), (None, None), "not heard: nothing to keep or tell");
        assert_eq!(listen(20, false), (Some(true), Some(true)), "moved on early");
        assert_eq!(listen(150, false), (Some(false), Some(true)), "most of it: heard, but ended early");
        assert_eq!(listen(200 - SKIP_TOLERANCE_SECS, false), (Some(false), Some(false)), "to the end, near enough");
        assert_eq!(listen(200, true), (Some(false), Some(true)), "a seek tells the source it wasn't straight through");
    }

    #[test]
    fn reports_can_be_turned_off_but_the_history_still_keeps_it() {
        let reports = Reports::default();
        let mut tracker = ListenTracker { enabled: false, ..ListenTracker::new() };
        tracker.start(Some(&reports), crate::deezer::ID.key.into(), TrackId::from(1), Format::Mp3(320), &Track::test(1), None);
        tracker.heard(Duration::from_secs(60));
        assert_eq!(tracker.finish(Some(&reports)).map(|p| p.source).as_deref(), Some("deezer"));
        assert!(reports.0.lock().unwrap().is_empty());
    }
}
