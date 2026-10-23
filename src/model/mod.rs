//! Source-neutral types shared by the music sources, the player and the UI.
//!
//! - [`error`]: what went wrong talking to a source, worded for the listener

mod error;

pub use error::{Error, Service};

pub type Result<T> = std::result::Result<T, Error>;
