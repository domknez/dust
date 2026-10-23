//! The engine's view of where music comes from. The music sources implement it in
//! the app (through [`crate::provider::Provider`]); tests use fakes, so the playback
//! logic runs without network or audio hardware.

use crate::media::{OpenError, Playback};
use crate::model::{Listen, Quality, Track};
use std::sync::Arc;

/// Opens tracks and Flow, and receives listen reports.
pub trait Library: Send + Sync {
    /// Start decoding `track` at `at` seconds.
    fn open(&self, track: &Track, quality: Quality, at: f64) -> Result<Box<dyn Playback>, OpenError>;
    /// Next batch of Flow, optionally tuned to a mood.
    fn flow(&self, mood: Option<&str>) -> Result<Vec<Track>, String>;
    /// Fire-and-forget: a track started.
    fn report_listen_start(&self, song_id: u64);
    /// Fire-and-forget: a track finished or was skipped.
    fn report_listen(&self, listen: Listen);
}

/// Shared handle the engine keeps.
pub type SharedLibrary = Arc<dyn Library>;
