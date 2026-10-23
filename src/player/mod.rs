//! Playback. The engine runs on its own thread and owns the queue, so playback
//! continues while the UI sleeps; the UI talks to it through [`PlayerHandle`].
//!
//! - [`engine`]: the playback loop and command handling
//! - [`queue`]: queue bookkeeping (pure, unit-tested)
//! - [`library`]: what the engine needs from a music source (a seam for tests)
//! - [`listens`]: listen tracking and reporting to Deezer
//! - [`awake`]: keeping the OS from throttling dust while it plays

pub mod awake;
mod engine;
pub mod library;
mod listens;
mod queue;
pub mod saved;

pub use queue::Flow;

use crate::model::{Quality, Track};
use crate::output::airplay::Device;
use crate::provider::Provider;
use eframe::egui;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, PoisonError};

/// Where audio goes.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    Local,
    AirPlay(Device),
}

impl Output {
    pub fn name(&self) -> &str {
        match self {
            Output::Local => "This computer",
            Output::AirPlay(d) => &d.name,
        }
    }
}

/// What happens when a track ends.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Repeat {
    #[default]
    Off,
    /// Start the queue over after the last track.
    All,
    /// Play the current track again.
    One,
}

impl Repeat {
    /// The next mode for the repeat button: off → all → one → off.
    pub fn next(self) -> Repeat {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Repeat::Off => "off",
            Repeat::All => "all",
            Repeat::One => "one",
        }
    }

    pub fn from_key(key: &str) -> Repeat {
        match key {
            "all" => Repeat::All,
            "one" => Repeat::One,
            _ => Repeat::Off,
        }
    }
}

/// Requests from the UI (and the speaker's remote control) to the engine.
pub enum Cmd {
    /// The music source to play from; a different source than before stops
    /// playback and empties the queue.
    Client(Provider),
    Quality(Quality),
    /// Report listens to Deezer (history, Flow, Last.fm scrobbling).
    ReportListens(bool),
    Output(Output),

    /// Replace the queue and play from an index.
    Play(Vec<Track>, usize),
    /// Endless Flow, optionally tuned to a mood/genre config (e.g. "chill").
    PlayFlow(Option<String>),
    Toggle,
    Resume,
    Pause,
    Next,
    Prev,
    Seek(f64),
    Volume(f32),
    VolumeStep(f32),
    /// Shuffle the upcoming tracks, or put the queue back in order.
    Shuffle(bool),
    Repeat(Repeat),
    /// Set or cancel the sleep timer.
    Sleep(Option<Sleep>),

    /// Append to the end of the queue.
    Enqueue(Vec<Track>),
    /// Insert right after the current track.
    PlayNext(Vec<Track>),
    /// Remove the queue entry at an index.
    Remove(usize),
    /// Move a queue entry from one index to another.
    Move(usize, usize),
    /// Play the queue entry at an index.
    JumpTo(usize),
    /// Drop everything after the current track (and stop endless Flow).
    ClearUpcoming,
    /// Put back a queue saved by an earlier session, paused at a position; play
    /// continues from there. `flow`: set when it was endless Flow.
    Restore {
        tracks: Vec<Track>,
        index: usize,
        position: f64,
        flow: Option<Flow>,
    },
}

/// A timed sleep fades the music out over this long before it pauses.
pub const SLEEP_FADE: std::time::Duration = std::time::Duration::from_secs(30);

/// When the sleep timer stops the music.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sleep {
    /// Fade out over [`SLEEP_FADE`] and pause at this moment.
    At(std::time::Instant),
    /// Stop when the playing track ends (the next one waits, paused).
    EndOfTrack,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum State {
    #[default]
    Stopped,
    Loading,
    Playing,
    Paused,
}

/// Snapshot of the engine for the UI.
#[derive(Clone, Default)]
pub struct Status {
    pub state: State,
    pub track: Option<Track>,
    pub queue: Arc<Vec<Track>>,
    /// Index of the current track in `queue`.
    pub index: usize,
    pub position: f64,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub sleep: Option<Sleep>,
    /// The endless Flow being played, if any.
    pub flow: Option<Flow>,
    pub output: String,
    pub error: Option<String>,
}

/// Shown when the player thread has died.
const PLAYER_GONE: &str = "Playback stopped unexpectedly; restart dust";

/// The UI's side of the player: send commands, read status.
#[derive(Clone)]
pub struct PlayerHandle {
    tx: Sender<Cmd>,
    status: Arc<Mutex<Status>>,
}

impl PlayerHandle {
    pub fn spawn(ctx: egui::Context, volume: f32) -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status { output: Output::Local.name().into(), volume, ..Default::default() }));
        let shared = status.clone();
        // The audio device handle isn't Send, so the engine is built on its own thread.
        std::thread::Builder::new()
            .name("player".into())
            .spawn(move || engine::Engine::new(rx, shared, ctx, volume).run())
            .expect("spawn player");
        Self { tx, status }
    }

    pub fn send(&self, cmd: Cmd) {
        if self.tx.send(cmd).is_err() {
            // The player thread is gone (it panicked): say so rather than ignore
            // every click while the last status stays on screen.
            let mut st = self.status.lock().unwrap_or_else(PoisonError::into_inner);
            if st.error.as_deref() != Some(PLAYER_GONE) {
                log_error!("player thread is gone");
                st.error = Some(PLAYER_GONE.into());
                st.state = State::Stopped;
            }
        }
    }

    /// A handle with no player behind it: commands wait in the returned receiver,
    /// and the status is what the test sets.
    #[cfg(test)]
    pub fn detached() -> (Self, mpsc::Receiver<Cmd>) {
        let (tx, rx) = mpsc::channel();
        (Self { tx, status: Arc::default() }, rx)
    }

    #[cfg(test)]
    pub fn set_status(&self, change: impl FnOnce(&mut Status)) {
        change(&mut self.status.lock().unwrap());
    }

    pub fn status(&self) -> Status {
        // A player thread that panicked while publishing leaves the lock poisoned;
        // the snapshot in it is still the last one.
        self.status.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dead_player_shows_an_error() {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status { state: State::Playing, ..Default::default() }));
        let player = PlayerHandle { tx, status };
        drop(rx); // the player thread ended
        player.send(Cmd::Toggle);
        let st = player.status();
        assert_eq!(st.error.as_deref(), Some(PLAYER_GONE));
        assert_eq!(st.state, State::Stopped);
    }
}
