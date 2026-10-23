//! Browsing the catalogue: search, the user's library, the home page and what its
//! items open (albums, artists, mixes, Flow).

use super::{Deezer, Error, Result, parse};
use crate::model::{AlbumInfo, ArtistPage, Item, Likeable, Likes, Playlist, PlaylistId, SearchResults, Section, Track, TrackId};
use serde_json::json;

impl Deezer {
    pub fn search(&self, query: &str) -> Result<SearchResults> {
        let r = self.call(
            "deezer.pageSearch",
            json!({"query": query, "start": 0, "nb": 100, "suggest": false, "artist_suggest": false, "top_tracks": false}),
        )?;
        let sections = [
            ("Artists", "artist", "ARTIST"),
            ("Albums", "album", "ALBUM"),
            ("Playlists", "playlist", "PLAYLIST"),
            ("Podcasts", "show", "SHOW"),
        ]
        .into_iter()
        .map(|(title, kind, key)| parse::search_section(title, kind, &r[key]["data"]))
        .filter(|s| !s.items.is_empty())
        .collect();
        Ok(SearchResults { tracks: parse::tracks(&r["TRACK"]["data"]), sections })
    }

    /// The account's "Loved tracks".
    pub fn loved(&self) -> Result<Vec<Track>> {
        self.playlist(&PlaylistId::from(self.0.loved_id))
    }

    pub fn playlist(&self, id: &PlaylistId) -> Result<Vec<Track>> {
        let r = self.call(
            "deezer.pagePlaylist",
            json!({"playlist_id": parse::id_value(id.as_str()), "lang": "en", "nb": 2000, "start": 0, "tab": 0, "tags": true, "header": true}),
        )?;
        Ok(parse::tracks(&r["SONGS"]["data"]))
    }

    /// The account's playlists, without the Loved tracks playlist.
    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let r = self.call("deezer.pageProfile", json!({"user_id": self.0.user_id, "tab": "playlists", "nb": 500}))?;
        let list = r["TAB"]["playlists"]["data"].as_array().cloned().unwrap_or_default();
        let (none, loved) = (PlaylistId::from(0), PlaylistId::from(self.0.loved_id));
        Ok(list.iter().map(|v| parse::playlist(v, self.0.user_id)).filter(|p| p.id != none && p.id != loved).collect())
    }

    /// Personalised home page, as sections of items (what the web app shows).
    pub fn home(&self) -> Result<Vec<Section>> {
        let support = json!({
            "grid": ["channel", "album", "playlist", "flow", "smarttracklist", "artist"],
            "horizontal-grid": ["album", "playlist", "flow", "smarttracklist", "artist", "channel"],
            // "Mixes inspired by…": tracks, each opening that track's mix.
            "horizontal-list": ["track", "album", "playlist", "artist"],
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
        self.album_page(id).map(|(tracks, _)| tracks)
    }

    /// An album's tracks with its year, label and the artist's other releases.
    pub fn album_page(&self, id: &str) -> Result<(Vec<Track>, AlbumInfo)> {
        let r = self.call("deezer.pageAlbum", json!({"alb_id": id, "lang": "en", "header": true, "tab": 0}))?;
        Ok((parse::tracks(&r["SONGS"]["data"]), parse::album_info(&r)))
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

    /// A podcast's latest episodes, newest first.
    pub fn show_episodes(&self, id: &str) -> Result<Vec<Track>> {
        let r = self.call("deezer.pageShow", json!({"show_id": id, "start": 0, "nb": 100, "user_id": 0, "lang": "en"}))?;
        Ok(parse::show_episodes(&r))
    }

    /// The podcasts the account follows, as cards.
    pub fn favorite_shows(&self) -> Result<Vec<Item>> {
        let r = self.call("deezer.pageProfile", json!({"user_id": self.0.user_id, "tab": "podcasts", "nb": 200}))?;
        Ok(r["TAB"]["shows"]["data"].as_array().map(|a| a.iter().filter_map(|v| parse::card_item("show", v)).collect()).unwrap_or_default())
    }

    /// A track's mix: similar songs, starting with that track ("Mixes inspired by…").
    pub fn track_mix(&self, id: &str) -> Result<Vec<Track>> {
        let r = self.call("song.getSearchTrackMix", json!({"sng_id": id, "start_with_input_track": true}))?;
        Ok(parse::tracks(&r["data"]))
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

impl Deezer {
    /// Ids of liked tracks, albums and followed artists.
    pub fn likes(&self) -> Result<Likes> {
        let songs = self.call("song.getFavoriteIds", json!({"nb": 10000, "start": 0}))?;
        let tracks = songs["data"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| parse::number(&s["SNG_ID"]))
            .filter(|&id| id != 0)
            .map(TrackId::from)
            .collect();
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
            (Likeable::Track(id), true) => ("favorite_song.add", json!({"SNG_ID": id.as_str()})),
            (Likeable::Track(id), false) => ("favorite_song.remove", json!({"SNG_ID": id.as_str()})),
            (Likeable::Album(id), true) => ("album.addFavorite", json!({"ALB_ID": id})),
            (Likeable::Album(id), false) => ("album.deleteFavorite", json!({"ALB_ID": id})),
            (Likeable::Artist(id), true) => ("artist.addFavorite", json!({"ART_ID": id})),
            (Likeable::Artist(id), false) => ("artist.deleteFavorite", json!({"ART_ID": id})),
        };
        match self.call(method, body) {
            // Already liked: the outcome we wanted.
            Err(Error::Api { message: e, .. }) if liked && e.contains("ERROR_DATA_EXISTS") => Ok(()),
            other => other.map(|_| ()),
        }
    }
}
