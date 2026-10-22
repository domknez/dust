//! Playback. The engine runs on its own thread and owns the queue, so playback
//! continues while the UI sleeps; the UI talks to it through [`PlayerHandle`].
//!
//! - [`engine`]: the playback loop and command handling
//! - [`queue`]: queue bookkeeping (pure, unit-tested)
//! - [`library`]: what the engine needs from a music source (a seam for tests)
//! - [`stream`]: opening and decoding a track (Deezer song, podcast, server file)
//! - [`listens`]: listen tracking and reporting to Deezer
//! - [`awake`]: keeping the OS from throttling dust while it plays

pub mod awake;
mod engine;
pub mod library;
mod listens;
mod queue;
mod resample;
pub mod saved;
pub mod stream;

use crate::deezer::{Quality, Track};
use crate::output::airplay::Device;
use crate::provider::Provider;
use eframe::egui;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

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
    /// continues from there. `flow`: Some(mood) when it was endless Flow.
    Restore {
        tracks: Vec<Track>,
        index: usize,
        position: f64,
        flow: Option<Option<String>>,
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
    /// Active Flow config ("default" for plain Flow), if playing Flow.
    pub flow: Option<String>,
    pub output: String,
    pub error: Option<String>,
}

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
        let _ = self.tx.send(cmd);
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
}
