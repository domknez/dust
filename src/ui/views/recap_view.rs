//! A *recap*: one calendar month or year of listening on this source, told as
//! highlights. ‹ › step through months (years); the top tracks play, or become a
//! playlist. Worked out by [`recap`](crate::ui::recap).

use super::stats_view::{Kind, chart, figure_tiles};
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::format::{count, thousands};
use crate::ui::playlist_edit::Dialog;
use crate::ui::plays::when;
use crate::ui::recap::{self, Recap, Span};
use crate::ui::state::View;
use crate::ui::stats;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, Sense, Ui, pos2, vec2};
use std::rc::Rc;

/// The recap last worked out; redone when a play is added, another is opened or a
/// day passes.
#[derive(Default)]
pub struct RecapView {
    cache: Option<(Key, Rc<Recap>)>,
}

#[derive(PartialEq)]
struct Key {
    source: String,
    span: Span,
    /// [`Plays::revision`](crate::ui::plays::Plays::revision).
    plays: u64,
    today: jiff::civil::Date,
}

impl App {
    /// Open the recap to start with: a month's, or a year's.
    pub(in crate::ui) fn open_recap(&mut self, month: bool) {
        let zone = when::zone();
        let span = recap::opening(month, &self.plays.oldest_first(self.data_key()), when::today(&zone), &zone);
        self.open_view(View::Recap(span));
    }

    pub(in crate::ui) fn recap_page(&mut self, ui: &mut Ui, span: Span, acts: &mut Actions) {
        let Some(source) = self.client.as_ref().map(|c| c.data_key().to_string()) else { return };
        let zone = when::zone();
        let today = when::today(&zone);
        let key = Key { source, span, plays: self.plays.revision(), today };
        if self.recap_view.cache.as_ref().is_none_or(|(k, _)| *k != key) {
            let worked_out = recap::recap(&self.plays.oldest_first(&key.source), span, today, &zone);
            self.recap_view.cache = Some((key, Rc::new(worked_out)));
        }
        let Some(r) = self.recap_view.cache.as_ref().map(|(_, r)| r.clone()) else { return };

        egui::ScrollArea::vertical().id_salt("recap").auto_shrink([false, false]).show(ui, |ui| {
            header(ui, &r, today, acts);
            if r.summary.is_empty() {
                ui.add_space(20.0);
                let hint = if r.so_far { "Play something; it adds up here." } else { "Nothing was heard then on this computer." };
                widgets::empty_state(ui, "Nothing heard", hint);
                return;
            }
            ui.add_space(18.0);
            figure_tiles(ui, &totals(&r));
            ui.add_space(12.0);
            figure_tiles(ui, &habits(&r));
            ui.add_space(22.0);
            chart(ui, &r.chart);
            ui.add_space(22.0);
            top_track_buttons(ui, &r, acts);
            ui.add_space(26.0);
            let top_tracks = &r.summary.top_tracks[..r.summary.top_tracks.len().min(stats::TOP)];
            self.top_lists(ui, [&r.summary.top_artists, &r.summary.top_albums, top_tracks], acts);
            if !r.nothing_before && !r.summary.new_artists.is_empty() {
                ui.add_space(26.0);
                let caption = format!("FIRST HEARD · {}", r.summary.new_artist_count);
                self.top_list(ui, &caption, Kind::Artist, &r.summary.new_artists, acts);
            }
            ui.add_space(30.0);
        });
    }
}

