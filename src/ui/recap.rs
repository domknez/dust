//! Recaps: a calendar month or year of listening told as highlights, worked out
//! from the play history ([`Plays`](crate::ui::plays)). The figures and top lists
//! are the Listening page's ([`stats::summarize`]); on top come the days listened,
//! the longest run of them, the busiest day, the time of day most is heard, the
//! top artist's share and the change from the span before.

use crate::ui::plays::{Play, when};
use crate::ui::stats::{self, Bar, Summary};
use jiff::ToSpan;
use jiff::civil::Date;
use std::collections::{BTreeMap, HashMap};

/// Top tracks kept: the ten shown, and more for playing or saving as a playlist.
pub const TOP_TRACKS: usize = 50;

/// A calendar month (by its first day) or year.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Span {
    Month(Date),
    Year(i16),
}

impl Span {
    /// The month `day` falls in.
    pub fn month_of(day: Date) -> Self {
        Span::Month(day.first_of_month())
    }

    pub fn year_of(day: Date) -> Self {
        Span::Year(day.year())
    }

    pub fn first_day(self) -> Date {
        match self {
            Span::Month(first) => first,
            Span::Year(year) => Date::new(year, 1, 1).unwrap_or(Date::MIN),
        }
    }

    /// The day after the span.
    fn end_day(self) -> Date {
        let first = self.first_day();
        let next = match self {
            Span::Month(_) => first.checked_add(1.month()),
            Span::Year(_) => first.checked_add(1.year()),
        };
        next.unwrap_or(Date::MAX)
    }

    pub fn previous(self) -> Self {
        match self {
            Span::Month(first) => Span::Month(first.checked_sub(1.month()).unwrap_or(first)),
            Span::Year(year) => Span::Year(year - 1),
        }
    }

    pub fn next(self) -> Self {
        match self {
            Span::Month(first) => Span::Month(first.checked_add(1.month()).unwrap_or(first)),
            Span::Year(year) => Span::Year(year + 1),
        }
    }

    pub fn contains(self, day: Date) -> bool {
        self.first_day() <= day && day < self.end_day()
    }

    pub fn is_month(self) -> bool {
        matches!(self, Span::Month(_))
    }

    /// A month's year, or a year's latest month up to `today`.
    pub fn other_kind(self, today: Date) -> Self {
        match self {
            Span::Month(first) => Span::Year(first.year()),
            Span::Year(_) => Span::month_of(self.end_day().yesterday().unwrap_or(today).min(today)),
        }
    }

    /// "October 2026", "2026".
    pub fn title(self) -> String {
        match self {
            Span::Month(first) => first.strftime("%B %Y").to_string(),
            Span::Year(year) => year.to_string(),
        }
    }

    /// "September", "2025": the span before, as the change is told against it.
    fn name(self) -> String {
        match self {
            Span::Month(first) => first.strftime("%B").to_string(),
            Span::Year(year) => year.to_string(),
        }
    }

    /// Unix seconds from its first midnight until before the one after it.
    fn bounds(self, zone: &when::Zone) -> (u64, u64) {
        let at = |day| stats::unix(day, zone).unwrap_or(0);
        (at(self.first_day()), at(self.end_day()))
    }
}

/// The recap to open first: last month's (last year's in January) when anything
/// was heard then, else the one under way.
pub fn opening(month: bool, plays: &[&Play], today: Date, zone: &when::Zone) -> Span {
    let now = if month { Span::month_of(today) } else { Span::year_of(today) };
    let before = now.previous();
    let (start, end) = before.bounds(zone);
    let looking_back = month || today.month() == 1;
    if looking_back && plays.iter().any(|p| p.at >= start && p.at < end) { before } else { now }
}

/// Part of the day, by the hour the listening started.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TimeOfDay {
    Morning,
    Afternoon,
    Evening,
    Night,
}

impl TimeOfDay {
    fn at(hour: i8) -> Self {
        match hour {
            5..=11 => TimeOfDay::Morning,
            12..=16 => TimeOfDay::Afternoon,
            17..=21 => TimeOfDay::Evening,
            _ => TimeOfDay::Night,
        }
    }

    /// "Evenings", for "mostly: evenings".
    pub fn label(self) -> &'static str {
        match self {
            TimeOfDay::Morning => "Mornings",
            TimeOfDay::Afternoon => "Afternoons",
            TimeOfDay::Evening => "Evenings",
            TimeOfDay::Night => "Nights",
        }
    }
}

