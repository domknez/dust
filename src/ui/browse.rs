//! [`Browse`]: the page on screen, what it shows and the loads that fill it, and the
//! way back and forward.

use super::history::History;
use super::load::Load;
use super::state::{PlayMode, View};
use crate::model::{AlbumInfo, ArtistPage, Error, Item, ItemKind, Link, Page, Playlist, SearchResults, Section, Track};

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
    pub album_info: Option<AlbumInfo>,
    /// Artist, album and playlist cards of the current search.
    pub search_sections: Vec<Section>,
    /// The list's tracks, and for an album its year, label and the artist's other
    /// releases.
    pub tracks_load: Load<(Vec<Track>, Option<AlbumInfo>)>,
    pub search_load: Load<SearchResults>,
    /// A link pasted into search, being resolved.
    pub link_load: Load<Link>,
    /// The artist page being shown (for `View::Artist`).
    pub artist_page: Load<ArtistPage>,
    /// The page of sections being shown (for `View::Channel`).
    pub channel: Load<Page>,
    /// Discography tab on artist pages (albums, singles, live).
    pub discography_tab: usize,
    /// Tracks fetched to play straight from a card.
    pub quick_play: Load<(Vec<Track>, PlayMode)>,
    pub history: History,
}

impl Default for Browse {
    fn default() -> Self {
        Browse {
            view: View::Home,
            search: String::new(),
            searched: String::new(),
            tracks: Vec::new(),
            album_info: None,
            search_sections: Vec::new(),
            tracks_load: Load::Idle,
            search_load: Load::Idle,
            link_load: Load::Idle,
            artist_page: Load::Idle,
            channel: Load::Idle,
            discography_tab: 0,
            quick_play: Load::Idle,
            history: History::default(),
        }
    }
}

impl Browse {
    /// Empty the track list for a new page; the old page's loads are dropped, so a
    /// stale result never lands.
    pub fn clear_list(&mut self) {
        self.tracks.clear();
        self.album_info = None;
        self.search_sections.clear();
        self.tracks_load = Load::Idle;
        self.search_load = Load::Idle;
        self.link_load = Load::Idle;
    }

    /// A track list is on its way.
    pub fn loading_tracks(&self) -> bool {
        self.tracks_load.is_loading() || self.search_load.is_loading() || self.link_load.is_loading()
    }

    /// Why the track list (or the search, or the pasted link) didn't load.
    pub fn tracks_error(&self) -> Option<&str> {
        self.tracks_load.error().or(self.search_load.error()).or(self.link_load.error())
    }

    /// Collect finished loads into the page; `playlists` (the account's) are matched
    /// against a search. Returns a resolved link to open; errors (also kept with
    /// their loads) go to `failed`.
    pub fn poll(&mut self, playlists: &[Playlist], failed: &mut Vec<Error>) -> Option<Link> {
        match self.tracks_load.poll_take() {
            Some(Ok((tracks, info))) => {
                self.tracks = tracks;
                self.album_info = info;
            }
            Some(Err(e)) => failed.push(e),
            None => {}
        }
        match self.search_load.poll_take() {
            Some(Ok(results)) => {
                self.tracks = results.tracks;
                self.search_sections = results.sections;
                // The listener's own playlists first.
                if let Some(own) = own_playlists(playlists, &self.searched) {
                    self.search_sections.insert(0, own);
                }
            }
            Some(Err(e)) => failed.push(e),
            None => {}
        }
        failed.extend(self.artist_page.poll());
        failed.extend(self.channel.poll());
        match self.link_load.poll_take()? {
            Ok(link) => Some(link),
            Err(e) => {
                failed.push(e);
                None
            }
        }
    }
}

/// The account's playlists whose title contains `query` (case-insensitive), as a
/// search section; `None` when nothing matches.
fn own_playlists(playlists: &[Playlist], query: &str) -> Option<Section> {
    let query = query.trim().to_lowercase();
    let items: Vec<Item> = playlists
        .iter()
        .filter(|p| !query.is_empty() && p.title.to_lowercase().contains(&query))
        .map(|p| Item {
            kind: ItemKind::Playlist,
            id: p.id.to_string(),
            title: p.title.clone(),
            subtitle: format!("{} tracks", p.count),
            picture: p.picture.clone(),
        })
        .collect();
    (!items.is_empty()).then(|| Section::new("Your playlists", items))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn playlist(id: u64, title: &str) -> Playlist {
        Playlist { id: id.into(), title: title.into(), count: 3, owned: true, private: false, picture: None }
    }

    #[test]
    fn own_playlists_match_case_insensitively() {
        let mine = [playlist(1, "Rock Essentials"), playlist(2, "Rock & Chill"), playlist(3, "trip-hop mix")];
        let found = own_playlists(&mine, "  rock ").unwrap();
        assert_eq!(found.title, "Your playlists");
        assert_eq!(found.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["1", "2"]);
        assert_eq!(found.items[0].subtitle, "3 tracks");
        assert!(own_playlists(&mine, "jazz").is_none());
        assert!(own_playlists(&mine, "").is_none());
    }

    #[test]
    fn search_results_lead_with_own_playlists() {
        let mut browse = Browse { searched: "rock".into(), ..Default::default() };
        let (tx, rx) = channel();
        browse.search_load = Load::Loading(rx);
        assert!(browse.loading_tracks());
        tx.send(Ok(SearchResults::default())).unwrap();
        let mut failed = Vec::new();
        assert!(browse.poll(&[playlist(1, "Rock Essentials")], &mut failed).is_none());
        assert!(failed.is_empty() && !browse.loading_tracks());
        assert_eq!(browse.search_sections.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Your playlists"]);
    }

    #[test]
    fn a_new_page_drops_the_old_list_and_its_error() {
        let mut browse = Browse::default();
        let (tx, rx) = channel();
        browse.tracks_load = Load::Loading(rx);
        tx.send(Err(Error::Other("playlist gone".into()))).unwrap();
        let mut failed = Vec::new();
        browse.poll(&[], &mut failed);
        assert_eq!((failed.len(), browse.tracks_error()), (1, Some("playlist gone")));
        let (tx, rx) = channel();
        browse.tracks_load = Load::Loading(rx);
        browse.clear_list();
        assert!(tx.send(Ok((Vec::new(), None))).is_err(), "the old load is dropped");
        assert_eq!(browse.tracks_error(), None);
    }
}