/// ‹ title ›, *Month* / *Year*, and the recap in a sentence.
fn header(ui: &mut Ui, r: &Recap, today: jiff::civil::Date, acts: &mut Actions) {
    let p = colors();
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let earlier = (!r.nothing_before).then(|| r.span.previous());
        let later = (!r.so_far).then(|| r.span.next());
        if let Some(to) = step(ui, Icon::ChevronLeft, "Earlier", earlier) {
            acts.push(Action::Open(View::Recap(to)));
        }
        let title = if r.so_far { format!("{} so far", r.span.title()) } else { r.span.title() };
        let width = ui.painter().layout_no_wrap(title.clone(), ty::PAGE_TITLE.font(), p.text).size().x;
        let (rect, _) = ui.allocate_exact_size(vec2(width, 44.0), Sense::hover());
        text_left(ui.painter(), pos2(rect.left(), rect.center().y), &title, ty::PAGE_TITLE, p.text, width);
        if let Some(to) = step(ui, Icon::ChevronRight, "Later", later) {
            acts.push(Action::Open(View::Recap(to)));
        }
        ui.add_space(18.0);
        for (label, month) in [("Month", true), ("Year", false)] {
            if widgets::chip(ui, label, r.span.is_month() == month).clicked() && r.span.is_month() != month {
                acts.push(Action::Open(View::Recap(r.span.other_kind(today))));
            }
        }
    });
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    text_left(ui.painter(), pos2(line.left(), line.center().y), &story(r), ty::LEAD, p.dim, line.width());
}

/// ‹ or ›, dimmed when there's nowhere to go; where to, when clicked.
fn step(ui: &mut Ui, icon: Icon, hint: &str, to: Option<Span>) -> Option<Span> {
    let p = colors();
    let resp = widgets::icon_button(ui, icon, 14.0, if to.is_some() { p.text } else { p.faint });
    to.filter(|_| resp.on_hover_text(hint).clicked())
}

/// "41 h of music across 23 days, 12% more than September. Emma was 28% of it."
fn story(r: &Recap) -> String {
    if r.summary.is_empty() {
        return "From the play history on this computer.".into();
    }
    let mut line = format!("{} of music across {}", stats::duration(r.summary.secs), count(r.days, "day"));
    if let Some((before, change)) = &r.change {
        let more = match change {
            0 => format!(", as much as {before}"),
            c if *c > 0 => format!(", {c}% more than {before}"),
            c => format!(", {}% less than {before}", c.unsigned_abs()),
        };
        line.push_str(&more);
    }
    line.push('.');
    if let (Some(top), Some(share)) = (r.summary.top_artists.first(), r.top_share) {
        let name = top.track.artists.first().map_or(top.track.artist.as_str(), |a| a.name.as_str());
        line.push_str(&format!(" {name} was {share}% of it."));
    }
    line
}

/// Time listened, tracks, artists and the ones new.
fn totals(r: &Recap) -> Vec<(&'static str, String, String)> {
    let s = &r.summary;
    let skipped = if s.skipped > 0 { format!("{} skipped", thousands(s.skipped as u64)) } else { "none skipped".into() };
    let mut figures = vec![
        ("TIME LISTENED", stats::duration(s.secs), String::new()),
        ("TRACKS HEARD", thousands(s.plays as u64), skipped),
        ("ARTISTS", thousands(s.artists as u64), String::new()),
    ];
    if !r.nothing_before {
        figures.push(("NEW ARTISTS", thousands(s.new_artist_count as u64), "first heard".into()));
    }
    figures
}

/// Days listened, the longest run of them, the busiest day and the time of day.
fn habits(r: &Recap) -> Vec<(&'static str, String, String)> {
    let mut figures = vec![
        ("DAYS LISTENED", thousands(r.days as u64), format!("of {}", r.days_in)),
        ("LONGEST RUN", count(r.streak, "day"), "in a row".into()),
    ];
    if let Some((day, secs)) = r.busiest {
        figures.push(("BUSIEST DAY", day.strftime("%a %-d %b").to_string(), stats::duration(secs)));
    }
    if let Some(part) = r.mostly {
        figures.push(("MOSTLY", part.label().to_string(), String::new()));
    }
    figures
}

