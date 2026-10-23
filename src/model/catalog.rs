//! What browsing shows: cards, sections of cards and the pages built from them.

use super::{Art, Track};

/// One card on a page (home, search, an artist's page): a playlist, album, artist,
/// mix, flow, ...
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub picture: Option<Art>,
}

/// A titled row of cards.
#[derive(Clone, Debug)]
pub struct Section {
    pub title: String,
    pub layout: String,
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
