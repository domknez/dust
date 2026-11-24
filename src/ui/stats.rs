//! Listening stats, worked out from the play history ([`Plays`](crate::ui::plays)):
//! time listened, top artists, albums and tracks, the artists heard for the first
//! time, listening by day or month, and what played this week a year ago.

use crate::model::Track;
use crate::ui::plays::{Play, when};
use jiff::ToSpan;
use jiff::civil::Date;
use std::collections::{HashMap, HashSet};

pub const DAY: u64 = 24 * 60 * 60;
const YEAR: u64 = 365 * DAY;
/// "A year ago" is the week around the day: this long before it and after it.
const YEAR_AGO_BEFORE: u64 = 4 * DAY;
const YEAR_AGO_AFTER: u64 = 3 * DAY;
/// Tracks shown from a year ago.
const YEAR_AGO_TOP: usize = 5;
/// Entries in each top list.
pub const TOP: usize = 10;

/// The stretch of time the stats cover, ending now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Period {
    #[default]
    Week,
    Month,
    Year,
    /// Everything kept.
    All,
}

impl Period {
    pub const ALL: [Period; 4] = [Period::Week, Period::Month, Period::Year, Period::All];

    pub fn label(self) -> &'static str {
        match self {
            Period::Week => "Week",
            Period::Month => "Month",
            Period::Year => "Year",
            Period::All => "All time",
        }
    }

    /// "in the last 7 days", for sentences.
    pub fn phrase(self) -> &'static str {
        match self {
            Period::Week => "in the last 7 days",
            Period::Month => "in the last 30 days",
            Period::Year => "in the last 12 months",
            Period::All => "since the history starts",
        }
    }

    fn days(self) -> Option<u64> {
        match self {
            Period::Week => Some(7),
            Period::Month => Some(30),
            Period::Year => Some(365),
            Period::All => None,
        }
    }
}

/// An artist, album or track in a top list: one of its tracks (to open, play or
/// show the cover of), how often it was heard and for how long.
#[derive(Clone, Debug, PartialEq)]
pub struct Ranked {
    pub track: Track,
    pub plays: u32,
    pub secs: u64,
}

/// A bar of the listening chart: a day (week, month) or a month (year, all).
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    /// "Mon", "16", "Nov".
    pub label: String,
    /// "Monday 16 Nov", "November 2026", for the tooltip.
    pub name: String,
    pub secs: u64,
}

/// What the plays of a stretch of time add up to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    /// Time listened, skips included.
    pub secs: u64,
    /// Plays heard through (not skipped).
    pub plays: usize,
    pub skipped: usize,
    /// Different artists heard.
    pub artists: usize,
    pub top_artists: Vec<Ranked>,
    pub top_albums: Vec<Ranked>,
    pub top_tracks: Vec<Ranked>,
    /// Artists whose first play kept falls in the stretch, most heard first (none
    /// when it starts with the history).
    pub new_artists: Vec<Ranked>,
    pub new_artist_count: usize,
}

impl Summary {
    pub fn is_empty(&self) -> bool {
        self.plays + self.skipped == 0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub summary: Summary,
    pub chart: Vec<Bar>,
    /// The most heard tracks of the same week a year ago.
    pub year_ago: Vec<Ranked>,
    /// When the oldest play kept happened, if any.
    pub since: Option<u64>,
}

/// Which artist a play counts for: the first credited, else the display name.
pub(super) fn artist_key(track: &Track) -> String {
    match track.artists.first() {
        Some(a) if !a.id.is_empty() => format!("id:{}", a.id),
        _ => format!("name:{}", track.artist.to_lowercase()),
    }
}

fn album_key(track: &Track) -> String {
    if track.album_id.is_empty() { format!("name:{}|{}", track.album.to_lowercase(), artist_key(track)) } else { track.album_id.clone() }
}

/// Counts per key, keeping the latest track for each; ranked by plays heard
/// through, then time.
#[derive(Default)]
struct Tally(HashMap<String, Ranked>);

impl Tally {
    fn add(&mut self, key: String, play: &Play) {
        let counted = u32::from(!play.skipped);
        self.0
            .entry(key)
            .and_modify(|r| {
                r.plays += counted;
                r.secs += play.heard_secs;
                r.track = play.track.clone();
            })
            .or_insert_with(|| Ranked { track: play.track.clone(), plays: counted, secs: play.heard_secs });
    }

