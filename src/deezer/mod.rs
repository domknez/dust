//! Minimal Deezer client: a session from the `arl` cookie, the gw-light API for the
//! catalogue, media.deezer.com for stream URLs, and on-the-fly stripe decryption.
//!
//! - [`client`]: the [`Deezer`] handle, login and authenticated calls
//! - [`catalog`]: search, playlists, home page, albums, artists, mixes, Flow
//! - [`streaming`]: stream URL resolution (with regional fallbacks) and decrypted reads
//! - [`playlists`]: creating and editing the account's own playlists
//! - [`lyrics`]: synced and plain song lyrics
//! - [`listens`]: listen reports (history, Flow, Last.fm scrobbling)
//! - [`models`] / [`parse`]: data types and their construction from API JSON
//! - [`crypto`]: the `BF_CBC_STRIPE` stream cipher
//! - [`playback`]: opening songs and episodes for the player
//! - [`links`]: Deezer links pasted into search
//! - [`session`]: cookies, CSRF token and the raw gw-light transport

mod catalog;
mod client;
mod crypto;
mod links;
mod listens;
mod lyrics;
mod models;
mod parse;
mod playback;
mod playlists;
pub(crate) mod session;
mod streaming;

pub use catalog::{Likeable, Likes};
pub use client::Deezer;
pub use links::{Link, is_link};
pub use lyrics::{LyricLine, Lyrics};
pub use models::{
    AlbumInfo, ArtistPage, ArtistRef, Episode, Format, Item, Listen, Playlist, Quality, SearchResults, Section, ServerSong, Track,
    image_url,
};
pub use parse::thousands;

// Child modules use these as `super::{Error, Result}`.
use crate::model::{Error, Result, Service};

/// A failed request to Deezer, as an error naming Deezer.
fn http(e: ureq::Error) -> Error {
    Service::Deezer.http(e)
}

/// Deezer answered with an error.
fn api(message: impl Into<String>) -> Error {
    Service::Deezer.api(message)
}
