//! The Crate: things to check out later (tracks, albums, artists, or a note of a
//! name heard somewhere), from any source, each new until it's marked heard.
//!
//! Kept on this computer in `crate.jsonl` next to `settings.conf` as a log of
//! changes, one per line: an entry added, changed (heard, its note) or removed.
//! Reading replays them in order. A removal stays in the log (as long as it's
//! needed) so a copy of the log from elsewhere can't bring the entry back.

use crate::model::{Art, Track};
use crate::player::saved::{decode_track, encode_track};
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

const FILE: &str = "crate.jsonl";
/// The log is rewritten once it has this many more lines than entries.
const SLACK: usize = 200;

/// What an entry is.
#[derive(Clone, Debug, PartialEq)]
pub enum Thing {
    Track(Track),
    Album {
        id: String,
        title: String,
        artist: String,
        picture: Option<Art>,
    },
    Artist {
        id: String,
        name: String,
        picture: Option<Art>,
    },
    /// Just the note: a name to look for, a link, anything.
    Note,
}

impl Thing {
    /// "track", "album", ...: the log's name and the page's filter.
    pub fn kind(&self) -> &'static str {
        match self {
            Thing::Track(_) => "track",
            Thing::Album { .. } => "album",
            Thing::Artist { .. } => "artist",
            Thing::Note => "note",
        }
    }

    /// The source's id for it: the same thing saved twice is one entry.
    fn key(&self) -> Option<String> {
        match self {
            Thing::Track(t) => Some(format!("track:{}", t.id)),
            Thing::Album { id, .. } => Some(format!("album:{id}")),
            Thing::Artist { id, .. } => Some(format!("artist:{id}")),
            Thing::Note => None,
        }
    }

    fn encode(&self) -> Value {
        match self {
            Thing::Track(t) => json!({"kind": "track", "track": encode_track(t)}),
            Thing::Album { id, title, artist, picture } => {
                json!({"kind": "album", "id": id, "title": title, "artist": artist, "art": picture.as_ref().map(Art::as_str)})
            }
            Thing::Artist { id, name, picture } => {
                json!({"kind": "artist", "id": id, "name": name, "art": picture.as_ref().map(Art::as_str)})
            }
            Thing::Note => json!({"kind": "note"}),
        }
    }

    fn decode(v: &Value) -> Option<Self> {
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        let art = || v["art"].as_str().and_then(Art::new);
        Some(match v["kind"].as_str()? {
            "track" => Thing::Track(decode_track(&v["track"])?),
            "album" => Thing::Album { id: v["id"].as_str()?.into(), title: s("title"), artist: s("artist"), picture: art() },
            "artist" => Thing::Artist { id: v["id"].as_str()?.into(), name: s("name"), picture: art() },
            "note" => Thing::Note,
            _ => return None,
        })
    }
}

/// One thing in the Crate.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Its own id, random, for changes to point at.
    pub id: String,
    /// The [`key`](crate::source::SourceId::key) of the source it's on; empty for a
    /// note.
    pub source: String,
    /// When it was saved, Unix seconds.
    pub saved: u64,
    /// Where it was found ("Flow · chill", "History"); may be empty.
    pub from: String,
    pub note: String,
    pub heard: bool,
    pub thing: Thing,
}

impl Entry {
    /// What the entry is called: the track's, album's or artist's name, or the note.
    pub fn title(&self) -> &str {
        match &self.thing {
            Thing::Track(t) => &t.title,
            Thing::Album { title, .. } => title,
            Thing::Artist { name, .. } => name,
            Thing::Note => &self.note,
        }
    }

    /// Who it's by, for tracks and albums.
    pub fn by(&self) -> &str {
        match &self.thing {
            Thing::Track(t) => &t.artist,
            Thing::Album { artist, .. } => artist,
            Thing::Artist { .. } | Thing::Note => "",
        }
    }

    pub fn picture(&self) -> Option<&Art> {
        match &self.thing {
            Thing::Track(t) => t.art.as_ref(),
            Thing::Album { picture, .. } | Thing::Artist { picture, .. } => picture.as_ref(),
            Thing::Note => None,
        }
    }
}

