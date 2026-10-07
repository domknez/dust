//! Browsing the catalogue: search, the user's library, the home page and what its
//! items open (albums, artists, mixes, Flow).

use super::models::{ArtistPage, Item, Playlist, SearchResults, Section, Track};
use super::{Deezer, Error, Result, parse};
use serde_json::json;
use std::collections::HashSet;

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

/// Something the listener can like (Deezer: favourite tracks, albums, followed artists).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Likeable {
    Track(u64),
    Album(String),
    Artist(String),
}

/// Everything the account likes, by id.
#[derive(Clone, Debug, Default)]
pub struct Likes {
    pub tracks: HashSet<u64>,
    pub albums: HashSet<String>,
    pub artists: HashSet<String>,
}

impl Likes {
    pub fn contains(&self, what: &Likeable) -> bool {
        match what {
            Likeable::Track(id) => self.tracks.contains(id),
            Likeable::Album(id) => self.albums.contains(id),
            Likeable::Artist(id) => self.artists.contains(id),
        }
    }

    /// Record a like or unlike locally.
    pub fn set(&mut self, what: &Likeable, liked: bool) {
        fn apply<T: Eq + std::hash::Hash + Clone>(set: &mut HashSet<T>, id: &T, liked: bool) {
            if liked {
                set.insert(id.clone());
            } else {
                set.remove(id);
            }
        }
        match what {
            Likeable::Track(id) => apply(&mut self.tracks, id, liked),
            Likeable::Album(id) => apply(&mut self.albums, id, liked),
            Likeable::Artist(id) => apply(&mut self.artists, id, liked),
        }
    }
}

impl Deezer {
    /// Ids of liked tracks, albums and followed artists.
    pub fn likes(&self) -> Result<Likes> {
        let songs = self.call("song.getFavoriteIds", json!({"nb": 10000, "start": 0}))?;
        let tracks = songs["data"].as_array().into_iter().flatten().map(|s| parse::number(&s["SNG_ID"])).filter(|&id| id != 0).collect();
        let ids = |items: Vec<Item>| items.into_iter().map(|i| i.id).collect();
        Ok(Likes { tracks, albums: ids(self.favorite_albums()?), artists: ids(self.favorite_artists()?) })
    }

    /// Albums the account has added to its favourites, as cards.
    pub fn favorite_albums(&self) -> Result<Vec<Item>> {
        let r = self.call("deezer.pageProfile", json!({"user_id": self.0.user_id, "tab": "albums", "nb": 2000}))?;
        Ok(r["TAB"]["albums"]["data"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| parse::card_item("album", v)).collect())
            .unwrap_or_default())
    }

    /// Like or unlike on Deezer.
    pub fn set_liked(&self, what: &Likeable, liked: bool) -> Result<()> {
        let (method, body) = match (what, liked) {
            (Likeable::Track(id), true) => ("favorite_song.add", json!({"SNG_ID": id.to_string()})),
            (Likeable::Track(id), false) => ("favorite_song.remove", json!({"SNG_ID": id.to_string()})),
            (Likeable::Album(id), true) => ("album.addFavorite", json!({"ALB_ID": id})),
            (Likeable::Album(id), false) => ("album.deleteFavorite", json!({"ALB_ID": id})),
            (Likeable::Artist(id), true) => ("artist.addFavorite", json!({"ART_ID": id})),
            (Likeable::Artist(id), false) => ("artist.deleteFavorite", json!({"ART_ID": id})),
        };
        match self.call(method, body) {
            // Already liked: the outcome we wanted.
            Err(Error::Api(e)) if liked && e.contains("ERROR_DATA_EXISTS") => Ok(()),
            other => other.map(|_| ()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn likes_track_toggles() {
        let mut likes = Likes::default();
        let (song, album, artist) = (Likeable::Track(7), Likeable::Album("302127".into()), Likeable::Artist("27".into()));
        assert!(!likes.contains(&song));
        likes.set(&song, true);
        likes.set(&album, true);
        likes.set(&artist, true);
        assert!(likes.contains(&song) && likes.contains(&album) && likes.contains(&artist));
        likes.set(&album, false);
        assert!(!likes.contains(&album) && likes.contains(&song));
    }
}
