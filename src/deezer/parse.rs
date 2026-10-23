//! Building models from gw-light JSON, which mixes strings and numbers freely.

use super::images::art;
use crate::model::{AlbumInfo, Art, ArtistPage, ArtistRef, DeezerSong, Episode, Item, ItemKind, Origin, Playlist, Section, Track};
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

/// A picture from its family (`cover`, `playlist`, ...) and md5 fields, if both
/// are present.
pub fn picture(family: &Value, md5: &Value) -> Option<Art> {
    art(&text(family), &text(md5))
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
        art: art("cover", &text(&v["ALB_PICTURE"])),
        available: !blocked,
        available_from,
        origin: Origin::Deezer(DeezerSong { token, fallback }),
    })
}

/// A podcast episode, if it can be played (it has its own audio file).
pub fn episode(v: &Value) -> Option<Track> {
    let id = number(&v["EPISODE_ID"]);
    let url = text(&v["EPISODE_DIRECT_STREAM_URL"]);
    if id == 0 || url.is_empty() {
        return None;
    }
    let show = text(&v["SHOW_NAME"]);
    // "2026-09-17 00:40:49" -> "2026-09-17"
    let published = text(&v["EPISODE_PUBLISHED_TIMESTAMP"]).chars().take(10).collect();
    Some(Track {
        id,
        title: text(&v["EPISODE_TITLE"]),
        artist: show.clone(),
        artists: Vec::new(),
        album: show,
        album_id: String::new(),
        duration: number(&v["DURATION"]) as u32,
        art: art("talk", &text(&v["SHOW_ART_MD5"])),
        available: v["AVAILABLE"].as_bool() != Some(false),
        available_from: String::new(),
        origin: Origin::Episode(Episode { url, show_id: text(&v["SHOW_ID"]), published }),
    })
}

/// `deezer.pageShow`: the show's playable episodes, newest first.
pub fn show_episodes(page: &Value) -> Vec<Track> {
    let mut episodes: Vec<Track> = page["EPISODES"]["data"].as_array().map(|a| a.iter().filter_map(episode).collect()).unwrap_or_default();
    episodes.sort_by(|a, b| {
        let date = |t: &Track| t.episode().map(|e| e.published.clone()).unwrap_or_default();
        date(b).cmp(&date(a))
    });
    episodes
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

/// A playlist of the profile; `me` is the account's user id (ownership).
pub fn playlist(v: &Value, me: u64) -> Playlist {
    Playlist {
        id: number(&v["PLAYLIST_ID"]),
        title: text(&v["TITLE"]),
        count: number(&v["NB_SONG"]) as u32,
        owned: number(&v["PARENT_USER_ID"]) == me,
        private: number(&v["STATUS"]) == 1,
        picture: picture(&v["PICTURE_TYPE"], &v["PLAYLIST_PICTURE"]),
    }
}

pub fn section(v: &Value) -> Section {
    Section { title: text(&v["title"]), items: v["items"].as_array().map(|a| a.iter().map(item).collect()).unwrap_or_default() }
}

/// Most cards a search section shows.
const SEARCH_SECTION_ITEMS: usize = 24;

/// A search result list (`ARTIST`, `ALBUM`, `PLAYLIST` data) as a card section.
pub fn search_section(title: &str, kind: &str, data: &Value) -> Section {
    let items = data.as_array().map(|a| a.iter().filter_map(|v| card_item(kind, v)).take(SEARCH_SECTION_ITEMS).collect());
    Section { title: title.into(), items: items.unwrap_or_default() }
}

/// An artist, album or playlist from gw-light data as a card.
pub fn card_item(kind: &str, v: &Value) -> Option<Item> {
    let (id, title, picture) = match kind {
        "artist" => (&v["ART_ID"], &v["ART_NAME"], art("artist", &text(&v["ART_PICTURE"]))),
        "album" => (&v["ALB_ID"], &v["ALB_TITLE"], art("cover", &text(&v["ALB_PICTURE"]))),
        "playlist" => (&v["PLAYLIST_ID"], &v["TITLE"], picture(&v["PICTURE_TYPE"], &v["PLAYLIST_PICTURE"])),
        "show" => (&v["SHOW_ID"], &v["SHOW_NAME"], art("talk", &text(&v["SHOW_ART_MD5"]))),
        _ => return None,
    };
    let id = text(id);
    if id.is_empty() || id == "0" {
        return None;
    }
    let (title, subtitle) = english_texts(kind, &id, v, text(title), String::new());
    Some(Item { kind: item_kind(kind, &id), id, title, subtitle, picture })
}

/// Most of the artist's other releases an album page lists.
const MORE_RELEASES: usize = 6;

/// An album page's year, label and the artist's other releases.
pub fn album_info(page: &Value) -> AlbumInfo {
    let data = &page["DATA"];
    let id = text(&data["ALB_ID"]);
    let year = |v: &Value| {
        let date = Some(text(&v["ORIGINAL_RELEASE_DATE"])).filter(|d| !d.is_empty()).unwrap_or_else(|| text(&v["PHYSICAL_RELEASE_DATE"]));
        date.get(..4).unwrap_or_default().to_string()
    };
    let more = page["ALBUMS"]["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| text(&v["ALB_ID"]) != id && !text(&v["ALB_ID"]).is_empty())
        .take(MORE_RELEASES)
        .map(|v| Item {
            kind: ItemKind::Album,
            id: text(&v["ALB_ID"]),
            title: text(&v["ALB_TITLE"]),
            subtitle: year(v),
            picture: art("cover", &text(&v["ALB_PICTURE"])),
        })
        .collect();
    AlbumInfo { year: year(data), label: text(&data["LABEL_NAME"]), more }
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
        let picture = art("cover", &text(&v["ALB_PICTURE"]));
        groups[group].1.push((date, Item { kind: ItemKind::Album, id, title: text(&v["ALB_TITLE"]), subtitle, picture }));
    }
    groups
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(title, mut dated)| {
            // ISO dates sort as text; the sort is stable, so ties keep Deezer's order.
            dated.sort_by(|a, b| b.0.cmp(&a.0));
            Section { title: title.into(), items: dated.into_iter().map(|(_, item)| item).collect() }
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
            sections.push(Section { title: title.into(), items });
        }
    }
    ArtistPage {
        name: text(&data["ART_NAME"]),
        picture: art("artist", &text(&data["ART_PICTURE"])),
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
    Item { kind: item_kind(&kind, &id), id, title, subtitle, picture }
}

/// What a card of Deezer's `type` opens. Flow's `default` card is plain Flow, the
/// others its moods.
fn item_kind(kind: &str, id: &str) -> ItemKind {
    let tuning = || Some(id.to_string()).filter(|id| id != "default");
    match kind {
        "album" => ItemKind::Album,
        "artist" => ItemKind::Artist,
        "playlist" => ItemKind::Playlist,
        "smarttracklist" => ItemKind::Mix,
        "track" => ItemKind::TrackMix,
        "show" => ItemKind::Show,
        "flow" => ItemKind::Flow { mood: tuning() },
        "radio" => ItemKind::Radio { genre: tuning() },
        other => ItemKind::Other(other.to_string()),
    }
}

/// 1234567 -> "1,234,567"
fn thousands(n: u64) -> String {
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
        "show" => {
            let by = text(&data["LABEL_NAME"]);
            (title, if by.is_empty() { "Podcast".to_string() } else { by })
        }
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
                // The month is the second tag: ["monthlyTop", "09", "2026"].
                "monthly-top" => month_name(&text(&data["TAGS"][1])).map_or(title, |m| format!("My top {m}")),
                // "daily" in the data; deezer.com draws its names into the cover.
                id if id.starts_with("inspired-by-") => format!("Daily mix {}", id.trim_start_matches("inspired-by-")),
                _ => title,
            };
            // "Featuring A, B" arrives as e.g. "Sadrži A, B": swap the leading word.
            let sub = match subtitle.split_once(' ') {
                Some((first, rest)) if first != "Featuring" && !rest.is_empty() => format!("Featuring {rest}"),
                _ => subtitle,
            };
            (title, sub)
        }
        // A track opening its mix: "by <artist>".
        "track" => (title, format!("by {}", text(&data["ART_NAME"]))),
        _ => (title, subtitle),
    }
}

