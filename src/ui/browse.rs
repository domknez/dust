//! [`Browse`]: the page on screen, what it shows and the loads that fill it, and the
//! way back and forward.

use super::history::History;
use super::state::{PlayMode, View};
use super::tasks::Task;
use crate::deezer::{AlbumInfo, ArtistPage, Link, SearchResults, Section, Track};

/// The page in the main area and its content. Replaced wholesale along with the
/// [`Library`](super::library::Library) when the source changes.
pub struct Browse {
    pub view: View,
    /// The search field's text.
    pub search: String,
    /// The query the results on screen are for.
    pub searched: String,
    /// The track list on screen (a collection, Loved tracks or search results).
    pub tracks: Vec<Track>,
    /// The list's tracks, and for an album its year, label and the artist's other
    /// releases.
    pub tracks_task: Task<(Vec<Track>, Option<AlbumInfo>)>,
    pub album_info: Option<AlbumInfo>,
    /// Artist, album and playlist cards of the current search.
    pub search_sections: Vec<Section>,
    pub search_task: Task<SearchResults>,
    /// A Deezer link pasted into search, being resolved.
    pub link_task: Task<Link>,
    pub list_error: Option<String>,
    /// The artist page being shown (for `View::Artist`), once loaded.
    pub artist_page: Option<ArtistPage>,
    pub artist_task: Task<ArtistPage>,
    /// Discography tab on artist pages (albums, singles, live).
    pub discography_tab: usize,
    /// Tracks fetched to play straight from a card.
    pub quick_play: Task<(Vec<Track>, PlayMode)>,
    pub history: History,
}

impl Default for Browse {
    fn default() -> Self {
        Browse {
            view: View::Home,
            search: String::new(),
            searched: String::new(),
            tracks: Vec::new(),
            tracks_task: None,
            album_info: None,
            search_sections: Vec::new(),
            search_task: None,
            link_task: None,
            list_error: None,
            artist_page: None,
            artist_task: None,
            discography_tab: 0,
            quick_play: None,
            history: History::default(),
        }
    }
}

impl Browse {
    /// A track list is on its way.
    pub fn loading_tracks(&self) -> bool {
        self.tracks_task.is_some() || self.search_task.is_some() || self.link_task.is_some()
    }
}
