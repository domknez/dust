//! Play history: every track heard, from every source, one line per play in the
//! `history` [`Log`] (this computer's `history.jsonl`, or every computer's file in a
//! sync folder). The side panel's *history* tab shows the current source's, newest
//! first, by day.

use crate::model::{Track, TrackId};
use crate::player::saved::{decode_track, encode_track};
use crate::player::{Flow, Played};
use crate::ui::sync::Log;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// Plays kept, across sources and computers: about a year of listening, for the
/// stats.
pub const KEEP: usize = 15_000;
/// This computer's file is rewritten with the last [`KEEP`] once it holds this many
/// lines.
const COMPACT_AT: usize = KEEP + 1000;

/// One track heard.
#[derive(Clone, Debug, PartialEq)]
pub struct Play {
    /// The source's [`data_key`](crate::source::Source::data_key): which one (and
    /// which Deezer family member) played it.
    pub source: String,
    /// When it started, Unix seconds.
    pub at: u64,
    pub heard_secs: u64,
    pub skipped: bool,
    /// The endless Flow (or Radio) it came from.
    pub flow: Option<Flow>,
    pub track: Track,
}

impl Play {
    /// What makes two plays the same one (in two files, or a backup).
    fn identity(&self) -> (u64, &str, &TrackId) {
        (self.at, &self.source, &self.track.id)
    }

    pub fn new(played: Played) -> Self {
        Play {
            source: played.source,
            at: played.started_unix,
            heard_secs: played.heard_secs,
            skipped: played.skipped,
            flow: played.flow,
            track: played.track,
        }
    }

    fn encode(&self) -> Value {
        json!({
            "source": self.source,
            "at": self.at,
            "heard": self.heard_secs,
            "skipped": self.skipped,
            "flow": self.flow.as_ref().map(|f| json!({ "mood": f.mood })),
            "track": slim(encode_track(&self.track)),
        })
    }

    fn decode(v: &Value) -> Option<Self> {
        Some(Play {
            source: v["source"].as_str()?.to_string(),
            at: v["at"].as_u64()?,
            heard_secs: v["heard"].as_u64().unwrap_or(0),
            skipped: v["skipped"].as_bool().unwrap_or(false),
            flow: v["flow"].as_object().map(|f| Flow { mood: f.get("mood").and_then(Value::as_str).map(str::to_string) }),
            track: decode_track(&v["track"])?,
        })
    }
}

/// A track as the history keeps it: without the Deezer stream token, which is most
/// of its size and expires anyway (playing it again fetches a fresh one).
fn slim(mut track: Value) -> Value {
    if let Some(token) = track.get_mut("token").filter(|t| t.is_string()) {
        *token = json!("");
    }
    track
}

/// The plays kept, oldest first, and the log they're kept in.
#[derive(Default)]
pub struct Plays {
    plays: Vec<Play>,
    /// The tracks and albums in `plays`, by source (a Deezer family member's apart).
    heard: HashMap<String, Heard>,
    log: Log,
    /// Lines in this computer's file, to know when to compact it.
    lines: usize,
    /// A play that couldn't be written (the sync folder unreachable): the file is
    /// written afresh before it is read again, so the play isn't lost.
    unsaved: bool,
    /// Goes up with every change, so views can keep what they work out from the
    /// plays until it does.
    revision: u64,
}

impl Plays {
    /// The history in `log`, every computer's merged; unreadable lines are skipped.
    pub fn load(log: Log) -> Self {
        let mut plays: Vec<Play> =
            log.read().iter().filter_map(|l| serde_json::from_str(l).ok()).filter_map(|v| Play::decode(&v)).collect();
        plays.sort_by_key(|p| p.at);
        // The same play in two files (a computer's file rewritten with all of them).
        plays.dedup_by(|a, b| a.identity() == b.identity());
        if plays.len() > KEEP {
            plays.drain(..plays.len() - KEEP);
        }
        let lines = log.own_lines();
        let mut heard = HashMap::<String, Heard>::new();
        plays.iter().for_each(|p| heard.entry(p.source.clone()).or_default().add(p));
        Plays { plays, heard, log, lines, unsaved: false, revision: 0 }
    }

