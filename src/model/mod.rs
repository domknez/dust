//! Source-neutral types shared by the music sources, the player and the UI. Each
//! source builds these from its own API; what only one source reads (a Deezer
//! song's token, a server song's file type) sits in [`Origin`].
//!
//! - [`error`]: what went wrong talking to a source, worded for the listener
//! - [`id`]: track and playlist ids, as their source writes them
//! - [`art`]: pictures, and how the source turns them into URLs
//! - [`track`]: tracks, their artists and their [`Origin`]
//! - [`catalog`]: cards, sections and the pages made of them
//! - [`playlist`], [`likes`], [`lyrics`]: the account's playlists and likes, lyrics
//! - [`quality`]: the quality asked for and the format delivered
//! - [`listen`]: listen reports

mod art;
mod catalog;
mod error;
mod id;
mod likes;
mod listen;
mod lyrics;
mod playlist;
mod quality;
mod track;

pub use art::{Art, ArtResolver};
pub use catalog::{AlbumInfo, ArtistPage, Item, ItemKind, Link, SearchResults, Section};
pub use error::{Error, Service};
pub use id::{PlaylistId, TrackId};
pub use likes::{Likeable, Likes};
pub use listen::Listen;
pub use lyrics::{LyricLine, Lyrics};
pub use playlist::Playlist;
pub use quality::{Format, Quality};
pub use track::{ArtistRef, DeezerSong, Episode, Origin, ServerSong, Track};

pub type Result<T> = std::result::Result<T, Error>;