/// A recap of one span.
#[derive(Clone, Debug, PartialEq)]
pub struct Recap {
    pub span: Span,
    /// The span hasn't ended yet: a recap so far.
    pub so_far: bool,
    pub summary: Summary,
    /// Days with any listening.
    pub days: usize,
    /// Days of the span gone by (all of them once it's over).
    pub days_in: usize,
    /// The most days in a row with listening.
    pub streak: usize,
    /// The day with the most listening, and how long.
    pub busiest: Option<(Date, u64)>,
    /// When most of the listening happened.
    pub mostly: Option<TimeOfDay>,
    /// The top artist's share of the time listened, in percent.
    pub top_share: Option<u32>,
    /// Time listened against the span before: (what of it, "September" or "the
    /// first 24 days of October" while this one is under way; percent more or
    /// less). None when nothing was heard then, or the history doesn't reach back
    /// to its start.
    pub change: Option<(String, i64)>,
    /// Listening per day (a month) or month (a year).
    pub chart: Vec<Bar>,
    /// Nothing was heard before the span (or ever): there's no recap to go back
    /// to, and "first heard" means little.
    pub nothing_before: bool,
}

/// The recap of `span` from `plays` (one source's, oldest first), as of `today` in
/// `zone`.
pub fn recap(plays: &[&Play], span: Span, today: Date, zone: &when::Zone) -> Recap {
    let (start, end) = span.bounds(zone);
    let shown: Vec<&Play> = plays.iter().copied().filter(|p| p.at >= start && p.at < end).collect();
    let summary = stats::summarize(plays, start, end, TOP_TRACKS);

    let mut per_day: BTreeMap<Date, u64> = BTreeMap::new();
    let mut per_part: HashMap<TimeOfDay, u64> = HashMap::new();
    for play in &shown {
        if let Some(day) = when::day(play.at, zone) {
            *per_day.entry(day).or_default() += play.heard_secs;
        }
        if let Some(hour) = when::hour(play.at, zone) {
            *per_part.entry(TimeOfDay::at(hour)).or_default() += play.heard_secs;
        }
    }
    let busiest =
        per_day.iter().filter(|(_, secs)| **secs > 0).max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(day, secs)| (*day, *secs));
    let mostly = per_part.into_iter().filter(|(_, secs)| *secs > 0).max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|(part, _)| part);
    let top_share =
        summary.top_artists.first().filter(|_| summary.secs > 0).map(|top| u32::try_from(top.secs * 100 / summary.secs).unwrap_or(100));

    let so_far = span.contains(today);
    let days_in = usize::try_from((span.end_day().min(today.tomorrow().unwrap_or(today)) - span.first_day()).get_days()).unwrap_or(0);
    let change = change(plays, span, so_far.then_some(days_in), zone).map(|(name, then)| (name, percent_change(then, summary.secs)));

    Recap {
        span,
        so_far,
        days: per_day.values().filter(|secs| **secs > 0).count(),
        days_in,
        streak: longest_run(per_day.iter().filter(|(_, secs)| **secs > 0).map(|(day, _)| *day)),
        busiest,
        mostly,
        top_share,
        change,
        chart: chart(span, &per_day, today),
        nothing_before: plays.first().is_none_or(|p| p.at >= start),
        summary,
    }
}

/// What to hold `span` against and the time listened then: the span before, or
/// while `span` is under way its first `days_in` days (as many as `span` has had).
/// None when nothing was heard then, or the history starts after it began, which
/// would make any span look like more.
fn change(plays: &[&Play], span: Span, days_in: Option<usize>, zone: &when::Zone) -> Option<(String, u64)> {
    let before = span.previous();
    let (start, full_end) = before.bounds(zone);
    if plays.first().is_none_or(|p| p.at >= start) {
        return None;
    }
    let (end, name) = match days_in {
        Some(n) => {
            let cut = before.first_day().checked_add((n as i64).days()).ok()?.min(before.end_day());
            let last = cut.yesterday().ok()?;
            let name = match before {
                Span::Month(_) => format!("the first {n} days of {}", before.name()),
                Span::Year(_) => format!("{} up to {}", before.name(), last.strftime("%-d %b")),
            };
            (stats::unix(cut, zone)?, name)
        }
        None => (full_end, before.name()),
    };
    let secs: u64 = plays.iter().filter(|p| p.at >= start && p.at < end).map(|p| p.heard_secs).sum();
    (secs > 0).then_some((name, secs))
}

/// From `then` to `now`, in whole percent (rounded towards zero).
fn percent_change(then: u64, now: u64) -> i64 {
    let (then, now) = (i128::from(then), i128::from(now));
    i64::try_from((now - then) * 100 / then).unwrap_or(i64::MAX)
}