    /// Read the log again (another computer added to it). Plays not yet written
    /// are written first; while they can't be, what's here is kept as it is.
    pub fn reload(&mut self) {
        if self.unsaved {
            self.rewrite();
            if self.unsaved {
                return;
            }
        }
        let revision = self.revision + 1;
        *self = Plays::load(std::mem::take(&mut self.log));
        self.revision = revision;
    }

    /// Keep the history in `log` from now on, starting it with what's kept here.
    pub fn move_to(&mut self, log: Log) {
        self.log = log;
        self.rewrite();
        self.reload();
    }

    /// Keep `play` and add it to the log.
    pub fn record(&mut self, play: Play) {
        self.revision += 1;
        let line = play.encode().to_string();
        self.heard.entry(play.source.clone()).or_default().add(&play);
        let at = self.plays.partition_point(|p| p.at <= play.at);
        self.plays.insert(at, play);
        self.keep_latest();
        if self.unsaved || self.lines + 1 >= COMPACT_AT {
            self.rewrite();
        } else if self.log.append(&line) {
            self.lines += 1;
        } else {
            self.unsaved = true;
        }
    }

    /// Keep the last [`KEEP`] plays; the tracks and albums of those dropped are no
    /// longer heard, as they wouldn't be after a restart.
    fn keep_latest(&mut self) {
        let Some(over) = self.plays.len().checked_sub(KEEP).filter(|&n| n > 0) else { return };
        for dropped in self.plays.drain(..over) {
            if let Some(heard) = self.heard.get_mut(&dropped.source) {
                heard.remove(&dropped);
            }
        }
    }

    /// The history as the log's lines, oldest first.
    pub fn lines(&self) -> Vec<String> {
        self.plays.iter().map(|p| p.encode().to_string()).collect()
    }

    /// Write this computer's file afresh with what's kept.
    fn rewrite(&mut self) {
        self.unsaved = !self.log.rewrite(&self.lines());
        if !self.unsaved {
            self.lines = self.plays.len();
        }
    }

    /// The plays of `source`, oldest first.
    pub fn oldest_first(&self, source: &str) -> Vec<&Play> {
        self.plays.iter().filter(|p| p.source == source).collect()
    }

    /// The plays of `source`, newest first.
    pub fn of(&self, source: &str) -> impl Iterator<Item = &Play> {
        self.plays.iter().rev().filter(move |p| p.source == source)
    }

    /// Whether `id` was played before from `source` (on any computer; the plays kept).
    pub fn has_heard(&self, source: &str, id: &TrackId) -> bool {
        self.heard.get(source).is_some_and(|h| h.tracks.contains_key(id))
    }

    /// Whether a track of album `id` was played before from `source`.
    pub fn has_heard_album(&self, source: &str, id: &str) -> bool {
        self.heard.get(source).is_some_and(|h| h.albums.contains_key(id))
    }

    /// The tracks played before from `source`, for Flow to leave out.
    pub fn heard(&self, source: &str) -> HashSet<TrackId> {
        self.heard.get(source).map(|h| h.tracks.keys().cloned().collect()).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.plays.len()
    }

    /// See [`Plays::revision`](Plays): changes whenever the plays do.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn log(&self) -> &Log {
        &self.log
    }

    /// Add the plays of a backup that aren't here.
    pub fn restore(&mut self, backup: &Plays) {
        let here: HashSet<(u64, &str, &TrackId)> = self.plays.iter().map(Play::identity).collect();
        let missing: Vec<Play> = backup.plays.iter().filter(|b| !here.contains(&b.identity())).cloned().collect();
        if missing.is_empty() {
            return;
        }
        self.revision += 1;
        missing.iter().for_each(|p| self.heard.entry(p.source.clone()).or_default().add(p));
        self.plays.extend(missing);
        self.plays.sort_by_key(|p| p.at);
        self.keep_latest();
        // One write for them all.
        self.rewrite();
    }
}

/// The tracks and albums of the plays kept, with how many plays each has, so
/// dropping a play forgets its track only with its last play.
#[derive(Default)]
struct Heard {
    tracks: HashMap<TrackId, usize>,
    albums: HashMap<String, usize>,
}