    fn top(self, n: usize) -> Vec<Ranked> {
        let mut all: Vec<Ranked> = self.0.into_values().filter(|r| r.plays > 0).collect();
        all.sort_by(|a, b| b.plays.cmp(&a.plays).then(b.secs.cmp(&a.secs)).then_with(|| a.track.title.cmp(&b.track.title)));
        all.truncate(n);
        all
    }
}

/// The stats of `plays` (oldest first, one source's) over `period`, ending at `now`
/// (Unix seconds) in `zone`.
pub fn compute(plays: &[&Play], period: Period, now: u64, zone: &when::Zone) -> Stats {
    let start = period.days().map_or(0, |d| now.saturating_sub(d * DAY));
    let start = start_of_period(period, now, zone).unwrap_or(start);
    let shown: Vec<&Play> = plays.iter().copied().filter(|p| p.at >= start && p.at <= now).collect();

    // The week around this day a year ago.
    let year_ago = {
        let then = now.saturating_sub(YEAR);
        let (from, to) = (then.saturating_sub(YEAR_AGO_BEFORE), then + YEAR_AGO_AFTER);
        let mut tally = Tally::default();
        for play in plays.iter().filter(|p| p.at >= from && p.at < to) {
            tally.add(play.track.id.to_string(), play);
        }
        tally.top(YEAR_AGO_TOP)
    };

    let since = plays.iter().map(|p| p.at).min();
    Stats { summary: summarize(plays, start, now + 1, TOP), chart: chart(&shown, period, now, since, zone), year_ago, since }
}

/// What `plays` (oldest first, one source's) from `start` until before `end` (Unix
/// seconds) add up to, with the `top_tracks` most heard tracks. Artists are new
/// when no play kept before `start` has them; with `start` 0 (everything), none are.
pub fn summarize(plays: &[&Play], start: u64, end: u64, top_tracks: usize) -> Summary {
    let shown: Vec<&Play> = plays.iter().copied().filter(|p| p.at >= start && p.at < end).collect();
    let mut artists = Tally::default();
    let mut albums = Tally::default();
    let mut tracks = Tally::default();
    for play in &shown {
        artists.add(artist_key(&play.track), play);
        if !play.track.album.is_empty() {
            albums.add(album_key(&play.track), play);
        }
        tracks.add(play.track.id.to_string(), play);
    }
    let artist_count = artists.0.len();

    let (new_artists, new_artist_count) = if start == 0 {
        (Vec::new(), 0)
    } else {
        let before: HashSet<String> = plays.iter().filter(|p| p.at < start).map(|p| artist_key(&p.track)).collect();
        let mut fresh = Tally::default();
        for play in &shown {
            let key = artist_key(&play.track);
            if !before.contains(&key) {
                fresh.add(key, play);
            }
        }
        let count = fresh.0.len();
        (fresh.top(TOP), count)
    };

    Summary {
        secs: shown.iter().map(|p| p.heard_secs).sum(),
        plays: shown.iter().filter(|p| !p.skipped).count(),
        skipped: shown.iter().filter(|p| p.skipped).count(),
        artists: artist_count,
        top_artists: artists.top(TOP),
        top_albums: albums.top(TOP),
        top_tracks: tracks.top(top_tracks),
        new_artists,
        new_artist_count,
    }
}

/// Where the period starts, Unix seconds: midnight of its first day, so the chart's
/// first bar is a whole day (or month).
fn start_of_period(period: Period, now: u64, zone: &when::Zone) -> Option<u64> {
    let today = when::day(now, zone)?;
    let first = match period {
        Period::Week => today.checked_sub(6.days()).ok()?,
        Period::Month => today.checked_sub(29.days()).ok()?,
        Period::Year => today.first_of_month().checked_sub(11.months()).ok()?,
        Period::All => return Some(0),
    };
    unix(first, zone)
}

/// Midnight starting `day` in `zone`, Unix seconds.
pub(super) fn unix(day: Date, zone: &when::Zone) -> Option<u64> {
    let start = day.to_zoned(zone.clone()).ok()?;
    u64::try_from(start.timestamp().as_second()).ok()
}

/// Time listened per day (week, month) or month (year, all: from the first month
/// kept, at least a year's worth).
fn chart(shown: &[&Play], period: Period, now: u64, oldest: Option<u64>, zone: &when::Zone) -> Vec<Bar> {
    let Some(today) = when::day(now, zone) else { return Vec::new() };
    let days = |n: i64, label: &dyn Fn(Date) -> String| -> Vec<(Date, Bar)> {
        (0..n)
            .rev()
            .filter_map(|back| today.checked_sub(back.days()).ok())
            .map(|d| (d, Bar { label: label(d), name: d.strftime("%A %-d %b").to_string(), secs: 0 }))
            .collect()
    };
    let by_month = |first: Date| -> Vec<(Date, Bar)> {
        let mut month = first.first_of_month();
        let mut bars = Vec::new();
        while month <= today {
            bars.push((month, Bar { label: month.strftime("%b").to_string(), name: month.strftime("%B %Y").to_string(), secs: 0 }));
            let Ok(next) = month.checked_add(1.month()) else { break };
            month = next;
        }
        bars
    };
    let year_back = today.first_of_month().checked_sub(11.months()).unwrap_or(today);
    let (mut bars, monthly) = match period {
        Period::Week => (days(7, &|d| d.strftime("%a").to_string()), false),
        Period::Month => (days(30, &|d| d.day().to_string()), false),
        Period::Year => (by_month(year_back), true),
        Period::All => {
            let first = oldest.and_then(|at| when::day(at, zone)).map_or(year_back, |d| d.min(year_back));
            (by_month(first), true)
        }
    };
    for play in shown {
        let Some(day) = when::day(play.at, zone) else { continue };
        let key = if monthly { day.first_of_month() } else { day };
        if let Ok(i) = bars.binary_search_by(|(d, _)| d.cmp(&key)) {
            bars[i].1.secs += play.heard_secs;
        }
    }
    bars.into_iter().map(|(_, bar)| bar).collect()
}

/// Lists made from the play history, opened like playlists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Smart {
    /// Most heard in the last [`ON_REPEAT_DAYS`].
    OnRepeat,
    /// First heard in the last [`DISCOVERED_DAYS`] and heard again since.
    Discovered,
    /// Liked, or heard through [`OFTEN`] or more, but not in the last
    /// [`FORGOTTEN_DAYS`].
    Forgotten,
    /// Heard through [`OFTEN`] or more and never skipped.
    NeverSkipped,
}

