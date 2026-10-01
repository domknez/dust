//! Building models from gw-light JSON, which mixes strings and numbers freely.

use super::models::{Item, Playlist, Section, Track};
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
    let artist = match v["ARTISTS"].as_array() {
        Some(a) if !a.is_empty() => a.iter().map(|a| text(&a["ART_NAME"])).collect::<Vec<_>>().join(", "),
        _ => text(&v["ART_NAME"]),
    };
    let title = match text(&v["VERSION"]) {
        version if version.is_empty() => text(&v["SNG_TITLE"]),
        version => format!("{} {version}", text(&v["SNG_TITLE"])),
    };
    let fallback =
        Some((number(&v["FALLBACK"]["SNG_ID"]), text(&v["FALLBACK"]["TRACK_TOKEN"]))).filter(|(id, tok)| *id != 0 && !tok.is_empty());
    Some(Track {
        id,
        title,
        artist,
        album: text(&v["ALB_TITLE"]),
        duration: number(&v["DURATION"]) as u32,
        token,
        cover: text(&v["ALB_PICTURE"]),
        fallback,
    })
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

pub fn item(v: &Value) -> Item {
    let picture = v["pictures"].get(0).and_then(|p| picture(&p["type"], &p["md5"]));
    let (kind, id, data) = (text(&v["type"]), text(&v["id"]), &v["data"]);
    // Deezer localises item texts by the listener's location, ignoring the requested
    // language; rebuild them in English from the raw data where we can.
    let (title, subtitle) = english_texts(&kind, &id, data, text(&v["title"]), text(&v["subtitle"]));
    Item { kind, id, title, subtitle, picture }
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