impl Heard {
    fn add(&mut self, play: &Play) {
        *self.tracks.entry(play.track.id.clone()).or_default() += 1;
        if !play.track.album_id.is_empty() {
            *self.albums.entry(play.track.album_id.clone()).or_default() += 1;
        }
    }

    fn remove(&mut self, play: &Play) {
        fn forget<K: std::hash::Hash + Eq>(counts: &mut HashMap<K, usize>, key: &K) {
            if let Some(n) = counts.get_mut(key) {
                *n -= 1;
                if *n == 0 {
                    counts.remove(key);
                }
            }
        }
        forget(&mut self.tracks, &play.track.id);
        forget(&mut self.albums, &play.track.album_id);
    }
}

/// "Flow", "Flow · chill", "Flow · classicrock" (a server's Radio with `radio`).
pub fn flow_label(flow: &Flow, radio: bool) -> String {
    let name = if radio { "Radio" } else { "Flow" };
    match &flow.mood {
        Some(mood) => format!("{name} · {}", mood.strip_prefix(crate::model::GENRE_FLOW).unwrap_or(mood).replace(['-', '_'], " ")),
        None => name.to_string(),
    }
}

/// When plays happened, as the history shows it: by local day, and the time of day.
pub mod when {
    use jiff::civil::Date;
    use jiff::tz::TimeZone;
    use jiff::{Timestamp, Zoned};

    pub use jiff::tz::TimeZone as Zone;

    /// This computer's time zone.
    pub fn zone() -> TimeZone {
        TimeZone::system()
    }

    fn local(unix: u64, zone: &TimeZone) -> Option<Zoned> {
        Some(Timestamp::from_second(unix as i64).ok()?.to_zoned(zone.clone()))
    }

    /// The local day `unix` falls on.
    pub fn day(unix: u64, zone: &TimeZone) -> Option<Date> {
        local(unix, zone).map(|z| z.date())
    }

    /// "21:47".
    pub fn clock(unix: u64, zone: &TimeZone) -> String {
        local(unix, zone).map(|z| z.strftime("%H:%M").to_string()).unwrap_or_default()
    }

    /// "Today", "Yesterday", "Monday" within the week, then "Sat 8 Nov" ("8 Nov
    /// 2025" in another year).
    pub fn day_name(day: Date, today: Date) -> String {
        match (today - day).get_days() {
            0 => "Today".into(),
            1 => "Yesterday".into(),
            2..=6 => day.strftime("%A").to_string(),
            _ if day.year() == today.year() => day.strftime("%a %-d %b").to_string(),
            _ => day.strftime("%-d %b %Y").to_string(),
        }
    }

    pub fn today(zone: &TimeZone) -> Date {
        Zoned::now().with_time_zone(zone.clone()).date()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeezerSong, Origin, TrackId};
    use std::path::{Path, PathBuf};

    fn track(id: u64) -> Track {
        Track::test(id)
    }

    fn play(source: &str, id: u64) -> Play {
        Play { source: source.into(), at: 1_700_000_000 + id, heard_secs: 30, skipped: id.is_multiple_of(2), flow: None, track: track(id) }
    }

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dust-plays-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn log(dir: &Path) -> Log {
        Log::new("history", Some(dir), None)
    }