/// A change to the Crate, as the log keeps it.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Add(Box<Entry>),
    Heard { id: String, heard: bool },
    Note { id: String, note: String },
    Remove { id: String },
}

impl Change {
    fn encode(&self, at: u64) -> Value {
        match self {
            Change::Add(e) => json!({
                "op": "add", "at": at, "id": e.id, "source": e.source, "saved": e.saved, "from": e.from,
                "note": e.note, "heard": e.heard, "thing": e.thing.encode(),
            }),
            Change::Heard { id, heard } => json!({"op": "heard", "at": at, "id": id, "heard": heard}),
            Change::Note { id, note } => json!({"op": "note", "at": at, "id": id, "note": note}),
            Change::Remove { id } => json!({"op": "remove", "at": at, "id": id}),
        }
    }

    fn decode(v: &Value) -> Option<Self> {
        let id = v["id"].as_str()?.to_string();
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        Some(match v["op"].as_str()? {
            "add" => Change::Add(Box::new(Entry {
                id,
                source: s("source"),
                saved: v["saved"].as_u64().or(v["at"].as_u64()).unwrap_or(0),
                from: s("from"),
                note: s("note"),
                heard: v["heard"].as_bool().unwrap_or(false),
                thing: Thing::decode(&v["thing"])?,
            })),
            "heard" => Change::Heard { id, heard: v["heard"].as_bool()? },
            "note" => Change::Note { id, note: s("note") },
            "remove" => Change::Remove { id },
            _ => return None,
        })
    }
}

/// The entries, newest first, and the log they're kept in (None: memory only).
#[derive(Default)]
pub struct Crate {
    entries: Vec<Entry>,
    /// Ids removed: an add for one of them (from another copy of the log) is ignored.
    removed: Vec<String>,
    file: Option<PathBuf>,
    lines: usize,
}

