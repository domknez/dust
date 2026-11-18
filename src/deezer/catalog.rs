//! Browsing the catalogue: search, the user's library, the home page and what its
//! items open (albums, artists, mixes, Flow).

use super::{Deezer, Result, parse};
use crate::model::{
    AlbumInfo, ArtistPage, Item, Likeable, Likes, Page, Playlist, PlaylistId, Release, SearchResults, Section, Track, TrackId,
};
use serde_json::json;

/// The home page's name for `page.get`.
const HOME: &str = "home";

/// Most of each list asked for at once (what Deezer's web player asks for, or
/// more); a list that comes back this long may have been cut, which the log says.
const SEARCH_HITS: usize = 100;
const PLAYLIST_TRACKS: usize = 2000;
const OWN_PLAYLISTS: usize = 500;
const DISCOGRAPHY: usize = 500;
const FOLLOWED: usize = 2000;
const FOLLOWED_SHOWS: usize = 200;
const EPISODES: usize = 100;
const TOP_TRACKS: usize = 100;
const LIKED_TRACKS: usize = 10_000;

/// `list`, noting in the log when it came back as long as asked for (`limit`): the
/// rest, if any, isn't shown.
fn capped<T>(what: &str, list: Vec<T>, limit: usize) -> Vec<T> {
    if list.len() >= limit {
        log_warn!("{what}: showing the first {limit}; there may be more");
    }
    list
}

/// Songs per `song.getListData` call.
const SONGS_PER_CALL: usize = 100;

impl Deezer {
    pub fn search(&self, query: &str) -> Result<SearchResults> {
        let r = self.call(
            "deezer.pageSearch",
            json!({"query": query, "start": 0, "nb": SEARCH_HITS, "suggest": false, "artist_suggest": false, "top_tracks": false}),
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
            json!({"playlist_id": parse::id_value(id.as_str()), "lang": "en", "nb": PLAYLIST_TRACKS, "start": 0, "tab": 0, "tags": true, "header": true}),
        )?;
        Ok(capped("playlist", parse::tracks(&r["SONGS"]["data"]), PLAYLIST_TRACKS))
    }