/// Tracks in each list, at most.
const SMART_MOST: usize = 50;
/// The lists' windows, in days, and how often a track must be heard through.
const ON_REPEAT_DAYS: u64 = 30;
const DISCOVERED_DAYS: u64 = 45;
const FORGOTTEN_DAYS: u64 = 60;
/// Heard through this often: again (on repeat, found lately), often (the rest).
const AGAIN: u32 = 2;
const OFTEN: u32 = 3;

impl Smart {
    pub const ALL: [Smart; 4] = [Smart::OnRepeat, Smart::Discovered, Smart::Forgotten, Smart::NeverSkipped];

    pub fn title(self) -> &'static str {
        match self {
            Smart::OnRepeat => "On repeat",
            Smart::Discovered => "Found lately",
            Smart::Forgotten => "Not heard in a while",
            Smart::NeverSkipped => "Never skipped",
        }
    }

    pub fn about(self) -> String {
        match self {
            Smart::OnRepeat => format!("What you played most in the last {ON_REPEAT_DAYS} days"),
            Smart::Discovered => format!("First heard in the last {DISCOVERED_DAYS} days, and played again since"),
            Smart::Forgotten => format!("Liked or played often, but not in the last {FORGOTTEN_DAYS} days"),
            Smart::NeverSkipped => format!("Played {OFTEN} times or more and never skipped"),
        }
    }
}

/// One track's plays, for the lists.
struct TrackPlays<'a> {
    track: &'a Track,
    first: u64,
    last: u64,
    heard: u32,
    skipped: u32,
}

