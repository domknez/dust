//! Getting the next track ready ahead of time, off the player thread: opening it
//! (an HTTP resolve and the first bytes) or, when endless Flow has run out, fetching
//! the next batch. The engine starts this near the end of a track and switches to
//! the opened track without waiting for the network.

use super::library::SharedLibrary;
use super::queue::Flow;
use crate::media::{OpenError, Playback};
use crate::model::{Quality, Track, TrackId};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// A preloaded track older than this isn't used: its connection may have gone idle
/// for too long (a paused player stops reading), so it is opened again.
const MAX_AGE: Duration = Duration::from_secs(30);

/// Work for a worker thread.
pub type Job = Box<dyn FnOnce() + Send>;
/// Runs a [`Job`] off the player thread: [`spawn_thread`] in the app; tests run
/// jobs inline or when they choose.
pub type Spawner = Box<dyn Fn(Job)>;

/// Run `job` on a thread of its own.
pub fn spawn_thread(job: Job) {
    // If the thread can't start, the job (and its sender) is dropped: the engine
    // sees a failed preload and opens the track the usual way.
    if let Err(e) = std::thread::Builder::new().name("preload".into()).spawn(job) {
        log_warn!("preload thread: {e}");
    }
}

pub enum Preload {
    /// Opening a track on a worker.
    Opening { id: TrackId, since: Instant, rx: Receiver<Result<Box<dyn Playback>, OpenError>> },
    /// Opened, waiting for the current track to end.
    Ready { id: TrackId, since: Instant, playback: Box<dyn Playback> },
    /// Fetching more of endless Flow on a worker.
    Refilling { flow: Flow, rx: Receiver<Result<Vec<Track>, String>> },
    /// Didn't work out (`id`: the track that wouldn't open; None: a refill). Not tried
    /// again: the end of the track does it the usual way, and reports the error.
    Failed { id: Option<TrackId> },
}

impl Preload {
    /// Start opening `track` from its start.
    pub fn open(spawn: &Spawner, library: SharedLibrary, track: &Track, quality: Quality) -> Preload {
        let (tx, rx) = mpsc::channel();
        let job_track = track.clone();
        log_debug!("preloading {} {} – {}", track.id, track.artist, track.title);
        spawn(Box::new(move || {
            let started = Instant::now();
            let opened = library.open(&job_track, quality, 0.0);
            log_opened(&job_track, 0.0, started, &opened);
            // Nobody listening: the preload was dropped, and the stream goes with it.
            let _ = tx.send(opened);
        }));
        Preload::Opening { id: track.id.clone(), since: Instant::now(), rx }
    }

    /// Start fetching the next batch of `flow`.
    pub fn refill(spawn: &Spawner, library: SharedLibrary, flow: Flow) -> Preload {
        let (tx, rx) = mpsc::channel();
        let mood = flow.mood.clone();
        log_debug!("fetching more Flow ahead of the end of the queue");
        spawn(Box::new(move || {
            let _ = tx.send(library.flow(mood.as_deref()));
        }));
        Preload::Refilling { flow, rx }
    }

    /// Whether this is (or was) the preload of track `id`.
    pub fn is_for(&self, id: &TrackId) -> bool {
        match self {
            Preload::Opening { id: own, .. } | Preload::Ready { id: own, .. } => own == id,
            Preload::Failed { id: own } => own.as_ref() == Some(id),
            Preload::Refilling { .. } => false,
        }
    }

    /// A worker is still at it.
    pub fn is_pending(&self) -> bool {
        matches!(self, Preload::Opening { .. } | Preload::Refilling { .. })
    }

    /// Pick up a worker's result. A finished refill hands back the Flow it was for
    /// and the tracks it fetched (and leaves this `Failed`, for the caller to clear
    /// if the tracks were of use).
    pub fn poll(&mut self) -> Option<(Flow, Vec<Track>)> {
        match self {
            Preload::Opening { id, since, rx } => match rx.try_recv() {
                Ok(Ok(playback)) => *self = Preload::Ready { id: id.clone(), since: *since, playback },
                Ok(Err(e)) => {
                    log_debug!("preloading {id} failed: {e}");
                    *self = Preload::Failed { id: Some(id.clone()) };
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => *self = Preload::Failed { id: Some(id.clone()) },
            },
            Preload::Refilling { flow, rx } => match rx.try_recv() {
                Ok(Ok(tracks)) => {
                    let flow = flow.clone();
                    *self = Preload::Failed { id: None };
                    return Some((flow, tracks));
                }
                Ok(Err(e)) => {
                    log_debug!("fetching more Flow ahead failed: {e}");
                    *self = Preload::Failed { id: None };
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => *self = Preload::Failed { id: None },
            },
            Preload::Ready { .. } | Preload::Failed { .. } => {}
        }
        None
    }

    /// The opened track, if it is ready and recent enough to use.
    pub fn take_ready(self) -> Option<(TrackId, Box<dyn Playback>)> {
        match self {
            Preload::Ready { id, since, playback } if since.elapsed() < MAX_AGE => Some((id, playback)),
            Preload::Ready { id, .. } => {
                log_debug!("preloaded {id} is too old to use; opening it again");
                None
            }
            _ => None,
        }
    }
}

/// Log how opening a track went (and how long it took).
pub fn log_opened(track: &Track, at: f64, started: Instant, opened: &Result<Box<dyn Playback>, OpenError>) {
    let ms = started.elapsed().as_millis();
    match opened {
        Ok(p) => log_info!("opened {} {} – {} at {at:.1} s in {ms} ms ({:?})", track.id, track.artist, track.title, p.format()),
        Err(e) => log_warn!("could not open {} {} – {} after {ms} ms: {e}", track.id, track.artist, track.title),
    }
}
