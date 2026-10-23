//! The queue saved between runs (`queue.json` next to `settings.conf`): tracks, the
//! current one, where it was, and whether it was endless Flow. Each music source
//! keeps its own: Deezer's in `queue.json`, a music server's in `queue-<key>.json`.

use super::{Flow, Status};
use crate::model::{Art, ArtistRef, DeezerSong, Episode, Origin, ServerSong, Track, TrackId};
use crate::settings;
use serde_json::{Value, json};
use std::path::PathBuf;

/// The queue file of a source; `source` is empty for Deezer.
fn file(source: &str) -> String {
    let safe: String = source.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    if safe.is_empty() { "queue.json".into() } else { format!("queue-{safe}.json") }
}
const VERSION: u64 = 1;

/// A queue to restore.
#[derive(Debug, PartialEq)]
pub struct Saved {
    pub tracks: Vec<Track>,
    pub index: usize,
    pub position: f64,
    /// Set when it was endless Flow.
    pub flow: Option<Flow>,
}

/// Where the queue files are: next to the settings, or (tests) any folder.
#[derive(Clone)]
pub struct QueueFiles {
    dir: Option<PathBuf>,
}

impl QueueFiles {
    pub fn in_settings_dir() -> Self {
        QueueFiles { dir: settings::dir() }
    }

    #[cfg(test)]
    pub fn at(dir: PathBuf) -> Self {
        QueueFiles { dir: Some(dir) }
    }

    fn path(&self, source: &str) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join(file(source)))
    }

    /// Write the player's queue for `source` (see [`file`]); an empty queue removes
    /// the file.
    pub fn save(&self, st: &Status, source: &str) {
        let Some(path) = self.path(source) else { return };
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

    pub fn load(&self, source: &str) -> Option<Saved> {
        let text = std::fs::read_to_string(self.path(source)?).ok()?;
        let saved = decode(&serde_json::from_str(&text).ok()?);
        if saved.is_none() {
            log_warn!("ignoring an unreadable saved queue");
        }
        saved
    }
}