/// The tracks of list `which` from `plays` (oldest first), most heard first;
/// `liked` says which tracks the account likes.
pub fn smart_list(plays: &[&Play], which: Smart, now: u64, liked: &dyn Fn(&crate::model::TrackId) -> bool) -> Vec<Track> {
    let window = match which {
        Smart::OnRepeat => Some(now.saturating_sub(ON_REPEAT_DAYS * DAY)),
        _ => None,
    };
    let mut by_track: HashMap<&crate::model::TrackId, TrackPlays> = HashMap::new();
    for play in plays.iter().filter(|p| window.is_none_or(|from| p.at >= from)) {
        let entry = by_track.entry(&play.track.id).or_insert(TrackPlays {
            track: &play.track,
            first: play.at,
            last: play.at,
            heard: 0,
            skipped: 0,
        });
        entry.track = &play.track;
        entry.last = entry.last.max(play.at);
        entry.first = entry.first.min(play.at);
        if play.skipped {
            entry.skipped += 1;
        } else {
            entry.heard += 1;
        }
    }
    let keep = |t: &TrackPlays| match which {
        Smart::OnRepeat => t.heard >= AGAIN,
        Smart::Discovered => t.first >= now.saturating_sub(DISCOVERED_DAYS * DAY) && t.heard >= AGAIN,
        Smart::Forgotten => t.last < now.saturating_sub(FORGOTTEN_DAYS * DAY) && (t.heard >= OFTEN || liked(&t.track.id)),
        Smart::NeverSkipped => t.heard >= OFTEN && t.skipped == 0,
    };
    let mut kept: Vec<TrackPlays> = by_track.into_values().filter(|t| keep(t) && !t.track.is_episode()).collect();
    kept.sort_by(|a, b| b.heard.cmp(&a.heard).then(b.last.cmp(&a.last)));
    kept.into_iter().take(SMART_MOST).map(|t| t.track.clone()).collect()
}

