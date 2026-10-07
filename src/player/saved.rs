//! The queue saved between runs (`queue.json` next to `settings.conf`): tracks, the
//! current one, where it was, and whether it was endless Flow.

use super::Status;
use crate::deezer::{ArtistRef, Track};
use crate::settings;
use serde_json::{Value, json};
use std::path::PathBuf;

const FILE: &str = "queue.json";
const VERSION: u64 = 1;

/// A queue to restore.
#[derive(Debug, PartialEq)]
pub struct Saved {
    pub tracks: Vec<Track>,
    pub index: usize,
    pub position: f64,
    /// Some(mood) when it was endless Flow (None = plain Flow).
    pub flow: Option<Option<String>>,
}

fn path() -> Option<PathBuf> {
    Some(settings::dir()?.join(FILE))
}

/// Write the player's queue; an empty queue removes the file.
pub fn save(st: &Status) {
    let Some(path) = path() else { return };
    if st.queue.is_empty() {
        let _ = std::fs::remove_file(&path);
        return;
    }
    let text = encode(st).to_string();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Write then rename, so a crash mid-write can't leave a broken file.
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, &path)) {
        log_warn!("could not save the queue to {}: {e}", path.display());
    }
}

pub fn load() -> Option<Saved> {
    let text = std::fs::read_to_string(path()?).ok()?;
    let saved = decode(&serde_json::from_str(&text).ok()?);
    if saved.is_none() {
        log_warn!("ignoring an unreadable saved queue");
    }
    saved
}

fn encode(st: &Status) -> Value {
    let flow = st.flow.as_ref().map(|id| if id == "default" { Value::Null } else { Value::String(id.clone()) });
    json!({
        "version": VERSION,
        "index": st.index,
        "position": st.position,
        "flow": flow.map(|mood| json!({ "mood": mood })),
        "tracks": st.queue.iter().map(encode_track).collect::<Vec<_>>(),
    })
}

fn decode(v: &Value) -> Option<Saved> {
    if v["version"].as_u64()? != VERSION {
        return None;
    }
    let tracks: Vec<Track> = v["tracks"].as_array()?.iter().filter_map(decode_track).collect();
    let index = v["index"].as_u64()? as usize;
    if index >= tracks.len() {
        return None;
    }
    let flow = v["flow"].as_object().map(|f| f.get("mood").and_then(Value::as_str).map(str::to_string));
    Some(Saved { tracks, index, position: v["position"].as_f64().unwrap_or(0.0).max(0.0), flow })
}

fn encode_track(t: &Track) -> Value {
    json!({
        "id": t.id,
        "title": t.title,
        "artist": t.artist,
        "artists": t.artists.iter().map(|a| json!({"id": a.id, "name": a.name})).collect::<Vec<_>>(),
        "album": t.album,
        "album_id": t.album_id,
        "duration": t.duration,
        "token": t.token,
        "cover": t.cover,
        "fallback": t.fallback.as_ref().map(|(id, token)| json!({"id": id, "token": token})),
        "available": t.available,
        "available_from": t.available_from,
    })
}

fn decode_track(v: &Value) -> Option<Track> {
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    let artists = v["artists"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| Some(ArtistRef { id: x["id"].as_str()?.into(), name: x["name"].as_str()?.into() })).collect())
        .unwrap_or_default();
    let fallback = v["fallback"].as_object().and_then(|f| Some((f.get("id")?.as_u64()?, f.get("token")?.as_str()?.to_string())));
    Some(Track {
        id: v["id"].as_u64()?,
        title: s("title"),
        artist: s("artist"),
        artists,
        album: s("album"),
        album_id: s("album_id"),
        duration: v["duration"].as_u64().unwrap_or(0) as u32,
        token: s("token"),
        cover: s("cover"),
        fallback,
        // Files from before 0.10 didn't record it; those tracks were playable then.
        available: v["available"].as_bool().unwrap_or(true),
        available_from: s("available_from"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn track(id: u64) -> Track {
        Track {
            id,
            title: format!("Song {id}"),
            artist: "A, B".into(),
            artists: vec![ArtistRef { id: "27".into(), name: "A".into() }],
            album: "Album".into(),
            album_id: "302127".into(),
            duration: 200,
            token: "tok".into(),
            cover: "md5".into(),
            fallback: Some((9, "tok2".into())),
            available: true,
            available_from: String::new(),
        }
    }

    fn status(flow: Option<&str>) -> Status {
        Status { queue: Arc::new(vec![track(1), track(2)]), index: 1, position: 42.5, flow: flow.map(str::to_string), ..Default::default() }
    }

    #[test]
    fn round_trips() {
        let saved = decode(&encode(&status(None))).unwrap();
        assert_eq!(saved, Saved { tracks: vec![track(1), track(2)], index: 1, position: 42.5, flow: None });
        assert_eq!(decode(&encode(&status(Some("default")))).unwrap().flow, Some(None));
        assert_eq!(decode(&encode(&status(Some("chill")))).unwrap().flow, Some(Some("chill".into())));
    }

    #[test]
    fn rejects_unusable_files() {
        assert!(decode(&json!({"version": 99, "index": 0, "tracks": []})).is_none());
        let mut v = encode(&status(None));
        v["index"] = json!(5);
        assert!(decode(&v).is_none(), "index past the end");
    }
}
