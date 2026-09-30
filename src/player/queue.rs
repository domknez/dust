//! Queue bookkeeping: the tracks, which one is current, and endless Flow. Pure
//! data manipulation — no audio, no network — so it is fully unit-testable.

use crate::deezer::Track;

/// Endless Flow: the queue refills itself from Deezer as it runs out.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    /// Mood/genre config ("chill", ...); None for plain Flow.
    pub mood: Option<String>,
}

impl Flow {
    /// Identifier the UI uses to highlight the active Flow tile.
    pub fn id(&self) -> String {
        self.mood.clone().unwrap_or_else(|| "default".into())
    }
}

/// What removing an entry did to the current track.
#[derive(Debug, PartialEq)]
pub enum Removed {
    Nothing,
    /// An entry other than the current one.
    Other,
    /// The current track; `current` now points at what took its place, if anything.
    Current { replaced: bool },
}

#[derive(Default)]
pub struct Queue {
    tracks: Vec<Track>,
    index: usize,
    pub flow: Option<Flow>,
    /// Changed since the UI last took a copy.
    dirty: bool,
}

impl Queue {
    pub fn current(&self) -> Option<&Track> {
        self.tracks.get(self.index)
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn has_next(&self) -> bool {
        self.index + 1 < self.tracks.len()
    }

    /// Copy for the UI if anything changed since the last call.
    pub fn take_snapshot(&mut self) -> Option<Vec<Track>> {
        std::mem::take(&mut self.dirty).then(|| self.tracks.clone())
    }

    pub fn replace(&mut self, tracks: Vec<Track>, index: usize) {
        self.tracks = tracks;
        self.index = index;
        self.flow = None;
        self.dirty = true;
    }

    /// Start endless Flow with an empty queue (filled by [`Queue::extend_unique`]).
    pub fn start_flow(&mut self, flow: Flow) {
        self.tracks.clear();
        self.index = 0;
        self.flow = Some(flow);
        self.dirty = true;
    }

    pub fn enqueue(&mut self, tracks: Vec<Track>) {
        if self.tracks.is_empty() {
            self.index = 0;
        }
        self.tracks.extend(tracks);
        self.dirty = true;
    }

    pub fn play_next(&mut self, tracks: Vec<Track>) {
        let at = if self.tracks.is_empty() { 0 } else { self.index + 1 };
        self.tracks.splice(at..at, tracks);
        self.dirty = true;
    }

    /// Append tracks not already queued (Flow batches overlap). Returns whether any were added.
    pub fn extend_unique(&mut self, tracks: Vec<Track>) -> bool {
        let before = self.tracks.len();
        for t in tracks {
            if !self.tracks.iter().any(|q| q.id == t.id) {
                self.tracks.push(t);
            }
        }
        self.dirty = true;
        self.tracks.len() > before
    }

    pub fn remove(&mut self, i: usize) -> Removed {
        if i >= self.tracks.len() {
            return Removed::Nothing;
        }
        self.tracks.remove(i);
        self.dirty = true;
        if i < self.index {
            self.index -= 1;
            Removed::Other
        } else if i == self.index {
            let replaced = self.index < self.tracks.len();
            if !replaced {
                self.index = self.tracks.len().saturating_sub(1);
            }
            Removed::Current { replaced }
        } else {
            Removed::Other
        }
    }

    pub fn move_entry(&mut self, from: usize, to: usize) {
        if from >= self.tracks.len() || to >= self.tracks.len() || from == to {
            return;
        }
        let track = self.tracks.remove(from);
        self.tracks.insert(to, track);
        self.index = index_after_move(self.index, from, to);
        self.dirty = true;
    }

    /// Make entry `i` current. Returns false if it doesn't exist.
    pub fn jump_to(&mut self, i: usize) -> bool {
        let valid = i < self.tracks.len();
        if valid {
            self.index = i;
        }
        valid
    }

    /// Move to the next entry. Returns false at the end of the queue.
    pub fn advance(&mut self) -> bool {
        let moved = self.has_next();
        if moved {
            self.index += 1;
        }
        moved
    }

    /// Move to the previous entry. Returns false at the start.
    pub fn back(&mut self) -> bool {
        let moved = self.index > 0;
        if moved {
            self.index -= 1;
        }
        moved
    }

    /// Drop everything after the current track and stop endless Flow.
    pub fn clear_upcoming(&mut self) {
        self.flow = None;
        self.tracks.truncate(self.index + 1);
        self.dirty = true;
    }
}

/// Where the current track ends up after moving the entry at `from` to `to`.
fn index_after_move(current: usize, from: usize, to: usize) -> usize {
    if current == from {
        to
    } else if from < current && to >= current {
        current - 1
    } else if from > current && to <= current {
        current + 1
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: u64) -> Track {
        Track { id, title: id.to_string(), artist: String::new(), album: String::new(), duration: 1, token: String::new(), cover: String::new(), fallback: None }
    }

    fn ids(q: &Queue) -> Vec<u64> {
        q.tracks.iter().map(|t| t.id).collect()
    }

    /// Simulate on a real Vec and compare with the bookkeeping.
    #[test]
    fn move_keeps_current_track() {
        for len in 1..6 {
            for current in 0..len {
                for from in 0..len {
                    for to in 0..len {
                        let mut v: Vec<usize> = (0..len).collect();
                        let x = v.remove(from);
                        v.insert(to, x);
                        let expected = v.iter().position(|&t| t == current).unwrap();
                        assert_eq!(index_after_move(current, from, to), expected, "len {len} cur {current} {from}->{to}");
                    }
                }
            }
        }
    }

    #[test]
    fn play_next_and_enqueue() {
        let mut q = Queue::default();
        q.enqueue(vec![t(1)]);
        q.play_next(vec![t(2)]);
        q.enqueue(vec![t(3)]);
        assert_eq!(ids(&q), [1, 2, 3]);
        assert_eq!(q.current().map(|t| t.id), Some(1));
    }

    #[test]
    fn remove_adjusts_current() {
        let mut q = Queue::default();
        q.replace((1..=4).map(t).collect(), 2);
        assert_eq!(q.remove(0), Removed::Other);
        assert_eq!(q.current().map(|t| t.id), Some(3));
        assert_eq!(q.remove(1), Removed::Current { replaced: true });
        assert_eq!(q.current().map(|t| t.id), Some(4));
        assert_eq!(q.remove(1), Removed::Current { replaced: false });
        assert_eq!(q.remove(9), Removed::Nothing);
    }

    #[test]
    fn flow_extends_without_duplicates() {
        let mut q = Queue::default();
        q.start_flow(Flow { mood: Some("chill".into()) });
        assert!(q.extend_unique(vec![t(1), t(2)]));
        assert!(!q.extend_unique(vec![t(2)]));
        assert_eq!(ids(&q), [1, 2]);
        q.clear_upcoming();
        assert_eq!(ids(&q), [1]);
        assert!(q.flow.is_none());
    }

    #[test]
    fn snapshot_only_when_changed() {
        let mut q = Queue::default();
        assert!(q.take_snapshot().is_none());
        q.enqueue(vec![t(1)]);
        assert_eq!(q.take_snapshot().map(|v| v.len()), Some(1));
        assert!(q.take_snapshot().is_none());
    }
}
