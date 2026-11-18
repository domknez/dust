//! The sleep timer: when it stops the music, and the fade-out before a timed one.
//! It only decides; the engine pauses, stops and sets the sink's volume.

use super::{SLEEP_FADE, Sleep};
use crate::model::Track;
use std::time::{Duration, Instant};

/// Volume changes per second while a timed sleep fades out (a speaker gets a
/// change about twice a second, not every loop).
const FADE_STEPS_PER_SEC: f32 = 2.0;

#[derive(Default)]
pub struct SleepTimer {
    set: Option<Sleep>,
    /// Share of the listener's volume the sink plays at while fading out.
    fade: Option<f32>,
}

/// What a look at a timed sleep found.
#[derive(Debug, PartialEq)]
pub enum Tick {
    /// Not yet; nothing to change.
    Waiting,
    /// Not yet: the sink plays at this share of the listener's volume from now.
    Fade(f32),
    /// Time's up (the timer is cleared). `restore`: a fade was on, so the sink
    /// goes back to the listener's volume.
    Due { restore: bool },
}

impl SleepTimer {
    pub fn get(&self) -> Option<Sleep> {
        self.set
    }

    /// Set (or with None, turn off) the timer. Returns whether a fade was undone,
    /// so the sink goes back to the listener's volume.
    pub fn set(&mut self, sleep: Option<Sleep>) -> bool {
        self.set = sleep;
        self.end_fade()
    }

    /// Tests: as if time moved on to `sleep`'s deadline, keeping any fade.
    #[cfg(test)]
    pub fn reset_deadline(&mut self, sleep: Sleep) {
        self.set = Some(sleep);
    }

    /// The timer was used up at the end of a track.
    pub fn clear(&mut self) {
        self.set = None;
    }

    /// Where a timed sleep stands at `now`; fading only while `playing`.
    pub fn tick(&mut self, now: Instant, playing: bool) -> Tick {
        let Some(Sleep::At(at)) = self.set else { return Tick::Waiting };
        if now >= at {
            self.set = None;
            return Tick::Due { restore: self.end_fade() };
        }
        let left = at - now;
        if left >= SLEEP_FADE || !playing {
            return Tick::Waiting;
        }
        let level = fade_level(left);
        if self.fade == Some(level) {
            return Tick::Waiting;
        }
        self.fade = Some(level);
        Tick::Fade(level)
    }

    /// Stop fading; whether a fade was on.
    pub fn end_fade(&mut self) -> bool {
        self.fade.take().is_some()
    }

    /// Whether the music stops when `current` ends, with `next` after it (`last`:
    /// nothing comes after it in the queue): at the end of the track; of the album,
    /// when the next is from another (or none, or the queue starts over); of the
    /// queue, when there's nothing after it.
    pub fn stops_after(&self, current: Option<&Track>, next: Option<&Track>, last: bool) -> bool {
        match self.set {
            Some(Sleep::EndOfTrack) => true,
            Some(Sleep::EndOfAlbum) => {
                let album = |t: &Track| if t.album_id.is_empty() { t.album.clone() } else { t.album_id.clone() };
                last || match (current, next) {
                    (Some(now), Some(next)) => album(now).is_empty() || album(now) != album(next),
                    _ => true,
                }
            }
            Some(Sleep::EndOfQueue) => last,
            Some(Sleep::At(_)) | None => false,
        }
    }

    /// "track", "album", "queue": where a sleep at the end of something stops.
    pub fn end_of(&self) -> &'static str {
        match self.set {
            Some(Sleep::EndOfAlbum) => "album",
            Some(Sleep::EndOfQueue) => "queue",
            _ => "track",
        }
    }
}

/// The share of the volume `left` before a timed sleep, in whole fade steps.
fn fade_level(left: Duration) -> f32 {
    let steps = (SLEEP_FADE.as_secs_f32() * FADE_STEPS_PER_SEC).max(1.0);
    (left.as_secs_f32() / SLEEP_FADE.as_secs_f32() * steps).ceil() / steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(album_id: &str) -> Track {
        Track { album_id: album_id.into(), ..Track::test(1) }
    }

    #[test]
    fn the_end_of_an_album_is_where_the_next_track_is_from_another() {
        let timer = SleepTimer { set: Some(Sleep::EndOfAlbum), fade: None };
        assert!(!timer.stops_after(Some(&on("1")), Some(&on("1")), false), "same album");
        assert!(timer.stops_after(Some(&on("1")), Some(&on("2")), false));
        assert!(timer.stops_after(Some(&on("1")), Some(&on("1")), true), "the queue's end");
        let queue = SleepTimer { set: Some(Sleep::EndOfQueue), fade: None };
        assert!(!queue.stops_after(Some(&on("1")), Some(&on("2")), false) && queue.stops_after(None, None, true));
        assert!(!SleepTimer::default().stops_after(Some(&on("1")), None, true), "off");
    }

    #[test]
    fn a_timed_sleep_fades_in_steps_then_is_due() {
        let start = Instant::now();
        let mut timer = SleepTimer::default();
        timer.set(Some(Sleep::At(start + SLEEP_FADE)));
        assert_eq!(timer.tick(start, true), Tick::Waiting, "the fade hasn't started");
        let half = start + SLEEP_FADE / 2;
        assert_eq!(timer.tick(half, true), Tick::Fade(0.5));
        assert_eq!(timer.tick(half, true), Tick::Waiting, "no change, nothing sent");
        assert_eq!(timer.tick(half, false), Tick::Waiting, "paused: no fading");
        assert_eq!(timer.tick(start + SLEEP_FADE, true), Tick::Due { restore: true });
        assert_eq!(timer.get(), None);
    }
}
