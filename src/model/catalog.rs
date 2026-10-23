//! What browsing shows: cards, sections of cards and the pages built from them.

use super::{Art, PlaylistId, Track};

/// One card on a page (home, search, an artist's page): a playlist, album, artist,
/// mix, flow, ...
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    /// The source's id for it, opened through the source again.
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub picture: Option<Art>,
}

/// What a card opens or plays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Album,
    Artist,
    Playlist,
    /// A list of songs picked for the listener (Deezer's daily mixes, discovery; a
    /// music server's random and artist mixes).
    Mix,
    /// Songs like a track, starting with it ("Mixes inspired by…").
    TrackMix,
    /// A podcast.
    Show,
    /// Deezer's endless Flow: plain, or tuned to a mood ("chill", ...).
    Flow {
        mood: Option<String>,
    },
    /// A music server's endless Radio: the whole library, or one genre.
    Radio {
        genre: Option<String>,
    },
    /// Something dust doesn't open (Deezer's channels, live streams, ...), by the
    /// source's name for it.
    Other(String),
}

impl ItemKind {
    /// Flow or Radio, which play endlessly instead of opening.
    pub fn is_endless(&self) -> bool {
        matches!(self, ItemKind::Flow { .. } | ItemKind::Radio { .. })
    }

    /// The mood or genre an endless card is tuned to; None for plain Flow / Radio
    /// and for other cards.
    pub fn tuning(&self) -> Option<&str> {
        match self {
            ItemKind::Flow { mood: tuning } | ItemKind::Radio { genre: tuning } => tuning.as_deref(),
            _ => None,
        }
    }
}

/// A titled row of cards.
#[derive(Clone, Debug)]
pub struct Section {
    pub title: String,
    pub items: Vec<Item>,
}

/// An artist's page: header, popular tracks and card sections (discography,
/// related artists, playlists).
#[derive(Clone, Debug, Default)]
pub struct ArtistPage {
    pub name: String,
    pub picture: Option<Art>,
    pub fans: u64,
    pub top: Vec<Track>,
    pub sections: Vec<Section>,
}

/// Search hits: tracks, plus artist, album, playlist and podcast card sections.
#[derive(Clone, Debug, Default)]
pub struct SearchResults {
    pub tracks: Vec<Track>,
    pub sections: Vec<Section>,
}

/// What a link pasted into search points at, with enough to show its page header
/// right away.
#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Album { id: String, title: String, artist: String, picture: Option<Art> },
    Artist { id: String },
    Playlist { id: PlaylistId, title: String, picture: Option<Art> },
}

/// What an album page shows besides its tracks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlbumInfo {
    /// Release year, empty if unknown.
    pub year: String,
    pub label: String,
    /// The artist's other releases (newest first), as album cards whose subtitle is
    /// the year.
    pub more: Vec<Item>,
}
