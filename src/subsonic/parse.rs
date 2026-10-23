//! Models from the Subsonic API's JSON (`f=json`). Servers differ in which optional
//! fields they fill, so everything falls back to the classic Subsonic fields.

use super::{api, local_id};
use crate::model::{Art, ArtistRef, Item, ItemKind, LyricLine, Lyrics, Origin, Playlist, Result, Section, ServerSong, Track};
use serde_json::Value;

/// The `subsonic-response` object of a reply, or the server's error.
pub fn response(mut body: Value) -> Result<Value> {
    let r = body.get_mut("subsonic-response").map(Value::take).ok_or_else(|| api("not a Subsonic server"))?;
    if r["status"] == "ok" {
        return Ok(r);
    }
    let message = text(&r["error"]["message"]);
    Err(api(if message.is_empty() { format!("server error {}", r["error"]["code"]) } else { message }))
}

pub fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn number(v: &Value) -> u64 {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
}

/// The entries under `key`, which servers send as a list (or, with one entry, an
/// object).
pub fn list<'a>(v: &'a Value, key: &str) -> Vec<&'a Value> {
    match &v[key] {
        Value::Array(a) => a.iter().collect(),
        Value::Null => Vec::new(),
        one => vec![one],
    }
}

/// The picture of a song, album, artist or playlist: its cover art id.
pub fn cover(v: &Value) -> Option<Art> {
    Art::new(text(&v["coverArt"]))
}

/// The artists credited on a song or album: OpenSubsonic's list, else the single
/// classic artist.
fn artists(v: &Value) -> Vec<ArtistRef> {
    let listed: Vec<ArtistRef> = list(v, "artists")
        .into_iter()
        .filter_map(|a| Some(ArtistRef { id: Some(text(&a["id"])).filter(|i| !i.is_empty())?, name: text(&a["name"]) }))
        .collect();
    if !listed.is_empty() {
        return listed;
    }
    let (id, name) = (text(&v["artistId"]), text(&v["artist"]));
    if id.is_empty() { Vec::new() } else { vec![ArtistRef { id, name }] }
}

