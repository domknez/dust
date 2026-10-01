//! Minimal Deezer client: a session from the `arl` cookie, the gw-light API for the
//! catalogue, media.deezer.com for stream URLs, and on-the-fly stripe decryption.
//!
//! - [`client`]: the [`Deezer`] handle, login and authenticated calls
//! - [`catalog`]: search, playlists, home page, albums, artists, mixes, Flow
//! - [`streaming`]: stream URL resolution (with regional fallbacks) and decrypted reads
//! - [`listens`]: listen reports (history, Flow, Last.fm scrobbling)
//! - [`models`] / [`parse`]: data types and their construction from API JSON
//! - [`crypto`]: the `BF_CBC_STRIPE` stream cipher
//! - [`prefetch`]: stall-proof network reads for streams
//! - [`session`]: cookies, CSRF token and the raw gw-light transport

mod catalog;
mod client;
mod crypto;
mod error;
mod listens;
mod models;
mod parse;
mod prefetch;
mod session;
mod streaming;

pub use client::Deezer;
pub use models::{Format, Item, Listen, Playlist, Quality, Section, Track, image_url};
pub use streaming::StreamSource;

pub use error::Error;

pub type Result<T> = std::result::Result<T, Error>;
