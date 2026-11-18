//! The Crate: things to check out later (tracks, albums, artists, or a note of a
//! name heard somewhere), from any source, each new until it's marked heard.
//!
//! Kept in the `crate` [`Log`] (this computer's `crate.jsonl`, or every computer's
//! file in a sync folder) as changes, one per line: an entry added, changed (heard,
//! its note) or removed, each with when. Reading replays every computer's in time
//! order. A removal stays in the log so another computer's copy of the entry can't
//! bring it back.

use crate::model::{Art, Track};
use crate::player::saved::{decode_track, encode_track};
use crate::ui::sync::Log;
use serde_json::{Value, json};
use std::collections::HashMap;

/// This computer's file is rewritten once it has this many more lines than entries.
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
        /// What it is: [`About`] filled in when saved, or looked up later.
        about: About,
    },
    Artist {
        id: String,
        name: String,
        picture: Option<Art>,
    },
    /// Just the note: a name to look for, a link, anything.
    Note,
}

/// What kind of release an album is, when it came out and how long it is; empty
/// (and 0) until known.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct About {
    /// "Album", "EP", "Single", "Live", "Compilation".
    pub kind: String,
    /// "2026".
    pub year: String,
    pub tracks: u32,
}

impl About {
    /// Not looked up yet: saved before dust kept it, or found without it.
    pub fn unknown(&self) -> bool {
        self.tracks == 0
    }

