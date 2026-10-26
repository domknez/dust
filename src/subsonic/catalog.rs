//! Browsing a music server: home shelves, albums, artists, playlists, search, likes
//! (stars), lyrics, mixes and play reports.

use super::Subsonic;
use super::parse::{self, list, text};
use crate::model::{
    AlbumInfo, ArtistPage, Error, Item, ItemKind, Likeable, Likes, Lyrics, Playlist, PlaylistId, Result, SearchResults, Section, Track,
    TrackId,
};
use serde_json::Value;

/// Cards per home shelf.
const SHELF: &str = "20";
/// Songs in a mix.
const MIX: &str = "50";
/// Page size when listing the whole library.
const PAGE: usize = 500;
/// Mix ids on the home page: songs like an artist's (anything else: random songs).
/// The ids are the server's own; only [`Subsonic::mix`] reads them.
const ARTIST_MIX: &str = "artist:";
/// Genres offered on the Radio panel, the ones with the most songs first.
const RADIO_GENRES: usize = 12;
/// A genre needs this many songs to get its own Radio station.
const RADIO_MIN_SONGS: u64 = 10;

impl Subsonic {
    /// Home: the Radio panel with the library's genres, then shelves of albums
    /// (recently added, played, most played, favourites, random) and mixes.
    pub fn home(&self) -> Result<Vec<Section>> {
        let shelves = [
            ("newest", "Recently added"),
            ("recent", "Recently played"),
            ("frequent", "Most played"),
            ("starred", "Favorite albums"),
            ("random", "Random albums"),
        ];
        // One request per shelf, side by side.
        let lists: Vec<Result<Value>> = std::thread::scope(|s| {
            let jobs: Vec<_> = shelves.iter().map(|(kind, _)| s.spawn(move || self.album_values(kind, SHELF, "0"))).collect();
            jobs.into_iter().map(|j| j.join().unwrap_or_else(|_| Err(Error::Other("home shelf failed".into())))).collect()
        });
        let lists = lists.into_iter().collect::<Result<Vec<Value>>>()?;
        // Mixes around the artists played most, else those added last.
        let mix_source = [2, 1, 0].into_iter().map(|i| &lists[i]).find(|l| !list(l, "album").is_empty());
        let mut sections: Vec<Section> = shelves
            .iter()
            .zip(&lists)
            .map(|((_, title), albums)| Section::new(*title, parse::albums(list(albums, "album"))))
            .filter(|s| !s.items.is_empty())
            .collect();
        let mixes = Section::new("Mixes", mix_cards(mix_source));
        if !mixes.items.is_empty() {
            sections.insert(sections.len().min(1), mixes);
        }
        // Without genres the panel still plays the whole library.
        let genres = self.call("getGenres", &[]).unwrap_or_default();
        sections.insert(0, Section::new("Radio", radio_stations(&genres)));
        Ok(sections)
    }

    /// One page of `getAlbumList2` (its `albumList2` object).
    fn album_values(&self, kind: &str, size: &str, offset: &str) -> Result<Value> {
        let mut r = self.call("getAlbumList2", &[("type", kind), ("size", size), ("offset", offset)])?;
        Ok(r["albumList2"].take())
    }

    fn album_list(&self, kind: &str, size: &str, offset: &str) -> Result<Vec<Item>> {
        Ok(parse::albums(list(&self.album_values(kind, size, offset)?, "album")))
    }

    pub fn album(&self, id: &str) -> Result<Vec<Track>> {
        let r = self.call("getAlbum", &[("id", id)])?;
        Ok(parse::songs(list(&r["album"], "song")))
    }

    /// An album's tracks, year and label, and the artist's other releases.
    pub fn album_page(&self, id: &str) -> Result<(Vec<Track>, AlbumInfo)> {
        let r = self.call("getAlbum", &[("id", id)])?;
        let album = &r["album"];
        let tracks = parse::songs(list(album, "song"));
        let year = album["year"].as_u64().filter(|&y| y > 0).map(|y| y.to_string()).unwrap_or_default();
        let label = list(album, "recordLabels").first().map(|l| text(&l["name"])).unwrap_or_default();
        let artist = text(&album["artistId"]);
        let more = if artist.is_empty() {
            Vec::new()
        } else {
            self.call("getArtist", &[("id", &artist)])
                .map(|r| parse::discography(&r["artist"]).into_iter().flat_map(|s| s.items).filter(|i| i.id != id).collect())
                .unwrap_or_default()
        };
        // Album cards on the album page show just the year.
        let more =
            more.into_iter().map(|i| Item { subtitle: i.subtitle.split(" · ").next().unwrap_or_default().to_string(), ..i }).collect();
        Ok((tracks, AlbumInfo { year, label, more }))
    }