/// "09" -> "September".
fn month_name(number: &str) -> Option<&'static str> {
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    MONTHS.get(number.parse::<usize>().ok()?.checked_sub(1)?).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_info_has_year_label_and_other_releases() {
        let page = serde_json::json!({
            "DATA": {"ALB_ID": "1", "ORIGINAL_RELEASE_DATE": "2021-04-30", "LABEL_NAME": "Roadrunner Records"},
            "ALBUMS": {"data": [
                {"ALB_ID": "1", "ALB_TITLE": "Fortitude", "PHYSICAL_RELEASE_DATE": "2021-04-30"},
                {"ALB_ID": "2", "ALB_TITLE": "Magma", "PHYSICAL_RELEASE_DATE": "2016-06-17", "ALB_PICTURE": "abc"}
            ]}
        });
        let info = album_info(&page);
        assert_eq!((info.year.as_str(), info.label.as_str()), ("2021", "Roadrunner Records"));
        assert_eq!(info.more.len(), 1, "the album itself is left out");
        assert_eq!((info.more[0].title.as_str(), info.more[0].subtitle.as_str()), ("Magma", "2016"));
        assert_eq!(info.more[0].picture, art("cover", "abc"));
        assert_eq!(album_info(&serde_json::json!({})), AlbumInfo::default());
    }
    use serde_json::json;

    fn episode_json(id: &str, published: &str) -> Value {
        json!({
            "EPISODE_ID": id,
            "EPISODE_TITLE": format!("Episode {id}"),
            "DURATION": "13430",
            "EPISODE_DIRECT_STREAM_URL": format!("https://example.com/{id}.mp3"),
            "EPISODE_PUBLISHED_TIMESTAMP": published,
            "SHOW_ID": "724752",
            "SHOW_NAME": "Lex Fridman Podcast",
            "SHOW_ART_MD5": "fa87",
            "AVAILABLE": true
        })
    }

    #[test]
    fn episodes_and_shows() {
        let t = episode(&episode_json("935346132", "2026-09-17 00:40:49")).unwrap();
        assert_eq!(
            (t.id, t.title.as_str(), t.artist.as_str(), t.duration, t.art.as_ref().map(Art::as_str)),
            (935346132, "Episode 935346132", "Lex Fridman Podcast", 13430, Some("talk/fa87"))
        );
        assert!(t.available && t.album_id.is_empty() && t.artists.is_empty());
        let e = t.episode().unwrap();
        assert_eq!(
            (e.url.as_str(), e.show_id.as_str(), e.published.as_str()),
            ("https://example.com/935346132.mp3", "724752", "2026-09-17")
        );
        // Without its own audio file there is nothing to play.
        assert!(episode(&json!({"EPISODE_ID": "1", "EPISODE_DIRECT_STREAM_URL": ""})).is_none());

        let page = json!({"EPISODES": {"total": 3, "data": [
            episode_json("1", "2026-01-02 10:00:00"),
            episode_json("2", "2026-03-01 10:00:00"),
            episode_json("3", "2025-12-31 10:00:00"),
        ]}});
        assert_eq!(show_episodes(&page).iter().map(|t| t.id).collect::<Vec<_>>(), vec![2, 1, 3], "newest first");

        let card = card_item(
            "show",
            &json!({"SHOW_ID": "724752", "SHOW_NAME": "Lex Fridman Podcast", "SHOW_ART_MD5": "fa87", "LABEL_NAME": "Lex Fridman"}),
        )
        .unwrap();
        assert_eq!((&card.kind, card.title.as_str(), card.subtitle.as_str()), (&ItemKind::Show, "Lex Fridman Podcast", "Lex Fridman"));
        assert_eq!(card.picture, art("talk", "fa87"));
    }

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
        let top = json!({"TAGS": ["monthlyTop", "09", "2026"]});
        assert_eq!(english_texts("smarttracklist", "monthly-top", &top, "Moj top rujan".into(), String::new()).0, "My top September");
        assert_eq!(english_texts("smarttracklist", "inspired-by-3", &json!({}), "daily".into(), String::new()).0, "Daily mix 3");
        let track = json!({"ART_NAME": "Slash"});
        assert_eq!(english_texts("track", "1", &track, "Ghost".into(), "Slash".into()), ("Ghost".into(), "by Slash".into()));
    }

    #[test]
    fn home_items_have_kinds() {
        let kind = |v: Value| item(&v).kind;
        assert_eq!(kind(json!({"type": "playlist", "id": "1479458365"})), ItemKind::Playlist);
        assert_eq!(kind(json!({"type": "album", "id": "302127"})), ItemKind::Album);
        assert_eq!(kind(json!({"type": "artist", "id": "27"})), ItemKind::Artist);
        assert_eq!(kind(json!({"type": "smarttracklist", "id": "discovery"})), ItemKind::Mix);
        assert_eq!(kind(json!({"type": "track", "id": "4229026412"})), ItemKind::TrackMix);
        assert_eq!(kind(json!({"type": "show", "id": "724752"})), ItemKind::Show);
        assert_eq!(kind(json!({"type": "flow", "id": "default", "title": "Flow"})), ItemKind::Flow { mood: None });
        assert_eq!(kind(json!({"type": "flow", "id": "chill"})), ItemKind::Flow { mood: Some("chill".into()) });
        assert_eq!(kind(json!({"type": "channel", "id": "x"})), ItemKind::Other("channel".into()));
        let flow = item(&json!({"type": "flow", "id": "hip-hop", "title": "Hip-hop", "pictures": [{"type": "misc", "md5": "f1"}]}));
        assert_eq!((flow.title.as_str(), flow.kind.tuning(), flow.picture), ("Hip Hop", Some("hip-hop"), art("misc", "f1")));
    }

    #[test]
    fn search_sections() {
        let artists = json!([{"ART_ID": "27", "ART_NAME": "Daft Punk", "ART_PICTURE": "abc", "NB_FAN": 2}, {"ART_ID": "0"}]);
        let s = search_section("Artists", "artist", &artists);
        assert_eq!(s.items.len(), 1);
        let a = &s.items[0];
        assert_eq!((&a.kind, a.id.as_str(), a.title.as_str(), a.subtitle.as_str()), (&ItemKind::Artist, "27", "Daft Punk", "2 fans"));
        assert_eq!(a.picture, art("artist", "abc"));
        let albums = json!([{"ALB_ID": 302127, "ALB_TITLE": "Discovery", "ALB_PICTURE": "def", "ART_NAME": "Daft Punk"}]);
        let b = &search_section("Albums", "album", &albums).items[0];
        assert_eq!((b.id.as_str(), b.subtitle.as_str(), b.picture.as_ref().map(Art::as_str)), ("302127", "Daft Punk", Some("cover/def")));
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
        assert_eq!(t.origin, Origin::Deezer(DeezerSong { token: "tok".into(), fallback: Some((8, "tok2".into())) }));
        assert!(track(&json!({"SNG_ID": "-5", "TRACK_TOKEN": "x"})).is_none());
    }
}