/// "12 h 40 min", "35 min", "2 days 4 h".
pub fn duration(secs: u64) -> String {
    let mins = secs / 60;
    let (days, hours, mins) = (mins / (24 * 60), mins / 60 % 24, mins % 60);
    match (days, hours) {
        (0, 0) => format!("{mins} min"),
        (0, h) if mins == 0 => format!("{h} h"),
        (0, h) => format!("{h} h {mins} min"),
        (d, h) => format!("{d} {} {h} h", if d == 1 { "day" } else { "days" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArtistRef, TrackId};

    fn zone() -> when::Zone {
        when::Zone::UTC
    }

    /// 2026-11-16 12:00 UTC.
    const NOW: u64 = 1_794_830_400;

    fn track(id: u64, artist: &str, album: &str) -> Track {
        Track {
            artist: artist.into(),
            artists: vec![ArtistRef { id: format!("a-{artist}"), name: artist.into() }],
            album: album.into(),
            album_id: format!("al-{album}"),
            ..Track::test(id)
        }
    }

    fn play(days_ago: u64, id: u64, artist: &str, album: &str, skipped: bool) -> Play {
        Play {
            source: "deezer".into(),
            at: NOW - days_ago * DAY,
            heard_secs: if skipped { 20 } else { 200 },
            skipped,
            flow: None,
            track: track(id, artist, album),
        }
    }

    fn stats(plays: &[Play], period: Period) -> Stats {
        let refs: Vec<&Play> = plays.iter().collect();
        compute(&refs, period, NOW, &zone())
    }

    #[test]
    fn top_lists_count_plays_heard_through() {
        let plays = [
            play(40, 9, "Old", "Gone", false),
            play(3, 1, "Emma", "Killing", false),
            play(2, 2, "Emma", "Killing", false),
            play(2, 3, "Low", "Double", false),
            play(1, 3, "Low", "Double", true),
            play(1, 4, "Low", "Double", true),
            play(0, 1, "Emma", "Killing", false),
        ];
        let s = stats(&plays, Period::Week);
        assert_eq!(s.summary.plays, 4);
        assert_eq!(s.summary.skipped, 2);
        assert_eq!(s.summary.secs, 4 * 200 + 2 * 20);
        assert_eq!(s.summary.artists, 2);
        let names: Vec<_> = s.summary.top_artists.iter().map(|r| (r.track.artist.as_str(), r.plays)).collect();
        assert_eq!(names, [("Emma", 3), ("Low", 1)]);
        assert_eq!(s.summary.top_tracks[0].track.id, TrackId::from(1));
        assert_eq!(s.summary.top_tracks[0].plays, 2);
        assert!(s.summary.top_tracks.iter().all(|r| r.track.id != TrackId::from(4)), "only skipped: not in the top");
        assert_eq!(s.summary.top_albums[0].track.album, "Killing");
    }

    #[test]
    fn new_artists_are_those_not_heard_before() {
        let plays = [play(40, 9, "Old", "Gone", false), play(3, 1, "Old", "Gone", false), play(2, 2, "Fresh", "New", false)];
        let s = stats(&plays, Period::Week);
        assert_eq!(s.summary.new_artist_count, 1);
        assert_eq!(s.summary.new_artists[0].track.artist, "Fresh");
        assert_eq!(stats(&plays, Period::All).summary.new_artist_count, 0);
    }

    #[test]
    fn the_chart_has_a_bar_per_day_or_month() {
        let plays = [play(0, 1, "A", "X", false), play(0, 2, "A", "X", false), play(6, 3, "A", "X", false), play(8, 4, "A", "X", false)];
        let week = stats(&plays, Period::Week).chart;
        assert_eq!(week.len(), 7);
        assert_eq!(week.last().map(|b| (b.label.as_str(), b.secs)), Some(("Mon", 400)));
        assert_eq!(week[0].secs, 200, "6 days ago is the first bar; 8 days ago is out");
        assert_eq!(stats(&plays, Period::Month).chart.len(), 30);
        let year = stats(&plays, Period::Year).chart;
        assert_eq!(year.len(), 12);
        assert_eq!(year.last().map(|b| (b.label.as_str(), b.secs)), Some(("Nov", 800)));
        assert_eq!(stats(&plays, Period::All).chart.len(), 12, "at least a year");
    }

    #[test]
    fn a_year_ago_is_the_same_week() {
        let plays = [play(366, 7, "Then", "T", false), play(380, 8, "Before", "B", false), play(0, 1, "Now", "N", false)];
        let s = stats(&plays, Period::Week);
        assert_eq!(s.year_ago.iter().map(|r| r.track.id.clone()).collect::<Vec<_>>(), [TrackId::from(7)]);
        assert_eq!(s.since, Some(NOW - 380 * DAY));
    }

    #[test]
    fn lists_from_the_history() {
        let mut plays = vec![
            play(200, 1, "Old", "O", false),
            play(190, 1, "Old", "O", false),
            play(180, 1, "Old", "O", false),
            play(150, 2, "Liked", "L", false),
            play(20, 3, "New", "N", false),
            play(10, 3, "New", "N", false),
            play(5, 4, "Mixed", "M", false),
            play(4, 4, "Mixed", "M", true),
            play(3, 4, "Mixed", "M", false),
            play(2, 4, "Mixed", "M", false),
        ];
        plays.sort_by_key(|p| p.at);
        let refs: Vec<&Play> = plays.iter().collect();
        let liked = |id: &TrackId| *id == TrackId::from(2);
        let ids = |which| smart_list(&refs, which, NOW, &liked).iter().map(|t| t.id.to_string()).collect::<Vec<_>>();
        assert_eq!(ids(Smart::OnRepeat), ["4", "3"]);
        assert_eq!(ids(Smart::Discovered), ["4", "3"]);
        assert_eq!(ids(Smart::Forgotten), ["1", "2"], "played often, or liked; not lately");
        assert_eq!(ids(Smart::NeverSkipped), ["1"]);
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration(35 * 60), "35 min");
        assert_eq!(duration(3 * 3600), "3 h");
        assert_eq!(duration(12 * 3600 + 40 * 60), "12 h 40 min");
        assert_eq!(duration(52 * 3600), "2 days 4 h");
    }
}