    /// Artist page: popular songs, discography, similar artists.
    pub fn artist(&self, id: &str) -> Result<ArtistPage> {
        let r = self.call("getArtist", &[("id", id)])?;
        let artist = &r["artist"];
        let name = text(&artist["name"]);
        let mut sections = parse::discography(artist);
        if let Ok(info) = self.call("getArtistInfo2", &[("id", id), ("count", "12")]) {
            let similar = parse::artists_items(list(&info["artistInfo2"], "similarArtist"));
            if !similar.is_empty() {
                sections.push(Section::new("Similar artists", similar));
            }
        }
        let top = self.top_songs(&name, artist).unwrap_or_default();
        let picture = parse::cover(artist);
        Ok(ArtistPage { name, picture, fans: 0, top, sections })
    }

    /// An artist's popular songs (needs Last.fm on the server); otherwise songs from
    /// their newest albums.
    fn top_songs(&self, name: &str, artist: &Value) -> Result<Vec<Track>> {
        let r = self.call("getTopSongs", &[("artist", name), ("count", "20")])?;
        let top = parse::songs(list(&r["topSongs"], "song"));
        if !top.is_empty() {
            return Ok(top);
        }
        let mut albums = list(artist, "album");
        albums.sort_by_key(|a| std::cmp::Reverse(a["year"].as_u64().unwrap_or(0)));
        let mut songs = Vec::new();
        for album in albums.into_iter().take(3) {
            songs.extend(self.album(&text(&album["id"]))?);
            if songs.len() >= 10 {
                break;
            }
        }
        songs.truncate(10);
        Ok(songs)
    }

    /// Played straight from an artist card: their popular songs.
    pub fn artist_top(&self, id: &str) -> Result<Vec<Track>> {
        let r = self.call("getArtist", &[("id", id)])?;
        let artist = &r["artist"];
        self.top_songs(&text(&artist["name"]), artist)
    }

    /// Every artist on the server (the Artists page).
    pub fn favorite_artists(&self) -> Result<Vec<Item>> {
        let r = self.call("getArtists", &[])?;
        Ok(list(&r["artists"], "index").into_iter().flat_map(|i| parse::artists_items(list(i, "artist"))).collect())
    }

    /// Every album on the server, by name (the Albums page).
    pub fn favorite_albums(&self) -> Result<Vec<Item>> {
        let mut all = Vec::new();
        loop {
            let page = self.album_list("alphabeticalByName", &PAGE.to_string(), &all.len().to_string())?;
            let done = page.len() < PAGE;
            all.extend(page);
            if done || all.len() >= 20 * PAGE {
                return Ok(all);
            }
        }
    }

    fn starred(&self) -> Result<Value> {
        Ok(self.call("getStarred2", &[])?["starred2"].take())
    }

    /// Starred songs ("Loved tracks").
    pub fn loved(&self) -> Result<Vec<Track>> {
        let starred = self.starred()?;
        Ok(parse::songs(list(&starred, "song")))
    }

    /// Ids of starred songs, albums and artists.
    pub fn likes(&self) -> Result<Likes> {
        let starred = self.starred()?;
        let ids = |key: &str| list(&starred, key).into_iter().map(|v| text(&v["id"])).filter(|i| !i.is_empty());
        let tracks = ids("song").map(TrackId::new).collect();
        Ok(Likes { tracks, albums: ids("album").collect(), artists: ids("artist").collect() })
    }

    /// Star or unstar.
    pub fn set_liked(&self, what: &Likeable, liked: bool) -> Result<()> {
        let method = if liked { "star" } else { "unstar" };
        match what {
            Likeable::Track(id) => self.call(method, &[("id", id.as_str())]),
            Likeable::Album(id) => self.call(method, &[("albumId", id)]),
            Likeable::Artist(id) => self.call(method, &[("artistId", id)]),
        }
        .map(drop)
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let r = self.call("getPlaylists", &[])?;
        Ok(parse::playlists(&r, self.name()))
    }

    pub fn playlist(&self, id: &PlaylistId) -> Result<Vec<Track>> {
        let r = self.call("getPlaylist", &[("id", id.as_str())])?;
        Ok(parse::songs(list(&r["playlist"], "entry")))
    }