    /// "EP · 5 tracks", "Album", "1 track"; empty when nothing is known.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.kind.is_empty() {
            parts.push(self.kind.clone());
        }
        match self.tracks {
            0 => {}
            1 => parts.push("1 track".into()),
            n => parts.push(format!("{n} tracks")),
        }
        parts.join(" · ")
    }
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
            Thing::Album { id, title, artist, picture, about } => json!({
                "kind": "album", "id": id, "title": title, "artist": artist, "art": picture.as_ref().map(Art::as_str),
                "type": about.kind, "year": about.year, "tracks": about.tracks,
            }),
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
            "album" => Thing::Album {
                id: v["id"].as_str()?.into(),
                title: s("title"),
                artist: s("artist"),
                picture: art(),
                about: About { kind: s("type"), year: s("year"), tracks: v["tracks"].as_u64().unwrap_or(0) as u32 },
            },
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

    /// The change and when it was made.
    fn decode(v: &Value) -> Option<(u64, Self)> {
        let at = v["at"].as_u64().unwrap_or(0);
        Some((at, Self::decode_change(v)?))
    }

    fn decode_change(v: &Value) -> Option<Self> {
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

/// The entries, newest first, and the log they're kept in.
#[derive(Default)]
pub struct Crate {
    entries: Vec<Entry>,
    /// When each entry last changed, so rewriting the log keeps the time.
    changed: HashMap<String, u64>,
    /// Ids removed, and when: an add for one of them (from another computer) is
    /// ignored.
    removed: Vec<(String, u64)>,
    log: Log,
    /// Lines in this computer's file.
    lines: usize,
    /// A change that couldn't be written (the sync folder unreachable): the file
    /// is written afresh before it is read again, so the change isn't lost.
    unsaved: bool,
}

impl Crate {
    /// The Crate in `log`: every computer's changes, in time order.
    pub fn load(log: Log) -> Self {
        let mut changes: Vec<(u64, Change)> =
            log.read().iter().filter_map(|l| serde_json::from_str(l).ok()).filter_map(|v| Change::decode(&v)).collect();
        changes.sort_by_key(|(at, _)| *at);
        let lines = log.own_lines();
        let mut store = Crate { log, lines, ..Default::default() };
        for (at, change) in changes {
            store.apply(at, change);
        }
        store
    }

    /// Read the log again (another computer changed it). Changes not yet written
    /// are written first; while they can't be, what's here is kept as it is.
    pub fn reload(&mut self) {
        if self.unsaved {
            self.rewrite();
            if self.unsaved {
                return;
            }
        }
        *self = Crate::load(std::mem::take(&mut self.log));
    }

    /// Keep the Crate in `log` from now on, starting it with what's here.
    pub fn move_to(&mut self, log: Log) {
        self.log = log;
        self.rewrite();
        self.reload();
    }

    pub fn log(&self) -> &Log {
        &self.log
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
            id: new_id(),
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

    /// Fill in what an album entry is (looked up after it was saved). Goes in the
    /// log as the entry again, which older versions of dust read too.
    pub fn set_about(&mut self, id: &str, about: About) {
        let Some(mut entry) = self.get(id).cloned() else { return };
        let Thing::Album { about: known, .. } = &mut entry.thing else { return };
        if *known == about {
            return;
        }
        *known = about;
        self.change(Change::Add(Box::new(entry)));
    }

    /// Apply `change` and add it to the log.
    pub fn change(&mut self, change: Change) {
        let at = unix_now();
        let line = change.encode(at).to_string();
        self.apply(at, change);
        if self.unsaved || self.lines + 1 > self.entries.len() + self.removed.len() + SLACK {
            self.rewrite();
        } else if self.log.append(&line) {
            self.lines += 1;
        } else {
            self.unsaved = true;
        }
    }

    fn apply(&mut self, at: u64, change: Change) {
        match change {
            Change::Add(entry) => {
                let entry = *entry;
                if self.removed.iter().any(|(id, _)| *id == entry.id) {
                    return;
                }
                let id = entry.id.clone();
                match self.entries.iter_mut().find(|e| e.id == id) {
                    // Another computer's newer copy of it.
                    Some(known) => *known = Entry { saved: known.saved, ..entry },
                    None => {
                        let at = self.entries.iter().position(|e| e.saved <= entry.saved).unwrap_or(self.entries.len());
                        self.entries.insert(at, entry);
                    }
                }
                self.changed.insert(id, at);
            }
            Change::Heard { id, heard } => {
                self.entries.iter_mut().filter(|e| e.id == id).for_each(|e| e.heard = heard);
                self.changed.insert(id, at);
            }
            Change::Note { id, note } => {
                self.entries.iter_mut().filter(|e| e.id == id).for_each(|e| e.note = note.clone());
                self.changed.insert(id, at);
            }
            Change::Remove { id } => {
                self.entries.retain(|e| e.id != id);
                self.changed.remove(&id);
                if !self.removed.iter().any(|(r, _)| *r == id) {
                    self.removed.push((id, at));
                }
            }
        }
    }

    /// The Crate as the log's lines: one add per entry (at when it last changed),
    /// and the removals.
    pub fn lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .entries
            .iter()
            .rev()
            .map(|e| Change::Add(Box::new(e.clone())).encode(self.changed.get(&e.id).copied().unwrap_or(e.saved)).to_string())
            .collect();
        lines.extend(self.removed.iter().map(|(id, at)| Change::Remove { id: id.clone() }.encode(*at).to_string()));
        lines
    }

    /// Write this computer's file afresh.
    fn rewrite(&mut self) {
        self.unsaved = !self.log.rewrite(&self.lines());
        if !self.unsaved {
            self.lines = self.entries.len() + self.removed.len();
        }
    }

    /// Make the Crate what `backup` holds: its entries back (new ids for ones
    /// removed since), others removed. Changes, so other computers follow.
    pub fn restore(&mut self, backup: &Crate) {
        let gone: Vec<String> = self.entries.iter().filter(|e| backup.get(&e.id).is_none()).map(|e| e.id.clone()).collect();
        for id in gone {
            self.change(Change::Remove { id });
        }
        for entry in backup.entries.iter().rev() {
            match self.get(&entry.id).cloned() {
                Some(now) => {
                    if now.heard != entry.heard {
                        self.change(Change::Heard { id: entry.id.clone(), heard: entry.heard });
                    }
                    if now.note != entry.note {
                        self.change(Change::Note { id: entry.id.clone(), note: entry.note.clone() });
                    }
                }
                None => {
                    let removed = self.removed.iter().any(|(id, _)| *id == entry.id);
                    let id = if removed { new_id() } else { entry.id.clone() };
                    self.change(Change::Add(Box::new(Entry { id, ..entry.clone() })));
                }
            }
        }
    }
}

fn new_id() -> String {
    format!("{:016x}", fastrand::u64(..))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeezerSong, Origin, TrackId};
    use std::path::{Path, PathBuf};

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

    fn log(dir: &Path) -> Log {
        Log::new("crate", Some(dir), None)
    }

    #[test]
    fn the_crate_survives_a_restart() {
        let dir = dir("restart");
        let mut c = Crate::load(log(&dir));
        let song = c.add("deezer", Thing::Track(track(1)), "Flow · chill", "");
        let note = c.add("deezer", Thing::Note, "", "  that Turkish psych band  ");
        let album = Thing::Album {
            id: "302127".into(),
            title: "Wild God".into(),
            artist: "Nick Cave".into(),
            picture: None,
            about: About::default(),
        };
        let gone = c.add("deezer", album, "", "from Ana");
        c.change(Change::Heard { id: song.clone(), heard: true });
        c.change(Change::Note { id: song.clone(), note: "great bass".into() });
        c.change(Change::Remove { id: gone });
        let again = Crate::load(log(&dir));
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
    fn changes_made_while_the_sync_folder_is_away_are_kept() {
        use crate::ui::sync::SyncDir;
        let root = dir("away");
        let mut c = Crate::load(Log::new("crate", None, Some(&SyncDir::new(&root, "Air-0001"))));
        let id = c.add("deezer", Thing::Track(track(1)), "", "");
        c.reload();
        assert!(c.get(&id).is_some(), "a reload while it can't be written keeps it");
        std::fs::create_dir_all(&root).unwrap();
        c.reload();
        assert!(c.get(&id).is_some(), "and writes it once the folder is back");
        assert!(Crate::load(c.log().clone()).get(&id).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_album_keeps_what_it_is_and_old_entries_learn_it() {
        let dir = dir("about");
        let mut c = Crate::load(log(&dir));
        let album = |about| Thing::Album { id: "1".into(), title: "Sun".into(), artist: "A".into(), picture: None, about };
        let ep = About { kind: "EP".into(), year: "2026".into(), tracks: 5 };
        let known = c.add("deezer", album(ep.clone()), "", "");
        // Saved by an older dust: no type, year or tracks in the log.
        let line = json!({"op": "add", "at": 1, "id": "old", "source": "deezer", "saved": 1,
            "thing": {"kind": "album", "id": "2", "title": "Moon", "artist": "B"}});
        assert!(c.log.append(&line.to_string()));
        let mut c = Crate::load(log(&dir));
        let about = |c: &Crate, id: &str| match &c.get(id).unwrap().thing {
            Thing::Album { about, .. } => about.clone(),
            _ => unreachable!(),
        };
        assert_eq!(about(&c, &known), ep);
        assert_eq!(ep.summary(), "EP · 5 tracks");
        assert!(about(&c, "old").unknown());
        assert_eq!(about(&c, "old").summary(), "");
        c.change(Change::Heard { id: "old".into(), heard: true });
        let single = About { kind: "Single".into(), year: "2025".into(), tracks: 1 };
        c.set_about("old", single.clone());
        let again = Crate::load(log(&dir));
        assert_eq!(about(&again, "old"), single);
        assert_eq!(single.summary(), "Single · 1 track");
        assert!(again.get("old").unwrap().heard, "what it is changes, nothing else");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_computers_share_the_crate() {
        use crate::ui::sync::SyncDir;
        let root = dir("two");
        std::fs::create_dir_all(&root).unwrap();
        let on = |name: &str, device: &str| Log::new("crate", Some(&root.join(name)), Some(&SyncDir::new(&root, device)));
        let mut air = Crate::load(on("air", "Air-0001"));
        let id = air.add("deezer", Thing::Track(track(1)), "", "");
        let mut imac = Crate::load(on("imac", "iMac-0002"));
        assert!(imac.get(&id).is_some(), "the iMac sees what the Air saved");
        imac.change(Change::Heard { id: id.clone(), heard: true });
        air.reload();
        assert!(air.get(&id).unwrap().heard);
        // The Air compacting its file keeps the iMac's change.
        air.rewrite();
        imac.reload();
        assert!(imac.get(&id).unwrap().heard);
        imac.change(Change::Remove { id: id.clone() });
        air.reload();
        air.rewrite();
        imac.reload();
        assert!(air.get(&id).is_none() && imac.get(&id).is_none(), "removed everywhere, for good");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_backup_is_brought_back() {
        let mut c = Crate::default();
        let kept = c.add("deezer", Thing::Track(track(1)), "", "");
        let gone = c.add("deezer", Thing::Track(track(2)), "", "");
        let mut backup = Crate::default();
        for line in c.lines() {
            let (at, change) = Change::decode(&serde_json::from_str(&line).unwrap()).unwrap();
            backup.apply(at, change);
        }
        c.change(Change::Remove { id: gone });
        c.change(Change::Heard { id: kept.clone(), heard: true });
        let later = c.add("deezer", Thing::Track(track(3)), "", "");
        c.restore(&backup);
        assert!(c.get(&later).is_none());
        assert!(!c.get(&kept).unwrap().heard);
        let titles: Vec<&str> = c.entries().iter().map(Entry::title).collect();
        assert_eq!(titles.len(), 2);
        assert!(titles.contains(&"Song 2"), "the removed one is back, under a new id");
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
        c.apply(u64::MAX, added);
        assert!(c.get(&id).is_none(), "an old add of it doesn't bring it back");
    }

    #[test]
    fn the_log_is_compacted() {
        let dir = dir("compact");
        let mut c = Crate::load(log(&dir));
        let id = c.add("deezer", Thing::Track(track(1)), "", "");
        for i in 0..(SLACK + 10) {
            c.change(Change::Heard { id: id.clone(), heard: i.is_multiple_of(2) });
        }
        let lines = std::fs::read_to_string(dir.join("crate.jsonl")).unwrap().lines().count();
        assert!(lines < SLACK, "{lines} lines");
        assert_eq!(Crate::load(log(&dir)).entries(), c.entries());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