    #[test]
    fn plays_survive_a_restart() {
        let dir = dir("restart");
        let mut plays = Plays::load(log(&dir));
        plays.record(play("deezer", 1));
        let mut flow = play("deezer", 2);
        flow.flow = Some(Flow { mood: Some("chill".into()) });
        plays.record(flow.clone());
        plays.record(play("navidrome", 3));
        let again = Plays::load(log(&dir));
        assert_eq!(again.of("deezer").cloned().collect::<Vec<_>>(), [flow, play("deezer", 1)], "newest first, this source's");
        assert_eq!(again.of("navidrome").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stream_tokens_are_not_kept() {
        let mut heard = play("deezer", 1);
        heard.track.origin = Origin::Deezer(DeezerSong { token: "a-long-token".into(), fallback: None });
        let line = heard.encode().to_string();
        assert!(!line.contains("a-long-token"));
        let back = Play::decode(&serde_json::from_str(&line).unwrap()).unwrap();
        assert_eq!(back.track.origin, Origin::Deezer(DeezerSong { token: String::new(), fallback: None }));
    }

    #[test]
    fn only_the_last_plays_are_kept() {
        let dir = dir("keep");
        let mut plays = Plays::load(log(&dir));
        for id in 0..(COMPACT_AT as u64 + 5) {
            plays.record(play("deezer", id));
        }
        assert_eq!(plays.of("deezer").count(), KEEP);
        assert_eq!(plays.of("deezer").next().map(|p| p.track.id.clone()), Some(TrackId::from(COMPACT_AT as u64 + 4)));
        assert!(!plays.has_heard("deezer", &TrackId::from(0)), "the oldest, dropped, isn't heard any more");
        assert!(plays.has_heard("deezer", &TrackId::from(COMPACT_AT as u64 + 4)));
        let lines = std::fs::read_to_string(dir.join("history.jsonl")).unwrap().lines().count();
        assert!(lines < COMPACT_AT, "the file was compacted, {lines} lines");
        assert_eq!(Plays::load(log(&dir)).of("deezer").count(), KEEP);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_computers_plays_are_merged_once() {
        use crate::ui::sync::SyncDir;
        let root = dir("two");
        std::fs::create_dir_all(&root).unwrap();
        let on = |device: &str| Log::new("history", Some(&root.join(device)), Some(&SyncDir::new(&root, device)));
        let mut air = Plays::load(on("Air-0001"));
        let mut imac = Plays::load(on("iMac-0002"));
        air.record(play("deezer", 3));
        imac.record(play("deezer", 1));
        imac.record(play("deezer", 5));
        air.reload();
        let order: Vec<_> = air.of("deezer").map(|p| p.track.id.to_string()).collect();
        assert_eq!(order, ["5", "3", "1"], "newest first, across computers");
        air.rewrite();
        imac.reload();
        assert_eq!(imac.len(), 3, "a play in two files is one play");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn plays_made_while_the_sync_folder_is_away_are_kept() {
        use crate::ui::sync::SyncDir;
        let root = dir("away");
        let mut plays = Plays::load(Log::new("history", None, Some(&SyncDir::new(&root, "Air-0001"))));
        plays.record(play("deezer", 1));
        plays.reload();
        assert_eq!(plays.len(), 1, "a reload while it can't be written keeps it");
        std::fs::create_dir_all(&root).unwrap();
        plays.reload();
        assert_eq!(plays.len(), 1, "and writes it once the folder is back");
        assert_eq!(Plays::load(plays.log().clone()).len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_backup_brings_back_the_plays_missing() {
        let dir = dir("restore");
        let mut backup = Plays::load(log(&dir.join("backup")));
        for id in [1, 2, 3] {
            backup.record(play("deezer", id));
        }
        let mut plays = Plays::load(log(&dir));
        plays.record(play("deezer", 2));
        let before = plays.revision();
        plays.restore(&backup);
        let ids: Vec<_> = plays.oldest_first("deezer").iter().map(|p| p.track.id.to_string()).collect();
        assert_eq!(ids, ["1", "2", "3"], "each once, in time order");
        assert!(plays.revision() > before && plays.has_heard("deezer", &TrackId::from(3)));
        assert_eq!(Plays::load(log(&dir)).len(), 3, "and written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_lines_are_skipped() {
        let dir = dir("broken");
        std::fs::create_dir_all(&dir).unwrap();
        let good = play("deezer", 1).encode().to_string();
        std::fs::write(dir.join("history.jsonl"), format!("not json\n{good}\n{{\"source\":\"deezer\"}}\n")).unwrap();
        assert_eq!(Plays::load(log(&dir)).of("deezer").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn days_have_names() {
        use jiff::civil::date;
        let today = date(2026, 11, 16);
        assert_eq!(when::day_name(today, today), "Today");
        assert_eq!(when::day_name(date(2026, 11, 15), today), "Yesterday");
        assert_eq!(when::day_name(date(2026, 11, 12), today), "Thursday");
        assert_eq!(when::day_name(date(2026, 11, 8), today), "Sun 8 Nov");
        assert_eq!(when::day_name(date(2025, 12, 31), today), "31 Dec 2025");
    }
}