    pub fn search(&self, query: &str) -> Result<SearchResults> {
        let r = self.call("search3", &[("query", query), ("songCount", "100"), ("albumCount", "40"), ("artistCount", "20")])?;
        let found = &r["searchResult3"];
        let sections = [("Artists", parse::artists_items(list(found, "artist"))), ("Albums", parse::albums(list(found, "album")))]
            .into_iter()
            .filter(|(_, items)| !items.is_empty())
            .map(|(title, items)| Section::new(title, items))
            .collect();
        Ok(SearchResults { tracks: parse::songs(list(found, "song")), sections })
    }

    /// A mix from the home page: random songs, or an artist's mix.
    pub fn mix(&self, id: &str) -> Result<Vec<Track>> {
        if let Some(artist) = id.strip_prefix(ARTIST_MIX) {
            return self.artist_mix(artist);
        }
        let r = self.call("getRandomSongs", &[("size", MIX)])?;
        Ok(parse::songs(list(&r["randomSongs"], "song")))
    }

    /// Songs like an artist's (their own songs, shuffled, when the server knows no
    /// similar ones).
    pub fn artist_mix(&self, artist: &str) -> Result<Vec<Track>> {
        let r = self.call("getSimilarSongs2", &[("id", artist), ("count", MIX)])?;
        let similar = parse::songs(list(&r["similarSongs2"], "song"));
        if !similar.is_empty() {
            return Ok(similar);
        }
        let mut own = self.artist_top(artist)?;
        fastrand::shuffle(&mut own);
        Ok(own)
    }

    /// Radio: random songs from the whole library, or from one genre. Asked again
    /// whenever the queue runs low, so it keeps going like Deezer's Flow.
    pub fn radio(&self, genre: Option<&str>) -> Result<Vec<Track>> {
        let mut params = vec![("size", MIX)];
        if let Some(genre) = genre {
            params.push(("genre", genre));
        }
        let r = self.call("getRandomSongs", &params)?;
        Ok(parse::songs(list(&r["randomSongs"], "song")))
    }

    /// A song's lyrics: OpenSubsonic's (synced when available), else the classic
    /// plain text found by artist and title.
    pub fn lyrics(&self, song: &TrackId) -> Result<Option<Lyrics>> {
        let id = song.as_str();
        if self.0.song_lyrics.load(std::sync::atomic::Ordering::Relaxed) {
            let r = self.call("getLyricsBySongId", &[("id", id)])?;
            if let Some(lyrics) = parse::structured_lyrics(&r) {
                return Ok(Some(lyrics));
            }
        }
        let r = self.call("getSong", &[("id", id)])?;
        let (artist, title) = (text(&r["song"]["artist"]), text(&r["song"]["title"]));
        let r = self.call("getLyrics", &[("artist", &artist), ("title", &title)])?;
        Ok(parse::plain_lyrics(&r))
    }

    /// Report a song: `submission` false when it starts ("now playing"), true once
    /// it has been listened to (counts as a play, and a Last.fm scrobble if the
    /// server is set up for it).
    pub fn scrobble(&self, song: &TrackId, submission: bool, started_unix: u64) -> Result<()> {
        let time = (started_unix * 1000).to_string();
        let submission = if submission { "true" } else { "false" };
        self.call("scrobble", &[("id", song.as_str()), ("submission", submission), ("time", &time)]).map(drop)
    }

    // ------------------------------------------------------------ playlists

    /// A new playlist holding `tracks`; its id, if the server says (servers before
    /// API 1.14 don't return the new playlist).
    pub fn create_playlist(&self, title: &str, tracks: &[TrackId]) -> Result<Option<PlaylistId>> {
        let mut params = vec![("name", title)];
        params.extend(tracks.iter().map(|id| ("songId", id.as_str())));
        let r = self.call("createPlaylist", &params)?;
        let id = text(&r["playlist"]["id"]);
        Ok((!id.is_empty()).then(|| PlaylistId::new(id)))
    }

    /// Add the tracks that aren't on the playlist yet; returns how many were added.
    pub fn add_to_playlist(&self, playlist: &PlaylistId, tracks: &[TrackId]) -> Result<usize> {
        let current: std::collections::HashSet<TrackId> = self.playlist(playlist)?.into_iter().map(|t| t.id).collect();
        let new: Vec<&TrackId> = tracks.iter().filter(|t| !current.contains(*t)).collect();
        if new.is_empty() {
            return Ok(0);
        }
        let mut params = vec![("playlistId", playlist.as_str())];
        params.extend(new.iter().map(|id| ("songIdToAdd", id.as_str())));
        self.call("updatePlaylist", &params)?;
        Ok(new.len())
    }