/// The most days in a row among `days` (ascending).
fn longest_run(days: impl Iterator<Item = Date>) -> usize {
    let (mut best, mut run, mut last) = (0, 0, None::<Date>);
    for day in days {
        run = if last.and_then(|d| d.tomorrow().ok()) == Some(day) { run + 1 } else { 1 };
        best = best.max(run);
        last = Some(day);
    }
    best
}

/// A bar per day of a month, or per month of a year; none past `today`.
fn chart(span: Span, per_day: &BTreeMap<Date, u64>, today: Date) -> Vec<Bar> {
    let mut bars = Vec::new();
    let mut at = span.first_day();
    while span.contains(at) && at <= today {
        let bar = match span {
            Span::Month(_) => Bar {
                label: at.day().to_string(),
                name: at.strftime("%A %-d %b").to_string(),
                secs: per_day.get(&at).copied().unwrap_or(0),
            },
            Span::Year(_) => {
                let month = Span::month_of(at);
                Bar {
                    label: at.strftime("%b").to_string(),
                    name: at.strftime("%B %Y").to_string(),
                    secs: per_day.range(month.first_day()..month.end_day()).map(|(_, secs)| secs).sum(),
                }
            }
        };
        bars.push(bar);
        let next = match span {
            Span::Month(_) => at.tomorrow(),
            Span::Year(_) => at.checked_add(1.month()),
        };
        let Ok(next) = next else { break };
        at = next;
    }
    bars
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArtistRef, Track};
    use jiff::civil::date;

    fn zone() -> when::Zone {
        when::Zone::UTC
    }

    /// A play on `day` at `hour` UTC.
    fn play(day: Date, hour: i8, id: u64, artist: &str, secs: u64) -> Play {
        let at = day.at(hour, 0, 0, 0).to_zoned(zone()).unwrap().timestamp().as_second() as u64;
        let track = Track {
            artist: artist.into(),
            artists: vec![ArtistRef { id: artist.into(), name: artist.into() }],
            album: format!("{artist} album"),
            album_id: format!("al-{artist}"),
            ..Track::test(id)
        };
        Play { source: "deezer".into(), at, heard_secs: secs, skipped: false, flow: None, track }
    }

    fn recap_of(plays: &[Play], span: Span, today: Date) -> Recap {
        let refs: Vec<&Play> = plays.iter().collect();
        recap(&refs, span, today, &zone())
    }

    #[test]
    fn spans_step_through_months_and_years() {
        let oct = Span::month_of(date(2026, 10, 17));
        assert_eq!(oct, Span::Month(date(2026, 10, 1)));
        assert_eq!(oct.title(), "October 2026");
        assert_eq!(oct.previous().title(), "September 2026");
        assert_eq!(Span::month_of(date(2026, 1, 9)).previous().title(), "December 2025");
        assert_eq!(Span::month_of(date(2026, 12, 9)).next().title(), "January 2027");
        assert!(oct.contains(date(2026, 10, 31)) && !oct.contains(date(2026, 11, 1)));
        let year = Span::year_of(date(2026, 3, 1));
        assert_eq!((year.title(), year.previous().title()), ("2026".into(), "2025".into()));
        assert!(year.contains(date(2026, 12, 31)) && !year.contains(date(2027, 1, 1)));
    }

    #[test]
    fn a_month_told_as_highlights() {
        let plays = [
            play(date(2026, 8, 31), 10, 9, "Earlier", 0),
            play(date(2026, 9, 20), 10, 9, "Earlier", 600),
            play(date(2026, 10, 1), 8, 1, "Emma", 200),
            play(date(2026, 10, 2), 21, 1, "Emma", 200),
            play(date(2026, 10, 3), 22, 2, "Low", 200),
            play(date(2026, 10, 3), 23, 3, "Low", 100),
            play(date(2026, 10, 10), 20, 1, "Emma", 200),
            play(date(2026, 11, 1), 9, 4, "Later", 999),
        ];
        let r = recap_of(&plays, Span::Month(date(2026, 10, 1)), date(2026, 11, 20));
        assert!(!r.so_far);
        assert_eq!((r.summary.plays, r.summary.secs), (5, 900), "October's only");
        assert_eq!(r.summary.top_artists[0].track.artist, "Emma");
        assert_eq!(r.top_share, Some(66), "600 of 900 seconds");
        assert_eq!((r.days, r.streak), (4, 3), "1–3 October in a row, then the 10th");
        assert_eq!(r.busiest, Some((date(2026, 10, 3), 300)));
        assert_eq!(r.mostly, Some(TimeOfDay::Evening), "21:00 and 20:00 against 22:00 and 23:00 at night: 400 to 300");
        assert_eq!(r.change, Some(("September".into(), 50)), "900 against 600 seconds");
        let late = recap_of(&plays[1..], Span::Month(date(2026, 10, 1)), date(2026, 11, 20));
        assert_eq!(late.change, None, "the history starts in mid-September: no telling how much was heard then");
        assert_eq!(r.chart.len(), 31);
        assert_eq!((r.chart[2].label.as_str(), r.chart[2].secs), ("3", 300));
        assert_eq!(r.summary.new_artist_count, 2, "Emma and Low weren't heard before");
        assert!(!r.nothing_before);
        assert_eq!(r.days_in, 31);
    }

    #[test]
    fn this_month_so_far() {
        let plays = [play(date(2026, 11, 2), 12, 1, "A", 100), play(date(2026, 11, 5), 12, 1, "A", 100)];
        let r = recap_of(&plays, Span::Month(date(2026, 11, 1)), date(2026, 11, 20));
        assert!(r.so_far && r.nothing_before);
        assert_eq!(r.days_in, 20, "up to today");
        assert_eq!(r.chart.len(), 20, "no days to come");
        assert_eq!(r.change, None, "nothing in October to compare with");
        assert_eq!(r.mostly, Some(TimeOfDay::Afternoon));
    }

    #[test]
    fn a_year_has_a_bar_per_month() {
        let plays = [
            play(date(2024, 12, 31), 23, 1, "A", 0),
            play(date(2025, 6, 10), 12, 1, "A", 1000),
            play(date(2025, 6, 20), 12, 1, "A", 1),
            play(date(2025, 12, 31), 23, 1, "A", 100),
            play(date(2026, 2, 3), 12, 2, "B", 300),
            play(date(2026, 2, 4), 12, 2, "B", 50),
        ];
        let r = recap_of(&plays, Span::Year(2026), date(2026, 6, 15));
        assert_eq!(r.chart.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(), ["Jan", "Feb", "Mar", "Apr", "May", "Jun"]);
        assert_eq!(r.chart[1].secs, 350);
        assert_eq!(r.summary.plays, 2, "the last night of 2025 isn't 2026's");
        assert_eq!(r.change, Some(("2025 up to 15 Jun".into(), -65)), "350 against 1000 seconds in the first half of 2025");
        let done = recap_of(&plays, Span::Year(2026), date(2027, 3, 1));
        assert_eq!(done.change, Some(("2025".into(), -68)), "350 against all of 2025: 1101");
    }

    #[test]
    fn an_empty_span_is_empty() {
        let r = recap_of(&[], Span::Month(date(2026, 10, 1)), date(2026, 11, 1));
        assert!(r.summary.is_empty() && r.nothing_before);
        assert_eq!((r.days, r.streak, r.busiest, r.mostly, r.top_share), (0, 0, None, None, None));
    }

    #[test]
    fn the_first_recap_looks_back_when_there_is_something_to_see() {
        let in_october = [play(date(2026, 10, 5), 12, 1, "A", 100)];
        let refs: Vec<&Play> = in_october.iter().collect();
        let open = |month, today| opening(month, &refs, today, &zone());
        assert_eq!(open(true, date(2026, 11, 20)), Span::Month(date(2026, 10, 1)), "last month's");
        assert_eq!(open(true, date(2026, 12, 20)), Span::Month(date(2026, 12, 1)), "nothing in November: this month so far");
        assert_eq!(open(false, date(2026, 11, 20)), Span::Year(2026), "this year so far");
        assert_eq!(open(false, date(2027, 1, 10)), Span::Year(2026), "in January, last year's");
        assert_eq!(Span::Year(2026).other_kind(date(2026, 11, 20)), Span::Month(date(2026, 11, 1)), "its month under way");
        assert_eq!(Span::Year(2025).other_kind(date(2026, 11, 20)), Span::Month(date(2025, 12, 1)), "its last month");
        assert_eq!(Span::Month(date(2025, 3, 1)).other_kind(date(2026, 11, 20)), Span::Year(2025));
    }

    #[test]
    fn changes_and_runs_count_right() {
        assert_eq!(percent_change(100, 50), -50);
        assert_eq!(percent_change(3, 4), 33);
        let days = [date(2026, 1, 30), date(2026, 1, 31), date(2026, 2, 1), date(2026, 2, 3)];
        assert_eq!(longest_run(days.into_iter()), 3, "across the end of a month");
        assert_eq!(longest_run(std::iter::empty()), 0);
    }
}