fn encode(st: &Status) -> Value {
    json!({
        "version": VERSION,
        "index": st.index,
        "position": st.position,
        "flow": st.flow.as_ref().map(|flow| json!({ "mood": flow.mood })),
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
    let flow = v["flow"].as_object().map(|f| Flow { mood: f.get("mood").and_then(Value::as_str).map(str::to_string) });
    Some(Saved { tracks, index, position: v["position"].as_f64().unwrap_or(0.0).max(0.0), flow })
}

/// A track as saved: every source's fields side by side, empty or null for the
/// ones a track doesn't have (the shape dust has always written). The picture is
/// `art`, as its source names it; files from 0.16 and before have `cover` instead
/// (see [`legacy_art`]).
///
/// The `id` is the source's: a number for Deezer (as always), text for a music
/// server. Files from 0.19 and before have a number standing for a server song's id
/// too; its real one is `server.id`, which is still written.
fn encode_track(t: &Track) -> Value {
    let (song, episode, server) = match &t.origin {
        Origin::Deezer(song) => (Some(song), None, None),
        Origin::Episode(episode) => (None, Some(episode), None),
        Origin::Server(server) => (None, None, Some(server)),
    };
    json!({
        "id": t.id.as_str().parse::<u64>().map_or_else(|_| json!(t.id.as_str()), Value::from),
        "title": t.title,
        "artist": t.artist,
        "artists": t.artists.iter().map(|a| json!({"id": a.id, "name": a.name})).collect::<Vec<_>>(),
        "album": t.album,
        "album_id": t.album_id,
        "duration": t.duration,
        "token": song.map(|s| s.token.as_str()).unwrap_or_default(),
        "art": t.art.as_ref().map(Art::as_str),
        "fallback": song.and_then(|s| s.fallback.as_ref()).map(|(id, token)| json!({"id": id, "token": token})),
        "available": t.available,
        "available_from": t.available_from,
        "episode": episode.map(|e| json!({"url": e.url, "show_id": e.show_id, "published": e.published})),
        "server": server.map(|s| json!({"id": t.id.as_str(), "suffix": s.suffix})),
    })
}

fn decode_track(v: &Value) -> Option<Track> {
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    let artists = v["artists"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| Some(ArtistRef { id: x["id"].as_str()?.into(), name: x["name"].as_str()?.into() })).collect())
        .unwrap_or_default();
    let episode = v["episode"].as_object().and_then(|e| {
        let s = |k: &str| e.get(k).and_then(Value::as_str).map(str::to_string);
        Some(Episode { url: s("url")?, show_id: s("show_id").unwrap_or_default(), published: s("published").unwrap_or_default() })
    });
    let server = v["server"].as_object().and_then(|s| {
        let text = |k: &str| s.get(k).and_then(Value::as_str).map(str::to_string);
        Some((TrackId::new(text("id")?), ServerSong { suffix: text("suffix").unwrap_or_default() }))
    });
    let id = match (&server, &v["id"]) {
        (Some((id, _)), _) => id.clone(),
        (None, Value::Number(n)) => TrackId::from(n.as_u64()?),
        (None, Value::String(id)) if !id.is_empty() => TrackId::new(id),
        _ => return None,
    };
    let origin = match (server, episode) {
        (Some((_, server)), _) => Origin::Server(server),
        (None, Some(episode)) => Origin::Episode(episode),
        (None, None) => {
            let fallback = v["fallback"].as_object().and_then(|f| Some((f.get("id")?.as_u64()?, f.get("token")?.as_str()?.to_string())));
            Origin::Deezer(DeezerSong { token: s("token"), fallback })
        }
    };
    Some(Track {
        id,
        title: s("title"),
        artist: s("artist"),
        artists,
        album: s("album"),
        album_id: s("album_id"),
        duration: v["duration"].as_u64().unwrap_or(0) as u32,
        art: match v["art"].as_str() {
            Some(name) => Art::new(name),
            None => legacy_art(&s("cover"), &origin),
        },
        // Files from before 0.10 didn't record it; those tracks were playable then.
        available: v["available"].as_bool().unwrap_or(true),
        available_from: s("available_from"),
        origin,
    })
}

/// The picture of a track saved by dust 0.16 or before, from its `cover`: a
/// server's cover art id behind `sub:`, else the md5 of a Deezer album cover (a
/// show's artwork for an episode) on Deezer's image CDN, whose [`Art`] is
/// `<family>/<md5>`.
fn legacy_art(cover: &str, origin: &Origin) -> Option<Art> {
    match origin {
        Origin::Server(_) => Art::new(cover.strip_prefix("sub:").unwrap_or(cover)),
        _ if cover.is_empty() => None,
        Origin::Episode(_) => Art::new(format!("talk/{cover}")),
        Origin::Deezer(_) => Art::new(format!("cover/{cover}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn track(id: u64) -> Track {
        Track {
            id: TrackId::from(id),
            title: format!("Song {id}"),
            artist: "A, B".into(),
            artists: vec![ArtistRef { id: "27".into(), name: "A".into() }],
            album: "Album".into(),
            album_id: "302127".into(),
            duration: 200,
            art: Art::new("cover/md5"),
            available: true,
            available_from: String::new(),
            origin: Origin::Deezer(DeezerSong { token: "tok".into(), fallback: Some((9, "tok2".into())) }),
        }
    }

    fn status(flow: Option<Flow>) -> Status {
        Status { queue: Arc::new(vec![track(1), track(2)]), index: 1, position: 42.5, flow, ..Default::default() }
    }

    #[test]
    fn round_trips() {
        let saved = decode(&encode(&status(None))).unwrap();
        assert_eq!(saved, Saved { tracks: vec![track(1), track(2)], index: 1, position: 42.5, flow: None });
        let plain = Flow { mood: None };
        assert_eq!(decode(&encode(&status(Some(plain.clone())))).unwrap().flow, Some(plain));
        let chill = Flow { mood: Some("chill".into()) };
        assert_eq!(decode(&encode(&status(Some(chill.clone())))).unwrap().flow, Some(chill));
    }

    #[test]
    fn episodes_keep_their_audio_file() {
        let episode = Track {
            origin: Origin::Episode(Episode {
                url: "https://example.com/ep.mp3".into(),
                show_id: "724752".into(),
                published: "2026-09-17".into(),
            }),
            ..track(3)
        };
        let st = Status { queue: Arc::new(vec![track(1), episode.clone()]), index: 1, ..Default::default() };
        assert_eq!(decode(&encode(&st)).unwrap().tracks, vec![track(1), episode]);
    }

    /// A server song with the server's own id.
    fn server_song(id: &str) -> Track {
        Track { id: TrackId::new(id), origin: Origin::Server(ServerSong { suffix: "flac".into() }), ..track(4) }
    }

    #[test]
    fn server_songs_keep_their_id() {
        let song = server_song("qPyR7XLJOiu93qWw1n06xy");
        let st = Status { queue: Arc::new(vec![song.clone()]), index: 0, ..Default::default() };
        let saved = encode(&st);
        assert_eq!(saved["tracks"][0]["id"], "qPyR7XLJOiu93qWw1n06xy");
        assert_eq!(saved["tracks"][0]["server"]["id"], "qPyR7XLJOiu93qWw1n06xy");
        assert_eq!(decode(&saved).unwrap().tracks, vec![song]);
        // A server that numbers its songs reads back the same.
        let numbered = server_song("1234");
        let st = Status { queue: Arc::new(vec![numbered.clone()]), index: 0, ..Default::default() };
        assert_eq!(decode(&encode(&st)).unwrap().tracks, vec![numbered]);
    }

    #[test]
    fn deezer_ids_stay_numbers() {
        let saved = encode(&status(None));
        assert_eq!(saved["tracks"][0]["id"], json!(1));
        let mut text_id = saved.clone();
        text_id["tracks"][0]["id"] = json!("1");
        assert_eq!(decode(&text_id).unwrap().tracks[0].id, TrackId::from(1));
        text_id["tracks"][0]["id"] = json!(null);
        text_id["index"] = json!(0);
        assert_eq!(decode(&text_id).unwrap().tracks.len(), 1, "a track without an id is dropped");
    }

    /// `queue.json` as dust 0.16 wrote it: a song with a regional fallback, a
    /// podcast episode, endless Flow tuned to a mood.
    const DEEZER_0_16: &str = r##"{"flow":{"mood":"chill"},"index":1,"position":61.3,"tracks":[
        {"album":"Discovery","album_id":"302127","artist":"Daft Punk","artists":[{"id":"27","name":"Daft Punk"}],
         "available":true,"available_from":"","cover":"2e018122cb56986277102d2041a592c8","duration":320,"episode":null,
         "fallback":{"id":3135554,"token":"AAAB-fallback"},"id":3135553,"server":null,"title":"One More Time","token":"AAAA-token"},
        {"album":"Lex Fridman Podcast","album_id":"","artist":"Lex Fridman Podcast","artists":[],"available":true,
         "available_from":"","cover":"fa87","duration":13430,
         "episode":{"published":"2026-09-17","show_id":"724752","url":"https://example.com/935346132.mp3"},
         "fallback":null,"id":935346132,"server":null,"title":"#480 – Episode","token":""}
    ],"version":1}"##;

    /// `queue-navidrome.json` as dust 0.16 wrote it.
    const NAVIDROME_0_16: &str = r##"{"flow":null,"index":0,"position":0.0,"tracks":[
        {"album":"Single Pulse","album_id":"5F81DCwcfjVByziJAH9maf","artist":"Square Wave Trio",
         "artists":[{"id":"1cX14wrZBmbXZQpEQ79FiO","name":"Square Wave Trio"}],"available":true,"available_from":"",
         "cover":"sub:al-5F81DCwcfjVByziJAH9maf_0","duration":20,"episode":null,"fallback":null,"id":8528590152792521616,
         "server":{"id":"qPyR7XLJOiu93qWw1n06xy","suffix":"wav"},"title":"Pulse","token":""}
    ],"version":1}"##;

    #[test]
    fn queues_saved_by_0_16_still_load() {
        let saved = decode(&serde_json::from_str(DEEZER_0_16).unwrap()).unwrap();
        assert_eq!((saved.index, saved.position, saved.flow.clone()), (1, 61.3, Some(Flow { mood: Some("chill".into()) })));
        let [song, episode] = &saved.tracks[..] else { panic!("two tracks") };
        assert_eq!(
            (song.id.as_str(), song.title.as_str(), song.album_id.as_str(), song.duration),
            ("3135553", "One More Time", "302127", 320)
        );
        assert_eq!(
            song.origin,
            Origin::Deezer(DeezerSong { token: "AAAA-token".into(), fallback: Some((3135554, "AAAB-fallback".into())) })
        );
        assert_eq!(
            episode.origin,
            Origin::Episode(Episode {
                url: "https://example.com/935346132.mp3".into(),
                show_id: "724752".into(),
                published: "2026-09-17".into()
            })
        );

        let saved = decode(&serde_json::from_str(NAVIDROME_0_16).unwrap()).unwrap();
        assert_eq!(saved.flow, None);
        let song = &saved.tracks[0];
        assert_eq!(song.id, TrackId::new("qPyR7XLJOiu93qWw1n06xy"), "the server's id, not the number standing for it");
        assert_eq!(song.origin, Origin::Server(ServerSong { suffix: "wav".into() }));
        assert_eq!(song.art, Art::new("al-5F81DCwcfjVByziJAH9maf_0"));
    }

    #[test]
    fn queues_keep_what_0_16_reads() {
        // An older dust reading a newer file (after a downgrade) finds every field
        // it knows but the picture, which it does without; a server song's `id` is
        // the server's own now (an older dust skips such a queue).
        for text in [DEEZER_0_16, NAVIDROME_0_16] {
            let old: Value = serde_json::from_str(text).unwrap();
            let saved = decode(&old).unwrap();
            let st = Status {
                queue: Arc::new(saved.tracks.clone()),
                index: saved.index,
                position: saved.position,
                flow: saved.flow.clone(),
                ..Default::default()
            };
            let mut new = encode(&st);
            for (track, old) in new["tracks"].as_array_mut().unwrap().iter_mut().zip(old["tracks"].as_array().unwrap()) {
                track.as_object_mut().unwrap().remove("art");
                track["cover"] = old["cover"].clone();
                if !old["server"].is_null() {
                    assert_eq!(track["id"], old["server"]["id"]);
                    track["id"] = old["id"].clone();
                }
            }
            assert_eq!(new, old);
            assert_eq!(decode(&encode(&st)).unwrap().tracks, saved.tracks, "and the new file reads back the same");
        }
    }

    #[test]
    fn each_source_has_its_own_file() {
        assert_eq!(file(""), "queue.json");
        assert_eq!(file("navidrome"), "queue-navidrome.json");
        assert_eq!(file("../x y"), "queue-___x_y.json");
    }

    #[test]
    fn rejects_unusable_files() {
        assert!(decode(&json!({"version": 99, "index": 0, "tracks": []})).is_none());
        let mut v = encode(&status(None));
        v["index"] = json!(5);
        assert!(decode(&v).is_none(), "index past the end");
    }
}