    /// Remove every occurrence of the tracks (Subsonic removes by position).
    pub fn remove_from_playlist(&self, playlist: &PlaylistId, tracks: &[TrackId]) -> Result<()> {
        let positions: Vec<String> =
            self.playlist(playlist)?.iter().enumerate().filter(|(_, t)| tracks.contains(&t.id)).map(|(i, _)| i.to_string()).collect();
        if positions.is_empty() {
            return Ok(());
        }
        let mut params = vec![("playlistId", playlist.as_str())];
        params.extend(positions.iter().map(|i| ("songIndexToRemove", i.as_str())));
        self.call("updatePlaylist", &params).map(drop)
    }

    pub fn rename_playlist(&self, playlist: &Playlist, title: &str) -> Result<()> {
        self.call("updatePlaylist", &[("playlistId", playlist.id.as_str()), ("name", title)]).map(drop)
    }

    pub fn delete_playlist(&self, playlist: &PlaylistId) -> Result<()> {
        self.call("deletePlaylist", &[("id", playlist.as_str())]).map(drop)
    }
}

/// A random mix, then one per artist (up to four) of `albums`.
fn mix_cards(albums: Option<&Value>) -> Vec<Item> {
    let mut cards = Vec::new();
    let mut seen = Vec::new();
    for album in albums.map(|a| list(a, "album")).unwrap_or_default() {
        let id = text(&album["artistId"]);
        if id.is_empty() || seen.contains(&id) {
            continue;
        }
        cards.push(Item {
            kind: ItemKind::Mix,
            id: format!("{ARTIST_MIX}{id}"),
            title: format!("{} mix", text(&album["artist"])),
            subtitle: "Songs like theirs".into(),
            picture: parse::albums(vec![album]).pop().and_then(|a| a.picture),
        });
        seen.push(id);
        if seen.len() == 4 {
            break;
        }
    }
    cards
}

/// The Radio panel: the whole library, then the biggest genres (each card's id is
/// the genre, empty for the whole library).
fn radio_stations(genres: &Value) -> Vec<Item> {
    let mut genres: Vec<(&str, u64)> = list(&genres["genres"], "genre")
        .into_iter()
        .filter_map(|g| Some((g["value"].as_str()?.trim(), g["songCount"].as_u64().unwrap_or(0))))
        .filter(|(name, songs)| !name.is_empty() && *songs >= RADIO_MIN_SONGS)
        .collect();
    genres.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let station = |genre: Option<&str>, title: &str| Item {
        kind: ItemKind::Radio { genre: genre.map(str::to_string) },
        id: genre.unwrap_or_default().into(),
        title: title.into(),
        subtitle: String::new(),
        picture: None,
    };
    std::iter::once(station(None, "Everything"))
        .chain(genres.into_iter().take(RADIO_GENRES).map(|(name, _)| station(Some(name), name)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_offers_the_biggest_genres() {
        let genres = serde_json::json!({"genres": {"genre": [
            {"value": "Jazz", "songCount": 12},
            {"value": "Rock", "songCount": 340},
            {"value": "Polka", "songCount": 2},
            {"value": "", "songCount": 99},
            {"value": "Metal", "songCount": 120},
        ]}});
        let stations = radio_stations(&genres);
        let genres: Vec<Option<&str>> = stations.iter().map(|s| s.kind.tuning()).collect();
        assert_eq!(genres, [None, Some("Rock"), Some("Metal"), Some("Jazz")]);
        assert_eq!(stations[0].title, "Everything");
        assert_eq!(radio_stations(&Value::Null).len(), 1);
    }

    #[test]
    fn mixes_one_per_artist() {
        let r: Value = serde_json::from_str(include_str!("fixtures/album_list.json")).unwrap();
        let albums = super::parse::response(r).unwrap()["albumList2"].take();
        let cards = mix_cards(Some(&albums));
        let titles: Vec<&str> = cards.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["Sine Collective mix", "Square Wave Trio mix"]);
        assert_eq!(cards[0].id, "artist:7AQnKMQc2yBxYD3IZDjJi6");
        assert!(cards.iter().all(|c| c.kind == ItemKind::Mix));
        assert!(mix_cards(None).is_empty());
    }
}
