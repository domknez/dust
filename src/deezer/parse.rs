//! Building models from gw-light JSON, which mixes strings and numbers freely.

use super::models::{ArtistPage, ArtistRef, Item, Playlist, Section, Track};
use serde_json::Value;

/// String field, tolerating numbers and nulls.
pub fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Unsigned number field, tolerating numeric strings; 0 when absent.
pub fn number(v: &Value) -> u64 {
    match v {
        Value::Number(n) => n.as_u64().unwrap_or(0),
        Value::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

/// (kind, md5) picture reference, if both are present.
fn picture(kind: &Value, md5: &Value) -> Option<(String, String)> {
    Some((text(kind), text(md5))).filter(|(k, m)| !k.is_empty() && !m.is_empty())
}

pub fn track(v: &Value) -> Option<Track> {
    let id = number(&v["SNG_ID"]);
    let token = text(&v["TRACK_TOKEN"]);
    if id == 0 || token.is_empty() {
        return None; // user uploads (negative ids) or unavailable tracks
    }
    // Every credited name is shown; only those with an id become links.
    let artist = match v["ARTISTS"].as_array() {
        Some(a) if !a.is_empty() => a.iter().map(|a| text(&a["ART_NAME"])).collect::<Vec<_>>().join(", "),
        _ => text(&v["ART_NAME"]),
    };
    let artists = artists(v);
    let title = match text(&v["VERSION"]) {
        version if version.is_empty() => text(&v["SNG_TITLE"]),
        version => format!("{} {version}", text(&v["SNG_TITLE"])),
    };
    let fallback =
        Some((number(&v["FALLBACK"]["SNG_ID"]), text(&v["FALLBACK"]["TRACK_TOKEN"]))).filter(|(id, tok)| *id != 0 && !tok.is_empty());
    // Not streamable on a subscription yet (e.g. an album's unreleased tracks),
    // unless Deezer offers another version.
    let rights = &v["RIGHTS"];
    let blocked = rights["STREAM_SUB_AVAILABLE"].as_bool() == Some(false) && fallback.is_none();
    let available_from = if blocked { text(&rights["STREAM_SUB"]) } else { String::new() };
    Some(Track {
        id,
        title,
        artist,
        artists,
        album: text(&v["ALB_TITLE"]),
        album_id: Some(text(&v["ALB_ID"])).filter(|id| id != "0").unwrap_or_default(),
        duration: number(&v["DURATION"]) as u32,
        token,
        cover: text(&v["ALB_PICTURE"]),
        fallback,
        available: !blocked,
        available_from,
    })
}

/// `ARTISTS` (or the lone `ART_ID`/`ART_NAME`) of a track or album.
pub fn artists(v: &Value) -> Vec<ArtistRef> {
    let one =
        |a: &Value| Some(ArtistRef { id: text(&a["ART_ID"]), name: text(&a["ART_NAME"]) }).filter(|a| !a.id.is_empty() && a.id != "0");
    match v["ARTISTS"].as_array() {
        Some(list) if !list.is_empty() => list.iter().filter_map(one).collect(),
        _ => one(v).into_iter().collect(),
    }
}

pub fn tracks(v: &Value) -> Vec<Track> {
    v.as_array().map(|a| a.iter().filter_map(track).collect()).unwrap_or_default()
}

pub fn playlist(v: &Value) -> Playlist {
    Playlist {
        id: number(&v["PLAYLIST_ID"]),
        title: text(&v["TITLE"]),
        count: number(&v["NB_SONG"]) as u32,
        picture: picture(&v["PICTURE_TYPE"], &v["PLAYLIST_PICTURE"]),
    }
}

pub fn section(v: &Value) -> Section {
    Section {
        title: text(&v["title"]),
        layout: text(&v["layout"]),
        items: v["items"].as_array().map(|a| a.iter().map(item).collect()).unwrap_or_default(),
    }
}

/// Most cards a search section shows.
const SEARCH_SECTION_ITEMS: usize = 24;

/// A search result list (`ARTIST`, `ALBUM`, `PLAYLIST` data) as a card section.
pub fn search_section(title: &str, kind: &str, data: &Value) -> Section {
    let items = data.as_array().map(|a| a.iter().filter_map(|v| card_item(kind, v)).take(SEARCH_SECTION_ITEMS).collect());
    Section { title: title.into(), layout: "search".into(), items: items.unwrap_or_default() }
}

/// An artist, album or playlist from gw-light data as a card.
pub fn card_item(kind: &str, v: &Value) -> Option<Item> {
    let (id, title, picture) = match kind {
        "artist" => (&v["ART_ID"], &v["ART_NAME"], picture(&"artist".into(), &v["ART_PICTURE"])),
        "album" => (&v["ALB_ID"], &v["ALB_TITLE"], picture(&"cover".into(), &v["ALB_PICTURE"])),
        "playlist" => (&v["PLAYLIST_ID"], &v["TITLE"], picture(&v["PICTURE_TYPE"], &v["PLAYLIST_PICTURE"])),
        _ => return None,
    };
    let id = text(id);
    if id.is_empty() || id == "0" {
        return None;
    }
    let (title, subtitle) = english_texts(kind, &id, v, text(title), String::new());
    Some(Item { kind: kind.into(), id, title, subtitle, picture })
}

/// Releases grouped like Deezer's artist page: albums, singles and EPs, then live
/// albums and compilations, each newest first.
pub fn discography(data: &Value) -> Vec<Section> {
    let mut groups: [(&str, Vec<(String, Item)>); 3] =
        [("Albums", Vec::new()), ("Singles & EPs", Vec::new()), ("Live & compilations", Vec::new())];
    for v in data.as_array().into_iter().flatten() {
        let id = text(&v["ALB_ID"]);
        if id.is_empty() {
            continue;
        }
        let subtypes = &v["SUBTYPES"];
        let (group, kind) = if subtypes["isLive"].as_bool() == Some(true) {
            (2, "Live")
        } else if subtypes["isCompilation"].as_bool() == Some(true) {
            (2, "Compilation")
        } else {
            match text(&v["TYPE"]).as_str() {
                "1" => (0, "Album"),
                "3" => (1, "EP"),
                _ => (1, "Single"),
            }
        };
        let date = Some(text(&v["ORIGINAL_RELEASE_DATE"])).filter(|d| !d.is_empty()).unwrap_or_else(|| text(&v["PHYSICAL_RELEASE_DATE"]));
        let subtitle = match date.get(..4) {
            Some(year) => format!("{year} · {kind}"),
            None => kind.to_string(),
        };
        let picture = picture(&"cover".into(), &v["ALB_PICTURE"]);
        groups[group].1.push((date, Item { kind: "album".into(), id, title: text(&v["ALB_TITLE"]), subtitle, picture }));
    }
    groups
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(title, mut dated)| {
            // ISO dates sort as text; the sort is stable, so ties keep Deezer's order.
            dated.sort_by(|a, b| b.0.cmp(&a.0));
            Section { title: title.into(), layout: "artist".into(), items: dated.into_iter().map(|(_, item)| item).collect() }
        })
        .collect()
}

/// `deezer.pageArtist` plus the full discography.
pub fn artist_page(page: &Value, discography_data: &Value) -> ArtistPage {
    let data = &page["DATA"];
    let cards = |kind: &str, list: &Value| {
        list.as_array().map(|a| a.iter().filter_map(|v| card_item(kind, v)).collect::<Vec<_>>()).unwrap_or_default()
    };
    let mut sections = discography(discography_data);
    for (title, kind, key) in [("Featured in", "playlist", "RELATED_PLAYLIST"), ("Fans also like", "artist", "RELATED_ARTISTS")] {
        let items = cards(kind, &page[key]["data"]);
        if !items.is_empty() {
            sections.push(Section { title: title.into(), layout: "artist".into(), items });
        }
    }
    ArtistPage {
        name: text(&data["ART_NAME"]),
        picture: Some(text(&data["ART_PICTURE"])).filter(|p| !p.is_empty()),
        fans: number(&data["NB_FAN"]),
        top: tracks(&page["TOP"]["data"]),
        sections,
    }
}

pub fn item(v: &Value) -> Item {
    let picture = v["pictures"].get(0).and_then(|p| picture(&p["type"], &p["md5"]));
    let (kind, id, data) = (text(&v["type"]), text(&v["id"]), &v["data"]);
    // Deezer localises item texts by the listener's location, ignoring the requested
    // language; rebuild them in English from the raw data where we can.
    let (title, subtitle) = english_texts(&kind, &id, data, text(&v["title"]), text(&v["subtitle"]));
    Item { kind, id, title, subtitle, picture }
}

/// 1234567 -> "1,234,567"
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{} {}", thousands(n), if n == 1 { one } else { many })
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn english_texts(kind: &str, id: &str, data: &Value, title: String, subtitle: String) -> (String, String) {
    match kind {
        "playlist" => {
            let mut sub = plural(number(&data["NB_SONG"]), "track", "tracks");
            let fans = number(&data["NB_FAN"]);
            if fans > 0 {
                sub = format!("{sub} · {}", plural(fans, "fan", "fans"));
            }
            (title, sub)
        }
        "album" => (title, text(&data["ART_NAME"])),
        "artist" => (title, plural(number(&data["NB_FAN"]), "fan", "fans")),
        "flow" => {
            let name = match id {
                "default" => "Flow".to_string(),
                other => other.split(['-', '_']).map(capitalize).collect::<Vec<_>>().join(" "),
            };
            (name, String::new())
        }
        "smarttracklist" => {
            let title = match id {
                "discovery" => "Discovery".to_string(),
                "new-releases" => "New releases".to_string(),
                _ => title,
            };
            // "Featuring A, B" arrives as e.g. "Sadrži A, B": swap the leading word.
            let sub = match subtitle.split_once(' ') {
                Some((first, rest)) if first != "Featuring" && !rest.is_empty() => format!("Featuring {rest}"),
                _ => subtitle,
            };
            (title, sub)
        }
        _ => (title, subtitle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn english_item_texts() {
        let pl = json!({"NB_SONG": "75", "NB_FAN": 185597});
        assert_eq!(english_texts("playlist", "1", &pl, "Radar".into(), "75 pjesama".into()).1, "75 tracks · 185,597 fans");
        assert_eq!(english_texts("album", "1", &json!({"ART_NAME": "TOOL"}), "Undertow".into(), "izvođača TOOL".into()).1, "TOOL");
        assert_eq!(english_texts("artist", "1", &json!({"NB_FAN": 1}), "X".into(), String::new()).1, "1 fan");
        assert_eq!(english_texts("flow", "motivation", &json!({}), "Vježbanje".into(), String::new()).0, "Motivation");
        assert_eq!(english_texts("flow", "hip-hop", &json!({}), "x".into(), String::new()).0, "Hip Hop");
        let (t, sub) = english_texts("smarttracklist", "discovery", &json!({}), "Otkriće".into(), "Sadrži Foxy Shazam, The Flynts".into());
        assert_eq!((t.as_str(), sub.as_str()), ("Discovery", "Featuring Foxy Shazam, The Flynts"));
        assert_eq!(thousands(4702289), "4,702,289");
    }

    #[test]
    fn search_sections() {
        let artists = json!([{"ART_ID": "27", "ART_NAME": "Daft Punk", "ART_PICTURE": "abc", "NB_FAN": 2}, {"ART_ID": "0"}]);
        let s = search_section("Artists", "artist", &artists);
        assert_eq!(s.items.len(), 1);
        let a = &s.items[0];
        assert_eq!((a.kind.as_str(), a.id.as_str(), a.title.as_str(), a.subtitle.as_str()), ("artist", "27", "Daft Punk", "2 fans"));
        assert_eq!(a.picture, Some(("artist".into(), "abc".into())));
        let albums = json!([{"ALB_ID": 302127, "ALB_TITLE": "Discovery", "ALB_PICTURE": "def", "ART_NAME": "Daft Punk"}]);
        let b = &search_section("Albums", "album", &albums).items[0];
        assert_eq!((b.id.as_str(), b.subtitle.as_str(), b.picture.as_ref().map(|p| p.0.as_str())), ("302127", "Daft Punk", Some("cover")));
        let playlists = json!([{"PLAYLIST_ID": "9", "TITLE": "Mix", "PICTURE_TYPE": "playlist", "PLAYLIST_PICTURE": "f", "NB_SONG": 3}]);
        assert_eq!(search_section("Playlists", "playlist", &playlists).items[0].subtitle, "3 tracks");
        assert!(search_section("Artists", "artist", &json!(null)).items.is_empty());
    }

    #[test]
    fn discography_groups() {
        let data = json!([
            {"ALB_ID": "1", "ALB_TITLE": "RAM", "TYPE": "1", "ORIGINAL_RELEASE_DATE": "2013-05-17", "ALB_PICTURE": "a", "SUBTYPES": {}},
            {"ALB_ID": "2", "ALB_TITLE": "Get Lucky", "TYPE": "0", "ORIGINAL_RELEASE_DATE": "2013-04-19", "SUBTYPES": {}},
            {"ALB_ID": "3", "ALB_TITLE": "Alive 2007", "TYPE": "1", "SUBTYPES": {"isLive": true}},
            {"ALB_ID": "4", "ALB_TITLE": "Sampler", "TYPE": "3", "PHYSICAL_RELEASE_DATE": "1997-01-01", "SUBTYPES": {}}
        ]);
        let sections = discography(&data);
        let titles: Vec<&str> = sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Albums", "Singles & EPs", "Live & compilations"]);
        assert_eq!(sections[0].items[0].subtitle, "2013 · Album");
        assert_eq!(sections[1].items.iter().map(|i| i.subtitle.as_str()).collect::<Vec<_>>(), ["2013 · Single", "1997 · EP"]);
        let reversed = json!([{"ALB_ID": "5", "TYPE": "3", "ORIGINAL_RELEASE_DATE": "1997-01-01"}, {"ALB_ID": "6", "TYPE": "0", "ORIGINAL_RELEASE_DATE": "2023-01-01"}]);
        assert_eq!(discography(&reversed)[0].items[0].id, "6");
        assert_eq!(sections[2].items[0].subtitle, "Live");
    }

    #[test]
    fn track_artists_link() {
        let t = track(&json!({"SNG_ID": "7", "TRACK_TOKEN": "x", "ARTISTS": [{"ART_ID": "27", "ART_NAME": "Daft Punk"}, {"ART_ID": "0", "ART_NAME": "?"}]})).unwrap();
        assert_eq!(t.artists, [ArtistRef { id: "27".into(), name: "Daft Punk".into() }]);
        let t = track(&json!({"SNG_ID": "8", "TRACK_TOKEN": "x", "ART_ID": "13", "ART_NAME": "Eminem", "ALB_ID": "302127"})).unwrap();
        assert_eq!((t.artist.as_str(), t.artists.len(), t.album_id.as_str()), ("Eminem", 1, "302127"));
        let t = track(&json!({"SNG_ID": "9", "TRACK_TOKEN": "x", "ALB_ID": 0})).unwrap();
        assert!(t.album_id.is_empty());
    }

    #[test]
    fn unreleased_tracks_are_unavailable() {
        let rights = |ok: bool, from: &str| json!({"STREAM_SUB_AVAILABLE": ok, "STREAM_SUB": from});
        let t = track(&json!({"SNG_ID": "1", "TRACK_TOKEN": "x", "RIGHTS": rights(false, "2026-11-20")})).unwrap();
        assert_eq!((t.available, t.available_from.as_str()), (false, "2026-11-20"));
        let t = track(&json!({"SNG_ID": "2", "TRACK_TOKEN": "x", "RIGHTS": rights(true, "2000-01-01")})).unwrap();
        assert!(t.available && t.available_from.is_empty());
        assert!(track(&json!({"SNG_ID": "3", "TRACK_TOKEN": "x"})).unwrap().available, "no rights info: assume playable");
        let fallback =
            json!({"SNG_ID": "4", "TRACK_TOKEN": "x", "RIGHTS": rights(false, ""), "FALLBACK": {"SNG_ID": 5, "TRACK_TOKEN": "y"}});
        assert!(track(&fallback).unwrap().available, "a regional fallback still plays");
    }

    #[test]
    fn track_with_version_and_fallback() {
        let t = track(&json!({
            "SNG_ID": "7", "TRACK_TOKEN": "tok", "SNG_TITLE": "Song", "VERSION": "(Live)",
            "ARTISTS": [{"ART_NAME": "A"}, {"ART_NAME": "B"}], "DURATION": "180",
            "FALLBACK": {"SNG_ID": 8, "TRACK_TOKEN": "tok2"}
        }))
        .unwrap();
        assert_eq!((t.title.as_str(), t.artist.as_str(), t.duration), ("Song (Live)", "A, B", 180));
        assert_eq!(t.fallback, Some((8, "tok2".into())));
        assert!(track(&json!({"SNG_ID": "-5", "TRACK_TOKEN": "x"})).is_none());
    }
}
