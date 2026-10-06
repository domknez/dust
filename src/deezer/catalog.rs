//! Browsing the catalogue: search, the user's library, the home page and what its
//! items open (albums, artists, mixes, Flow).

use super::models::{ArtistPage, Item, Playlist, SearchResults, Section, Track};
use super::{Deezer, Result, parse};
use serde_json::json;

impl Deezer {
    pub fn search(&self, query: &str) -> Result<SearchResults> {
        let r = self.call(
            "deezer.pageSearch",
            json!({"query": query, "start": 0, "nb": 100, "suggest": false, "artist_suggest": false, "top_tracks": false}),
        )?;
        let sections = [("Artists", "artist", "ARTIST"), ("Albums", "album", "ALBUM"), ("Playlists", "playlist", "PLAYLIST")]
            .into_iter()
            .map(|(title, kind, key)| parse::search_section(title, kind, &r[key]["data"]))
            .filter(|s| !s.items.is_empty())
            .collect();
        Ok(SearchResults { tracks: parse::tracks(&r["TRACK"]["data"]), sections })
    }

    /// The account's "Loved tracks".
    pub fn loved(&self) -> Result<Vec<Track>> {
        self.playlist(self.0.loved_id)
    }

    pub fn playlist(&self, id: u64) -> Result<Vec<Track>> {
        let r = self.call(
            "deezer.pagePlaylist",
            json!({"playlist_id": id, "lang": "en", "nb": 2000, "start": 0, "tab": 0, "tags": true, "header": true}),
        )?;
        Ok(parse::tracks(&r["SONGS"]["data"]))
    }

    /// The account's playlists, without the Loved tracks playlist.
    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let r = self.call("deezer.pageProfile", json!({"user_id": self.0.user_id, "tab": "playlists", "nb": 500}))?;
        let list = r["TAB"]["playlists"]["data"].as_array().cloned().unwrap_or_default();
        Ok(list.iter().map(parse::playlist).filter(|p| p.id != 0 && p.id != self.0.loved_id).collect())
    }

    /// Personalised home page, as sections of items (what the web app shows).
    pub fn home(&self) -> Result<Vec<Section>> {
        let support = json!({
            "grid": ["channel", "album", "playlist", "flow", "smarttracklist", "artist"],
            "horizontal-grid": ["album", "playlist", "flow", "smarttracklist", "artist", "channel"],
            "large-card": ["album", "playlist"],
            "slideshow": ["album", "playlist"],
            "filterable-grid": ["flow"],
            "item-highlight": ["radio"],
        });
        let input = json!({"PAGE": "home", "VERSION": "2.5", "SUPPORT": support, "LANG": "en", "OPTIONS": []}).to_string();
        let r = self.call_with("page.get", json!({}), &[("gateway_input", &input)])?;
        let sections = r["sections"].as_array().cloned().unwrap_or_default();
        Ok(sections.iter().map(parse::section).filter(|s| !s.items.is_empty()).collect())
    }

    pub fn album(&self, id: &str) -> Result<Vec<Track>> {
        let r = self.call("deezer.pageAlbum", json!({"alb_id": id, "lang": "en", "header": true, "tab": 0}))?;
        Ok(parse::tracks(&r["SONGS"]["data"]))
    }

    /// Everything on an artist's page, including the whole discography.
    pub fn artist(&self, id: &str) -> Result<ArtistPage> {
        let page = self.call("deezer.pageArtist", json!({"art_id": id, "lang": "en", "tab": 0}))?;
        let discography =
            self.call("album.getDiscography", json!({"art_id": id, "nb": 500, "nb_songs": 0, "start": 0, "filter_role_id": [0]}))?;
        Ok(parse::artist_page(&page, &discography["data"]))
    }

    /// Artists the account follows, as cards.
    pub fn favorite_artists(&self) -> Result<Vec<Item>> {
        let r = self.call("deezer.pageProfile", json!({"user_id": self.0.user_id, "tab": "artists", "nb": 2000}))?;
        Ok(r["TAB"]["artists"]["data"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| parse::card_item("artist", v)).collect())
            .unwrap_or_default())
    }

    pub fn artist_top(&self, id: &str) -> Result<Vec<Track>> {
        let r = self.call("artist.getTopTrack", json!({"art_id": id, "nb": 100}))?;
        Ok(parse::tracks(&r["data"]))
    }

    /// A personal mix ("smart tracklist"), e.g. a daily mix or "New releases".
    pub fn mix(&self, id: &str) -> Result<Vec<Track>> {
        let r = self.call("smartTracklist.getSongs", json!({"smartTracklist_id": id}))?;
        Ok(parse::tracks(&r["data"]))
    }

    /// Next batch of Flow, optionally tuned to a mood/genre config (e.g. "chill").
    pub fn flow(&self, config: Option<&str>) -> Result<Vec<Track>> {
        let mut body = json!({"user_id": self.0.user_id});
        if let Some(c) = config {
            body["config_id"] = json!(c);
        }
        let r = self.call("radio.getUserRadio", body)?;
        Ok(parse::tracks(&r["data"]))
    }
}
