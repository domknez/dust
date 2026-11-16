//! Play history: every track heard, from every source, kept on this computer in
//! `history.jsonl` next to `settings.conf`, one line per play, newest last. The
//! side panel's *history* tab shows the current source's, newest first, by day.

use crate::model::Track;
use crate::player::saved::{decode_track, encode_track};
use crate::player::{Flow, Played};
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Plays kept, across sources.
pub const KEEP: usize = 2000;
/// The file is rewritten with the last [`KEEP`] once it holds this many lines.
const COMPACT_AT: usize = 2 * KEEP;
const FILE: &str = "history.jsonl";

/// One track heard.
#[derive(Clone, Debug, PartialEq)]
pub struct Play {
    /// The source's [`key`](crate::source::SourceId::key): which one plays it.
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
    pub fn new(source: &str, played: Played) -> Self {
        Play {
            source: source.to_string(),
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
            "track": encode_track(&self.track),
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

/// The plays kept, oldest first, and the file they're kept in (None: memory only).
#[derive(Default)]
pub struct Plays {
    plays: Vec<Play>,
    file: Option<PathBuf>,
    /// Lines in the file, to know when to compact it.
    lines: usize,
}

impl Plays {
    /// The history kept in `dir`, if any; unreadable lines are skipped.
    pub fn load(dir: Option<&Path>) -> Self {
        let Some(file) = dir.map(|d| d.join(FILE)) else { return Plays::default() };
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let mut plays: Vec<Play> = lines.iter().filter_map(|l| serde_json::from_str(l).ok()).filter_map(|v| Play::decode(&v)).collect();
        if plays.len() > KEEP {
            plays.drain(..plays.len() - KEEP);
        }
        Plays { plays, file: Some(file), lines: lines.len() }
    }

    /// Keep `play` and add it to the file.
    pub fn record(&mut self, play: Play) {
        let line = play.encode().to_string();
        self.plays.push(play);
        if self.plays.len() > KEEP {
            self.plays.remove(0);
        }
        let Some(file) = &self.file else { return };
        if self.lines + 1 >= COMPACT_AT {
            self.rewrite();
            return;
        }
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let appended = std::fs::OpenOptions::new().create(true).append(true).open(file).and_then(|mut f| writeln!(f, "{line}"));
        match appended {
            Ok(()) => self.lines += 1,
            Err(e) => log_warn!("could not add to the play history at {}: {e}", file.display()),
        }
    }

    /// Write the file afresh with what's kept.
    fn rewrite(&mut self) {
        let Some(file) = &self.file else { return };
        let text: String = self.plays.iter().map(|p| p.encode().to_string() + "\n").collect();
        // Write then rename, so a crash mid-write can't lose the history.
        let tmp = file.with_extension("jsonl.tmp");
        match std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, file)) {
            Ok(()) => self.lines = self.plays.len(),
            Err(e) => log_warn!("could not rewrite the play history at {}: {e}", file.display()),
        }
    }

    /// The plays of `source`, newest first.
    pub fn of(&self, source: &str) -> impl Iterator<Item = &Play> {
        self.plays.iter().rev().filter(move |p| p.source == source)
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

    fn play(source: &str, id: u64) -> Play {
        Play { source: source.into(), at: 1_700_000_000 + id, heard_secs: 30, skipped: id.is_multiple_of(2), flow: None, track: track(id) }
    }

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dust-plays-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn plays_survive_a_restart() {
        let dir = dir("restart");
        let mut plays = Plays::load(Some(&dir));
        plays.record(play("deezer", 1));
        let mut flow = play("deezer", 2);
        flow.flow = Some(Flow { mood: Some("chill".into()) });
        plays.record(flow.clone());
        plays.record(play("navidrome", 3));
        let again = Plays::load(Some(&dir));
        assert_eq!(again.of("deezer").cloned().collect::<Vec<_>>(), [flow, play("deezer", 1)], "newest first, this source's");
        assert_eq!(again.of("navidrome").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_last_plays_are_kept() {
        let dir = dir("keep");
        let mut plays = Plays::load(Some(&dir));
        for id in 0..(COMPACT_AT as u64 + 5) {
            plays.record(play("deezer", id));
        }
        assert_eq!(plays.of("deezer").count(), KEEP);
        assert_eq!(plays.of("deezer").next().map(|p| p.track.id.clone()), Some(TrackId::from(COMPACT_AT as u64 + 4)));
        let lines = std::fs::read_to_string(dir.join(FILE)).unwrap().lines().count();
        assert!(lines < COMPACT_AT, "the file was compacted, {lines} lines");
        assert_eq!(Plays::load(Some(&dir)).of("deezer").count(), KEEP);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_lines_are_skipped() {
        let dir = dir("broken");
        std::fs::create_dir_all(&dir).unwrap();
        let good = play("deezer", 1).encode().to_string();
        std::fs::write(dir.join(FILE), format!("not json\n{good}\n{{\"source\":\"deezer\"}}\n")).unwrap();
        assert_eq!(Plays::load(Some(&dir)).of("deezer").count(), 1);
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