/// *Play top tracks* and *Save as playlist*, for the tracks that can be played.
fn top_track_buttons(ui: &mut Ui, r: &Recap, acts: &mut Actions) {
    let n = r.summary.top_tracks.iter().filter(|t| t.track.available).count();
    if n == 0 {
        return;
    }
    // Copied only when asked for, not every frame.
    let tracks = || r.summary.top_tracks.iter().map(|t| t.track.clone()).filter(|t| t.available).collect::<Vec<_>>();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if widgets::pill(ui, "Play top tracks", Some(Icon::Play), true).on_hover_text(count(n, "track")).clicked() {
            acts.push(Action::PlayFrom { tracks: tracks(), index: 0, shuffle: false });
        }
        if widgets::pill(ui, "Save as playlist", Some(Icon::Plus), false).clicked() {
            let title = format!("Top tracks of {}", r.span.title());
            acts.push(Action::PlaylistDialog(Dialog::Create { title, private: false, tracks: tracks() }));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArtistRef, Track};
    use crate::ui::recap::TimeOfDay;
    use crate::ui::stats::{Ranked, Summary};
    use jiff::civil::date;

    fn recap_with(secs: u64, change: Option<i64>) -> Recap {
        let track = Track { artists: vec![ArtistRef { id: "1".into(), name: "Emma".into() }], ..Track::test(1) };
        Recap {
            span: Span::Month(date(2026, 10, 1)),
            so_far: false,
            summary: Summary { secs, plays: 3, top_artists: vec![Ranked { track, plays: 3, secs }], ..Default::default() },
            days: 1,
            days_in: 31,
            streak: 1,
            busiest: Some((date(2026, 10, 3), secs)),
            mostly: Some(TimeOfDay::Night),
            top_share: Some(100),
            change: change.map(|c| ("September".into(), c)),
            chart: Vec::new(),
            nothing_before: false,
        }
    }

    #[test]
    fn the_recap_reads_as_a_sentence() {
        assert_eq!(story(&recap_with(3 * 3600, Some(12))), "3 h of music across 1 day, 12% more than September. Emma was 100% of it.");
        assert!(story(&recap_with(3600, Some(-40))).contains(", 40% less than September."));
        assert!(story(&recap_with(3600, Some(0))).contains(", as much as September."));
        assert!(story(&recap_with(3600, None)).starts_with("1 h of music across 1 day. Emma"), "nothing before to compare");
    }

    #[test]
    fn habits_say_how_the_listening_went() {
        let figures = habits(&recap_with(3600, None));
        let captions: Vec<_> = figures.iter().map(|f| f.0).collect();
        assert_eq!(captions, ["DAYS LISTENED", "LONGEST RUN", "BUSIEST DAY", "MOSTLY"]);
        assert_eq!(figures[0].2, "of 31");
        assert_eq!((figures[2].1.as_str(), figures[3].1.as_str()), ("Sat 3 Oct", "Nights"));
    }

    /// `t`'s page drawn twice (the fonts are in from the second frame), with what
    /// it asked for.
    fn draw(t: &mut crate::ui::app::testing::TestApp) -> Vec<Action> {
        let ctx = t.ctx.clone();
        crate::ui::style::install_fonts(&ctx);
        let mut acts = Actions::default();
        for _ in 0..2 {
            acts = Actions::default();
            let View::Recap(span) = t.app.browse.view.clone() else { panic!("not a recap") };
            ctx.run_ui(Default::default(), |ui| t.app.recap_page(ui, span, &mut acts)).textures_delta.clear();
        }
        acts.into_vec()
    }

    #[test]
    fn a_recap_opens_from_the_history_and_draws() {
        use crate::ui::app::testing::{TestApp, deezer};
        let mut t = TestApp::new("");
        t.app.source_ready(deezer());
        let today = when::today(&when::zone());
        for (id, days_ago) in [(1, 0), (2, 1), (1, 40)] {
            let at = crate::clock::unix_now() - days_ago * stats::DAY;
            let play =
                crate::ui::plays::Play { source: "deezer".into(), at, heard_secs: 200, skipped: false, flow: None, track: Track::test(id) };
            t.app.plays.record(play);
        }
        t.update(Action::OpenRecap { month: false });
        let zone = when::zone();
        let expected = recap::opening(false, &t.app.plays.oldest_first("deezer"), today, &zone);
        assert_eq!(t.app.browse.view, View::Recap(expected));
        let asked = draw(&mut t);
        assert!(asked.is_empty(), "drawing asks for nothing: {}", asked.len());
        let (_, r) = t.app.recap_view.cache.as_ref().expect("worked out");
        assert!(!r.summary.is_empty());

        let mut off = TestApp::new("history=false");
        off.app.source_ready(deezer());
        off.update(Action::OpenRecap { month: true });
        assert_eq!(off.app.browse.view, View::Home, "made from the history, which is off");
    }
}