    /// The account's playlists, without the Loved tracks playlist.
    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let r = self.call("deezer.pageProfile", json!({"user_id": self.0.user_id, "tab": "playlists", "nb": OWN_PLAYLISTS}))?;
        let list = capped("playlists", r["TAB"]["playlists"]["data"].as_array().cloned().unwrap_or_default(), OWN_PLAYLISTS);
        let (none, loved) = (PlaylistId::from(0), PlaylistId::from(self.0.loved_id));
        Ok(list.iter().map(|v| parse::playlist(v, self.0.user_id)).filter(|p| p.id != none && p.id != loved).collect())
    }

    /// Personalised home page, as sections of items (what the web app shows).
    pub fn home(&self) -> Result<Vec<Section>> {
        self.page(HOME).map(|page| page.sections)
    }

    /// A page of sections: Home, Explore ("channels/explore"), a channel
    /// ("channels/rock") or a section's "See all" ("channels/module/<id>").
    pub fn page(&self, page: &str) -> Result<Page> {
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
        let input = json!({"PAGE": page, "VERSION": "2.5", "SUPPORT": support, "LANG": "en", "OPTIONS": []}).to_string();
        let r = self.call_with("page.get", json!({}), &[("gateway_input", &input)])?;
        let sections = r["sections"].as_array().cloned().unwrap_or_default();
        let sections = sections.iter().map(parse::section).filter(|s| !s.items.is_empty()).collect();
        Ok(Page { title: parse::text(&r["title"]), sections })
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
        Ok(parse::artist_page(&page, &self.discography(id)?))
    }

    /// An artist's albums, singles and EPs with their release dates.
    pub fn releases(&self, id: &str) -> Result<Vec<Release>> {
        Ok(parse::releases(&self.discography(id)?))
    }

    /// Everything an artist released as the main artist, as `album.getDiscography`
    /// lists it.
    fn discography(&self, id: &str) -> Result<serde_json::Value> {
        let r =
            self.call("album.getDiscography", json!({"art_id": id, "nb": DISCOGRAPHY, "nb_songs": 0, "start": 0, "filter_role_id": [0]}))?;
        Ok(r["data"].clone())
    }

    /// Artists the account follows, as cards.
    pub fn favorite_artists(&self) -> Result<Vec<Item>> {
        let r = self.call("deezer.pageProfile", json!({"user_id": self.0.user_id, "tab": "artists", "nb": FOLLOWED}))?;
        let artists = r["TAB"]["artists"]["data"].as_array().map(|a| a.iter().filter_map(|v| parse::card_item("artist", v)).collect());
        Ok(capped("followed artists", artists.unwrap_or_default(), FOLLOWED))
    }

    /// A podcast's latest episodes, newest first.
    pub fn show_episodes(&self, id: &str) -> Result<Vec<Track>> {
        let r = self.call("deezer.pageShow", json!({"show_id": id, "start": 0, "nb": EPISODES, "user_id": 0, "lang": "en"}))?;
        Ok(parse::show_episodes(&r))
    }

    /// The podcasts the account follows, as cards.
    pub fn favorite_shows(&self) -> Result<Vec<Item>> {
        let r = self.call("deezer.pageProfile", json!({"user_id": self.0.user_id, "tab": "podcasts", "nb": FOLLOWED_SHOWS}))?;
        Ok(r["TAB"]["shows"]["data"].as_array().map(|a| a.iter().filter_map(|v| parse::card_item("show", v)).collect()).unwrap_or_default())
    }

    /// A track's mix: similar songs, starting with that track ("Mixes inspired by…").
    pub fn track_mix(&self, id: &str) -> Result<Vec<Track>> {
        let r = self.call("song.getSearchTrackMix", json!({"sng_id": id, "start_with_input_track": true}))?;
        Ok(parse::tracks(&r["data"]))
    }

    /// The playable tracks among `ids`, in their order ([`SONGS_PER_CALL`] per call).
    pub fn tracks(&self, ids: &[TrackId]) -> Result<Vec<Track>> {
        let mut found = Vec::with_capacity(ids.len());
        for batch in ids.chunks(SONGS_PER_CALL) {
            let ids: Vec<_> = batch.iter().map(|id| parse::id_value(id.as_str())).collect();
            let songs = parse::tracks(&self.call("song.getListData", json!({"sng_ids": ids}))?["data"]);
            found.extend(songs.into_iter().filter(|t| t.available));
        }
        Ok(found)
    }

    pub fn artist_top(&self, id: &str) -> Result<Vec<Track>> {
        let r = self.call("artist.getTopTrack", json!({"art_id": id, "nb": TOP_TRACKS}))?;
        Ok(parse::tracks(&r["data"]))
    }

    /// Songs by an artist and ones like them (the web app's artist mix).
    pub fn artist_mix(&self, id: &str) -> Result<Vec<Track>> {
        let r = self.call("smart.getSmartRadio", json!({"art_id": id}))?;
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
        let songs = self.call("song.getFavoriteIds", json!({"nb": LIKED_TRACKS, "start": 0}))?;
        let ids: Vec<TrackId> = songs["data"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| parse::number(&s["SNG_ID"]))
            .filter(|&id| id != 0)
            .map(TrackId::from)
            .collect();
        let tracks = capped("liked tracks", ids, LIKED_TRACKS).into_iter().collect();
        let ids = |items: Vec<Item>| items.into_iter().map(|i| i.id).collect();
        Ok(Likes { tracks, albums: ids(self.favorite_albums()?), artists: ids(self.favorite_artists()?) })
    }

    /// Albums the account has added to its favourites, as cards.
    pub fn favorite_albums(&self) -> Result<Vec<Item>> {
        let r = self.call("deezer.pageProfile", json!({"user_id": self.0.user_id, "tab": "albums", "nb": FOLLOWED}))?;
        let albums = r["TAB"]["albums"]["data"].as_array().map(|a| a.iter().filter_map(|v| parse::card_item("album", v)).collect());
        Ok(capped("liked albums", albums.unwrap_or_default(), FOLLOWED))
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
            Err(e) if liked && e.has_code("ERROR_DATA_EXISTS") => Ok(()),
            other => other.map(|_| ()),
        }
    }
}