impl Crate {
    pub fn load(dir: Option<&Path>) -> Self {
        let Some(file) = dir.map(|d| d.join(FILE)) else { return Crate::default() };
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let mut store = Crate { file: Some(file), ..Default::default() };
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            store.lines += 1;
            if let Some(change) = serde_json::from_str(line).ok().and_then(|v| Change::decode(&v)) {
                store.apply(change);
            }
        }
        store
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// How many are still to be heard.
    pub fn new_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.heard).count()
    }

    /// The entry already holding `thing`, if it's saved.
    pub fn find(&self, source: &str, thing: &Thing) -> Option<&Entry> {
        let key = thing.key()?;
        self.entries.iter().find(|e| e.source == source && e.thing.key().as_ref() == Some(&key))
    }

    /// Save `thing` as a new entry; returns its id.
    pub fn add(&mut self, source: &str, thing: Thing, from: &str, note: &str) -> String {
        let entry = Entry {
            id: format!("{:016x}", fastrand::u64(..)),
            source: if thing == Thing::Note { String::new() } else { source.to_string() },
            saved: unix_now(),
            from: from.to_string(),
            note: note.trim().to_string(),
            heard: false,
            thing,
        };
        let id = entry.id.clone();
        self.change(Change::Add(Box::new(entry)));
        id
    }

    /// Apply `change` and add it to the log.
    pub fn change(&mut self, change: Change) {
        let line = change.encode(unix_now()).to_string();
        self.apply(change);
        let Some(file) = &self.file else { return };
        if self.lines + 1 > self.entries.len() + self.removed.len() + SLACK {
            self.rewrite();
            return;
        }
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let appended = std::fs::OpenOptions::new().create(true).append(true).open(file).and_then(|mut f| writeln!(f, "{line}"));
        match appended {
            Ok(()) => self.lines += 1,
            Err(e) => log_warn!("could not save the Crate to {}: {e}", file.display()),
        }
    }

    fn apply(&mut self, change: Change) {
        match change {
            Change::Add(entry) => {
                let entry = *entry;
                if !self.removed.contains(&entry.id) && self.get(&entry.id).is_none() {
                    let at = self.entries.iter().position(|e| e.saved <= entry.saved).unwrap_or(self.entries.len());
                    self.entries.insert(at, entry);
                }
            }
            Change::Heard { id, heard } => self.entries.iter_mut().filter(|e| e.id == id).for_each(|e| e.heard = heard),
            Change::Note { id, note } => self.entries.iter_mut().filter(|e| e.id == id).for_each(|e| e.note = note.clone()),
            Change::Remove { id } => {
                self.entries.retain(|e| e.id != id);
                if !self.removed.contains(&id) {
                    self.removed.push(id);
                }
            }
        }
    }

    /// Write the log afresh: one add per entry, and the removals.
    fn rewrite(&mut self) {
        let Some(file) = &self.file else { return };
        let now = unix_now();
        let mut text = String::new();
        for entry in self.entries.iter().rev() {
            text += &(Change::Add(Box::new(entry.clone())).encode(now).to_string() + "\n");
        }
        for id in &self.removed {
            text += &(Change::Remove { id: id.clone() }.encode(now).to_string() + "\n");
        }
        let tmp = file.with_extension("jsonl.tmp");
        match std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, file)) {
            Ok(()) => self.lines = self.entries.len() + self.removed.len(),
            Err(e) => log_warn!("could not rewrite the Crate at {}: {e}", file.display()),
        }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeezerSong, Origin, TrackId};

    fn track(id: u64) -> Track {
        Track {
            id: TrackId::from(id),
            title: format!("Song {id}"),
            artist: "A".into(),
            artists: vec![],
            album: "Album".into(),
            album_id: "1".into(),
            duration: 200,
            art: None,
            available: true,
            available_from: String::new(),
            origin: Origin::Deezer(DeezerSong { token: "t".into(), fallback: None }),
        }
    }

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dust-crate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_crate_survives_a_restart() {
        let dir = dir("restart");
        let mut c = Crate::load(Some(&dir));
        let song = c.add("deezer", Thing::Track(track(1)), "Flow · chill", "");
        let note = c.add("deezer", Thing::Note, "", "  that Turkish psych band  ");
        let album = Thing::Album { id: "302127".into(), title: "Wild God".into(), artist: "Nick Cave".into(), picture: None };
        let gone = c.add("deezer", album, "", "from Ana");
        c.change(Change::Heard { id: song.clone(), heard: true });
        c.change(Change::Note { id: song.clone(), note: "great bass".into() });
        c.change(Change::Remove { id: gone });
        let again = Crate::load(Some(&dir));
        assert_eq!(again.entries(), c.entries());
        assert_eq!(again.entries().len(), 2);
        let saved = again.get(&song).unwrap();
        assert!(saved.heard);
        assert_eq!((saved.note.as_str(), saved.from.as_str()), ("great bass", "Flow · chill"));
        let saved = again.get(&note).unwrap();
        assert_eq!((saved.title(), saved.source.as_str()), ("that Turkish psych band", ""));
        assert_eq!(again.new_count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_thing_is_found_once_saved() {
        let mut c = Crate::default();
        c.add("deezer", Thing::Track(track(1)), "", "");
        assert!(c.find("deezer", &Thing::Track(track(1))).is_some());
        assert!(c.find("navidrome", &Thing::Track(track(1))).is_none(), "another source's track 1");
        assert!(c.find("deezer", &Thing::Note).is_none(), "notes are never the same thing");
    }

    #[test]
    fn a_removed_entry_stays_removed() {
        let mut c = Crate::default();
        let id = c.add("deezer", Thing::Track(track(1)), "", "");
        let added = Change::Add(Box::new(c.get(&id).unwrap().clone()));
        c.change(Change::Remove { id: id.clone() });
        c.apply(added);
        assert!(c.get(&id).is_none(), "an old add of it doesn't bring it back");
    }

    #[test]
    fn the_log_is_compacted() {
        let dir = dir("compact");
        let mut c = Crate::load(Some(&dir));
        let id = c.add("deezer", Thing::Track(track(1)), "", "");
        for i in 0..(SLACK + 10) {
            c.change(Change::Heard { id: id.clone(), heard: i.is_multiple_of(2) });
        }
        let lines = std::fs::read_to_string(dir.join(FILE)).unwrap().lines().count();
        assert!(lines < SLACK, "{lines} lines");
        assert_eq!(Crate::load(Some(&dir)).entries(), c.entries());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