pub fn song(v: &Value) -> Option<Track> {
    let id = text(&v["id"]);
    if id.is_empty() || v["isDir"] == true {
        return None;
    }
    let artists = artists(v);
    let artist = Some(text(&v["displayArtist"]))
        .filter(|a| !a.is_empty())
        .or_else(|| Some(text(&v["artist"])).filter(|a| !a.is_empty()))
        .unwrap_or_else(|| artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "));
    Some(Track {
        id: local_id(&id),
        title: text(&v["title"]),
        artist,
        artists,
        album: text(&v["album"]),
        album_id: text(&v["albumId"]),
        duration: number(&v["duration"]) as u32,
        art: cover(v),
        available: true,
        available_from: String::new(),
        origin: Origin::Server(ServerSong { id, suffix: text(&v["suffix"]).to_lowercase() }),
    })
}

pub fn songs(values: Vec<&Value>) -> Vec<Track> {
    values.into_iter().filter_map(song).collect()
}

/// An album card; `subtitle` says what's under the title (the artist, or the year
/// and kind on an artist's page).
pub fn album_item(v: &Value, subtitle: String) -> Option<Item> {
    let id = text(&v["id"]);
    (!id.is_empty()).then(|| Item {
        kind: ItemKind::Album,
        id,
        title: Some(text(&v["name"])).filter(|n| !n.is_empty()).unwrap_or_else(|| text(&v["title"])),
        subtitle,
        picture: cover(v),
    })
}

/// Album cards with the artist underneath.
pub fn albums(values: Vec<&Value>) -> Vec<Item> {
    values.into_iter().filter_map(|v| album_item(v, album_artist(v))).collect()
}

fn album_artist(v: &Value) -> String {
    Some(text(&v["displayArtist"])).filter(|a| !a.is_empty()).unwrap_or_else(|| text(&v["artist"]))
}

pub fn artist_item(v: &Value) -> Option<Item> {
    let id = text(&v["id"]);
    let albums = number(&v["albumCount"]);
    (!id.is_empty()).then(|| Item {
        kind: ItemKind::Artist,
        id,
        title: text(&v["name"]),
        subtitle: match albums {
            0 => String::new(),
            1 => "1 album".into(),
            n => format!("{n} albums"),
        },
        picture: cover(v),
    })
}

pub fn artists_items(values: Vec<&Value>) -> Vec<Item> {
    values.into_iter().filter_map(artist_item).collect()
}

/// The playlists of `getPlaylists`, with their server ids.
pub fn playlists(v: &Value, user: &str) -> Vec<(String, Playlist)> {
    list(&v["playlists"], "playlist").into_iter().filter_map(|p| playlist(p, user)).collect()
}

pub fn playlist(v: &Value, user: &str) -> Option<(String, Playlist)> {
    let id = text(&v["id"]);
    if id.is_empty() {
        return None;
    }
    let owner = text(&v["owner"]);
    let playlist = Playlist {
        id: local_id(&id),
        title: text(&v["name"]),
        count: number(&v["songCount"]) as u32,
        owned: owner.is_empty() || owner == user,
        private: v["public"] != true,
        picture: cover(v),
    };
    Some((id, playlist))
}

/// What kind of release an album is: OpenSubsonic's release types, else whether
/// it's a compilation. Index into `DISCOGRAPHY`, and the label shown under it.
fn release_kind(v: &Value) -> (usize, &'static str) {
    let types: Vec<String> = list(v, "releaseTypes").into_iter().map(|t| text(t).to_lowercase()).collect();
    let has = |t: &str| types.iter().any(|x| x == t);
    if has("live") {
        (2, "Live")
    } else if has("compilation") || v["isCompilation"] == true {
        (2, "Compilation")
    } else if has("single") {
        (1, "Single")
    } else if has("ep") {
        (1, "EP")
    } else {
        (0, "Album")
    }
}

/// Section titles of an artist's discography, as the artist page expects them.
pub const DISCOGRAPHY: [&str; 3] = ["Albums", "Singles & EPs", "Live & compilations"];

/// An artist's albums grouped like Deezer's discography, newest first.
pub fn discography(artist: &Value) -> Vec<Section> {
    let mut groups: [Vec<(u64, Item)>; 3] = Default::default();
    for v in list(artist, "album") {
        let (group, kind) = release_kind(v);
        let year = number(&v["year"]);
        let subtitle = if year > 0 { format!("{year} · {kind}") } else { kind.to_string() };
        if let Some(item) = album_item(v, subtitle) {
            groups[group].push((year, item));
        }
    }
    groups
        .into_iter()
        .zip(DISCOGRAPHY)
        .filter(|(items, _)| !items.is_empty())
        .map(|(mut dated, title)| {
            dated.sort_by_key(|d| std::cmp::Reverse(d.0));
            Section { title: title.into(), items: dated.into_iter().map(|(_, item)| item).collect() }
        })
        .collect()
}

/// Lyrics from OpenSubsonic's `getLyricsBySongId`: synced ones when there are any.
pub fn structured_lyrics(r: &Value) -> Option<Lyrics> {
    let all = list(&r["lyricsList"], "structuredLyrics");
    let best = all.iter().find(|l| l["synced"] == true).or_else(|| all.first())?;
    let offset = best["offset"].as_i64().unwrap_or(0);
    let lines: Vec<(Option<u32>, String)> =
        list(best, "line").into_iter().map(|l| (l["start"].as_i64().map(|ms| (ms + offset).max(0) as u32), text(&l["value"]))).collect();
    let synced = if best["synced"] == true {
        lines.iter().filter_map(|(at, line)| Some(LyricLine { at_ms: (*at)?, text: line.clone() })).collect()
    } else {
        Vec::new()
    };
    let plain: Vec<String> = lines.into_iter().map(|(_, line)| line).collect();
    lyrics_from(synced, plain)
}

/// Lyrics from the classic `getLyrics` (plain text).
pub fn plain_lyrics(r: &Value) -> Option<Lyrics> {
    let text = text(&r["lyrics"]["value"]);
    lyrics_from(Vec::new(), text.lines().map(|l| l.trim_end().to_string()).collect())
}

fn lyrics_from(synced: Vec<LyricLine>, plain: Vec<String>) -> Option<Lyrics> {
    if plain.iter().all(|l| l.trim().is_empty()) {
        return None;
    }
    Some(Lyrics { synced, plain, writers: String::new(), copyright: String::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Error;
    use serde_json::json;

    /// A recorded reply from a test Navidrome 0.58 (see the fixtures folder).
    fn fixture(name: &str) -> Value {
        let text = match name {
            "album" => include_str!("fixtures/album.json"),
            "album_list" => include_str!("fixtures/album_list.json"),
            "artist" => include_str!("fixtures/artist.json"),
            "artists" => include_str!("fixtures/artists.json"),
            "error_auth" => include_str!("fixtures/error_auth.json"),
            "playlists" => include_str!("fixtures/playlists.json"),
            "playlist" => include_str!("fixtures/playlist.json"),
            "search" => include_str!("fixtures/search.json"),
            "starred" => include_str!("fixtures/starred.json"),
            "top_songs" => include_str!("fixtures/top_songs.json"),
            _ => panic!("no fixture {name}"),
        };
        response(serde_json::from_str(text).unwrap()).unwrap()
    }

    #[test]
    fn errors_carry_the_servers_message() {
        let body: Value = serde_json::from_str(include_str!("fixtures/error_auth.json")).unwrap();
        match response(body) {
            Err(Error::Api { message: m, .. }) => assert_eq!(m, "Wrong username or password"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(response(json!({"hello": 1})), Err(Error::Api { .. })));
    }

    #[test]
    fn album_songs() {
        let r = fixture("album");
        let tracks = songs(list(&r["album"], "song"));
        assert_eq!(tracks.len(), 2);
        let t = &tracks[0];
        assert_eq!((t.title.as_str(), t.artist.as_str(), t.album.as_str(), t.duration), ("Pulse", "Square Wave Trio", "Single Pulse", 20));
        assert_eq!(t.album_id, "5F81DCwcfjVByziJAH9maf");
        assert_eq!(t.artists, vec![ArtistRef { id: "1cX14wrZBmbXZQpEQ79FiO".into(), name: "Square Wave Trio".into() }]);
        assert_eq!(t.art, Art::new("al-5F81DCwcfjVByziJAH9maf_0"));
        assert_eq!(t.origin, Origin::Server(ServerSong { id: "qPyR7XLJOiu93qWw1n06xy".into(), suffix: "wav".into() }));
        assert_eq!(t.id, local_id("qPyR7XLJOiu93qWw1n06xy"));
        assert!(t.available);
    }

    #[test]
    fn album_list_cards() {
        let r = fixture("album_list");
        let cards = albums(list(&r["albumList2"], "album"));
        assert_eq!(cards.len(), 4);
        assert!(cards.iter().all(|c| c.kind == ItemKind::Album && c.picture.as_ref().is_some_and(|p| p.as_str().starts_with("al-"))));
        let pulse = cards.iter().find(|c| c.title == "Single Pulse").unwrap();
        assert_eq!(pulse.subtitle, "Square Wave Trio");
    }

    #[test]
    fn artist_discography_and_index() {
        let sections = discography(&fixture("artist")["artist"]);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, "Albums");
        let years: Vec<&str> = sections[0].items.iter().map(|i| i.subtitle.as_str()).collect();
        assert_eq!(years, ["2025 · Album", "2024 · Album", "2024 · Album"], "newest first");
        let r = fixture("artists");
        let all: Vec<Item> = list(&r["artists"], "index").into_iter().flat_map(|i| artists_items(list(i, "artist"))).collect();
        assert_eq!(all.iter().map(|a| a.title.as_str()).collect::<Vec<_>>(), ["Sine Collective", "Square Wave Trio"]);
        assert_eq!(all[0].subtitle, "3 albums");
        assert_eq!(all[1].picture, Art::new("ar-1cX14wrZBmbXZQpEQ79FiO_0"));
    }

    #[test]
    fn release_kinds_group_like_deezer() {
        let artist = json!({"album": [
            {"id": "1", "name": "LP", "year": 2020, "releaseTypes": ["Album"]},
            {"id": "2", "name": "Hit", "year": 2021, "releaseTypes": ["Single"]},
            {"id": "3", "name": "Alive", "releaseTypes": ["Album", "Live"]},
            {"id": "4", "name": "Best of", "year": 2019, "isCompilation": true}
        ]});
        let sections = discography(&artist);
        let titles: Vec<&str> = sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Albums", "Singles & EPs", "Live & compilations"]);
        assert_eq!(sections[1].items[0].subtitle, "2021 · Single");
        assert_eq!(sections[2].items.iter().map(|i| i.subtitle.as_str()).collect::<Vec<_>>(), ["2019 · Compilation", "Live"]);
    }

    #[test]
    fn playlists_with_owner() {
        let found = playlists(&fixture("playlists"), "dust");
        assert_eq!(found.len(), 1);
        let (id, p) = &found[0];
        assert_eq!((id.as_str(), p.title.as_str(), p.count, p.owned, p.private), ("J78Lrnn5aCX1IyCU5feMey", "Testlist", 2, true, true));
        assert_eq!(p.id, local_id(id));
        assert!(!playlists(&fixture("playlists"), "someone else")[0].1.owned);
        let r = fixture("playlist");
        assert_eq!(songs(list(&r["playlist"], "entry")).len(), 2);
    }

    #[test]
    fn search_and_starred() {
        let r = fixture("search");
        let found = &r["searchResult3"];
        assert_eq!(songs(list(found, "song")).len(), 2);
        assert_eq!(albums(list(found, "album"))[0].title, "Live at the Lab");
        let r = fixture("starred");
        let starred = &r["starred2"];
        assert_eq!(songs(list(starred, "song"))[0].title, "Echo");
        assert_eq!(albums(list(starred, "album")).len(), 1);
        assert_eq!(artists_items(list(starred, "artist")).len(), 1);
        // An empty result is an empty object.
        assert!(list(&fixture("top_songs")["topSongs"], "song").is_empty());
    }

    #[test]
    fn single_entries_may_be_objects() {
        let v = json!({"song": {"id": "x", "title": "Solo", "suffix": "MP3"}});
        let tracks = songs(list(&v, "song"));
        assert_eq!(tracks.len(), 1);
        assert!(matches!(&tracks[0].origin, Origin::Server(s) if s.suffix == "mp3"));
    }

    #[test]
    fn synced_lyrics_preferred() {
        let r = json!({"lyricsList": {"structuredLyrics": [
            {"lang": "eng", "synced": false, "line": [{"value": "Plain one"}]},
            {"lang": "eng", "synced": true, "offset": -100, "line": [{"start": 1000, "value": "First"}, {"start": 3500, "value": "Second"}]}
        ]}});
        let l = structured_lyrics(&r).unwrap();
        assert_eq!(l.synced.iter().map(|x| (x.at_ms, x.text.as_str())).collect::<Vec<_>>(), [(900, "First"), (3400, "Second")]);
        assert_eq!(l.plain, ["First", "Second"]);
        assert_eq!(structured_lyrics(&json!({"lyricsList": {}})), None);
        let plain = plain_lyrics(&json!({"lyrics": {"artist": "A", "title": "T", "value": "La la\nLa"}})).unwrap();
        assert!(plain.synced.is_empty());
        assert_eq!(plain.plain, ["La la", "La"]);
        assert_eq!(plain_lyrics(&json!({"lyrics": {}})), None);
    }
}
