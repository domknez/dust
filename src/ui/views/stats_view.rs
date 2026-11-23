//! The *Listening* page: what the play history adds up to on this source, over the
//! last week, month, year or everything kept. Time listened, plays and artists, a
//! chart of listening by day or month, the top artists, albums and tracks, the
//! artists heard for the first time, and the same week a year ago.

use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::crate_store::Thing;
use crate::ui::format::thousands;
use crate::ui::plays::when;
use crate::ui::state::{Coll, CollSource};
use crate::ui::stats::{self, Bar, Period, Ranked, Smart, Stats};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, MenuTarget, Playlists, TrackMenu, text_left};
use eframe::egui::{self, Align2, CornerRadius, Rect, RichText, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

const ROW: f32 = 48.0;
const CHART: f32 = 120.0;
/// Columns side by side from this width; one under the other below it.
const THREE_COLUMNS: f32 = 900.0;

/// The page's period, and the stats last worked out (they're redone when a play is
/// added, the period changes or a day passes).
#[derive(Default)]
pub struct StatsView {
    pub period: Period,
    cache: Option<(Key, std::rc::Rc<Stats>)>,
    /// How many tracks each list made from the history holds.
    lists: Vec<(Smart, usize)>,
}

#[derive(PartialEq)]
struct Key {
    source: String,
    period: Period,
    /// [`Plays::revision`](crate::ui::plays::Plays::revision).
    plays: u64,
    /// The likes' revision: a list counts liked tracks.
    likes: u64,
    today: jiff::civil::Date,
}

/// What a top list holds.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Artist,
    Album,
    Track,
}

impl App {
    pub(in crate::ui) fn stats_page(&mut self, ui: &mut Ui, acts: &mut Actions) {
        let p = colors();
        let Some(source) = self.client.as_ref().map(|c| c.data_key().to_string()) else { return };
        let zone = when::zone();
        let key = Key {
            source,
            period: self.stats_view.period,
            plays: self.plays.revision(),
            likes: self.library.likes.revision,
            today: when::today(&zone),
        };
        if self.stats_view.cache.as_ref().is_none_or(|(k, _)| *k != key) {
            let plays = self.plays.oldest_first(&key.source);
            let now = crate::clock::unix_now();
            let worked_out = stats::compute(&plays, key.period, now, &zone);
            let likes = &self.library.likes.likes;
            let liked = |id: &crate::model::TrackId| likes.tracks.contains(id);
            self.stats_view.lists = Smart::ALL.iter().map(|&which| (which, stats::smart_list(&plays, which, now, &liked).len())).collect();
            self.stats_view.cache = Some((key, std::rc::Rc::new(worked_out)));
        }
        let Some(st) = self.stats_view.cache.as_ref().map(|(_, st)| st.clone()) else { return };

        egui::ScrollArea::vertical().id_salt("stats").auto_shrink([false, false]).show(ui, |ui| {
            let head = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover()).0;
            text_left(ui.painter(), head.min + vec2(0.0, 18.0), "Listening", ty::PAGE_TITLE, p.text, head.width());
            let line = match st.since.and_then(|at| when::day(at, &zone)) {
                Some(day) => format!("From the play history on this computer, which starts on {}.", day.strftime("%-d %B %Y")),
                None => "From the play history on this computer.".to_string(),
            };
            text_left(ui.painter(), head.min + vec2(0.0, 50.0), &line, ty::LEAD, p.dim, head.width());
            ui.add_space(14.0);
            self.period_chips(ui);
            ui.add_space(18.0);
            if st.plays + st.skipped == 0 {
                ui.add_space(20.0);
                let hint = format!("Play something; what you hear {} adds up here.", self.stats_view.period.phrase());
                widgets::empty_state(ui, "Nothing heard yet", &hint);
                return;
            }
            tiles(ui, &st, self.stats_view.period);
            ui.add_space(22.0);
            chart(ui, &st.chart);
            ui.add_space(26.0);
            lists(ui, &self.stats_view.lists, acts);
            ui.add_space(26.0);

            let lists = [
                ("TOP ARTISTS", Kind::Artist, &st.top_artists),
                ("TOP ALBUMS", Kind::Album, &st.top_albums),
                ("TOP TRACKS", Kind::Track, &st.top_tracks),
            ];
            if ui.available_width() >= THREE_COLUMNS {
                let gap = 28.0;
                let width = (ui.available_width() - 2.0 * gap) / 3.0;
                let top = ui.cursor().min;
                let mut bottom = top.y;
                for (i, (caption, kind, list)) in lists.into_iter().enumerate() {
                    let rect = Rect::from_min_size(pos2(top.x + i as f32 * (width + gap), top.y), vec2(width, 0.0));
                    let used = ui.scope_builder(UiBuilder::new().max_rect(rect.with_max_y(top.y + 10_000.0)), |ui| {
                        ui.set_width(width);
                        self.top_list(ui, caption, kind, list, acts);
                    });
                    bottom = bottom.max(used.response.rect.bottom());
                }
                ui.allocate_space(vec2(ui.available_width(), bottom - top.y));
            } else {
                for (caption, kind, list) in lists {
                    self.top_list(ui, caption, kind, list, acts);
                    ui.add_space(18.0);
                }
            }
            if !st.new_artists.is_empty() {
                ui.add_space(26.0);
                let caption = format!("FIRST HEARD {} · {}", self.stats_view.period.phrase().to_uppercase(), st.new_artist_count);
                self.top_list(ui, &caption, Kind::Artist, &st.new_artists, acts);
            }
            if !st.year_ago.is_empty() {
                ui.add_space(26.0);
                self.top_list(ui, "THIS WEEK A YEAR AGO", Kind::Track, &st.year_ago, acts);
            }
            ui.add_space(30.0);
        });
    }

    fn period_chips(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for period in Period::ALL {
                if widgets::chip(ui, period.label(), self.stats_view.period == period).clicked() {
                    self.stats_view.period = period;
                }
            }
        });
    }

    /// A caption and up to ten rows: rank, cover, name, and how often it was heard.
    fn top_list(&mut self, ui: &mut Ui, caption: &str, kind: Kind, list: &[Ranked], acts: &mut Actions) {
        let p = colors();
        widgets::caption(ui, caption);
        ui.add_space(4.0);
        if list.is_empty() {
            ui.label(RichText::new("Nothing heard through yet").color(p.faint).font(ty::SMALL.font()));
            return;
        }
        let playlists = Playlists { all: &self.library.playlists, recent: &self.prefs.recent_playlists };
        let mix = self.features().track_mix;
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, ranked) in list.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
            let track = &ranked.track;
            if resp.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
            }
            let rank = pos2(rect.left() + 14.0, rect.center().y);
            ui.painter().text(rank, Align2::CENTER_CENTER, (i + 1).to_string(), ty::META.font(), p.faint);
            let art =
                Rect::from_min_size(pos2(rect.left() + 30.0, rect.center().y - metrics::LIST_ART / 2.0), Vec2::splat(metrics::LIST_ART));
            let corner = if kind == Kind::Artist { (metrics::LIST_ART / 2.0) as u8 } else { radius::THUMB };
            widgets::cover(ui, &mut self.covers, track.art.as_ref(), metrics::THUMB_PX, art, corner);
            let count = plays(ranked.plays);
            let count_width = ui.painter().layout_no_wrap(count.clone(), ty::META.font(), p.dim).size().x;
            ui.painter().text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, count, ty::META.font(), p.dim);
            let x = art.right() + 12.0;
            let width = rect.right() - x - count_width - 18.0;
            let (name, sub) = match kind {
                Kind::Artist => (track.artists.first().map_or(track.artist.as_str(), |a| a.name.as_str()), stats::duration(ranked.secs)),
                Kind::Album => (track.album.as_str(), track.artist.clone()),
                Kind::Track => (track.title.as_str(), track.artist.clone()),
            };
            let title = widgets::TextLine { text: name, style: ty::ITEM.strong(), color: p.text };
            widgets::two_lines(
                ui.painter(),
                x,
                rect.center().y,
                width,
                title,
                widgets::TextLine { text: &sub, style: ty::SECONDARY, color: p.dim },
            );
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            match kind {
                Kind::Artist => {
                    if let Some(artist) = track.artists.first().filter(|a| !a.id.is_empty())
                        && resp.on_hover_text("Go to artist").clicked()
                    {
                        acts.push(Action::OpenArtist(artist.clone()));
                    }
                }
                Kind::Album => {
                    if let Some(album) = Coll::origin_of(track)
                        && resp.on_hover_text("Go to album").clicked()
                    {
                        acts.push(Action::OpenCollection(album));
                    }
                }
                Kind::Track => {
                    let target = MenuTarget { count: 1, playable: track.available };
                    match widgets::track_menu(&resp, playlists, false, mix && !track.is_episode(), target) {
                        Some(TrackMenu::Crate) => acts.push(Action::SaveToCrate(Thing::Track(track.clone()), "Listening".into())),
                        Some(choice) => acts.push(Action::TrackMenu(vec![track.clone()], choice)),
                        None => {}
                    }
                    if resp.clicked() && track.available {
                        acts.push(Action::PlayNow(track.clone()));
                    }
                }
            }
        }
        ui.spacing_mut().item_spacing.y = 6.0;
    }
}

/// Where a list card's description starts, and the room under it for the count.
const ABOUT_TOP: f32 = 38.0;
const COUNT_SPACE: f32 = 36.0;

/// The lists made from the history, as cards: name, what's in it, how many tracks.
/// Click opens one; right-click plays or queues it.
fn lists(ui: &mut Ui, lists: &[(Smart, usize)], acts: &mut Actions) {
    let p = colors();
    widgets::caption(ui, "MADE FROM YOUR HISTORY");
    ui.add_space(6.0);
    let gap = 12.0;
    let count = lists.len().max(1) as f32;
    let width = ((ui.available_width() - gap * (count - 1.0)) / count).max(160.0);
    let inner = width - 32.0;
    // Each card's description, wrapped to the card; all cards as tall as the
    // longest needs, so the count below never runs into it.
    let abouts: Vec<_> = lists
        .iter()
        .map(|(which, _)| {
            let job = egui::text::LayoutJob::simple(which.about(), ty::CAPTION.font(), p.dim, inner);
            ui.painter().layout_job(job)
        })
        .collect();
    let about_height = abouts.iter().map(|g| g.size().y).fold(0.0, f32::max);
    let height = (ABOUT_TOP + about_height + COUNT_SPACE).max(96.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(gap, gap);
        for (&(which, n), galley) in lists.iter().zip(abouts) {
            let (rect, resp) = ui.allocate_exact_size(vec2(width, height), if n > 0 { Sense::click() } else { Sense::hover() });
            let corner = CornerRadius::same(radius::CONTROL);
            ui.painter().rect_filled(rect, corner, if n > 0 && resp.hovered() { p.raised } else { p.surface });
            let edge = if n > 0 && resp.hovered() { p.accent } else { p.line };
            ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.0, edge), egui::StrokeKind::Inside);
            let x = rect.left() + 16.0;
            text_left(
                ui.painter(),
                pos2(x, rect.top() + 24.0),
                which.title(),
                ty::ITEM.strong(),
                if n > 0 { p.text } else { p.dim },
                inner,
            );
            ui.painter().galley(pos2(x, rect.top() + ABOUT_TOP), galley, p.dim);
            let tracks = if n == 0 { "nothing yet".to_string() } else { crate::ui::format::count(n, "track") };
            ui.painter().text(pos2(x, rect.bottom() - 14.0), Align2::LEFT_CENTER, tracks, ty::META.font(), p.faint);
            if n == 0 {
                continue;
            }
            if let Some(mode) = widgets::queue_menu(&resp) {
                acts.push(Action::PlaySource(CollSource::History(which), mode));
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                acts.push(Action::OpenCollection(Coll::history(which)));
            }
        }
    });
}

/// Time listened, tracks heard, artists, and new artists, as figures in a row.
fn tiles(ui: &mut Ui, st: &Stats, period: Period) {
    let p = colors();
    let skipped = if st.skipped > 0 { format!("{} skipped", thousands(st.skipped as u64)) } else { "none skipped".into() };
    let mut figures = vec![
        ("TIME LISTENED", stats::duration(st.secs), String::new()),
        ("TRACKS HEARD", thousands(st.plays as u64), skipped),
        ("ARTISTS", thousands(st.artists as u64), String::new()),
    ];
    if period != Period::All {
        figures.push(("NEW ARTISTS", thousands(st.new_artist_count as u64), "first heard".into()));
    }
    let gap = 12.0;
    let width = ((ui.available_width() - gap * (figures.len() - 1) as f32) / figures.len() as f32).min(240.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (caption, value, sub) in figures {
            let (rect, _) = ui.allocate_exact_size(vec2(width, 84.0), Sense::hover());
            let corner = CornerRadius::same(radius::CONTROL);
            ui.painter().rect_filled(rect, corner, p.surface);
            ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
            let x = rect.left() + 16.0;
            let inner = rect.width() - 32.0;
            text_left(ui.painter(), pos2(x, rect.top() + 20.0), caption, ty::OVERLINE, p.faint, inner);
            text_left(ui.painter(), pos2(x, rect.top() + 46.0), &value, ty::SECTION_TITLE, p.text, inner);
            if !sub.is_empty() {
                text_left(ui.painter(), pos2(x, rect.top() + 68.0), &sub, ty::CAPTION, p.dim, inner);
            }
        }
    });
}

/// Listening by day or month as bars on a baseline; hovering one says when and how
/// long.
fn chart(ui: &mut Ui, bars: &[Bar]) {
    let p = colors();
    widgets::caption(ui, "LISTENING");
    ui.add_space(6.0);
    let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), CHART + 22.0), Sense::hover());
    if bars.is_empty() {
        return;
    }
    let plot = Rect::from_min_max(area.min, pos2(area.right(), area.top() + CHART));
    let most = bars.iter().map(|b| b.secs).max().unwrap_or(0).max(1) as f32;
    let slot = plot.width() / bars.len() as f32;
    let width = (slot - 2.0).clamp(2.0, 28.0);
    // Labels where they fit: every bar, else every few.
    let widest = bars.iter().map(|b| ui.painter().layout_no_wrap(b.label.clone(), ty::META.font(), p.faint).size().x).fold(0.0, f32::max);
    let every = ((widest + 8.0) / slot).ceil().max(1.0) as usize;
    ui.painter().hline(plot.x_range(), plot.bottom(), egui::Stroke::new(1.0, p.line));
    for (i, bar) in bars.iter().enumerate() {
        let center = plot.left() + slot * (i as f32 + 0.5);
        let height = if bar.secs == 0 { 0.0 } else { (bar.secs as f32 / most * (CHART - 6.0)).max(3.0) };
        let hit = Rect::from_min_max(pos2(center - slot / 2.0, plot.top()), pos2(center + slot / 2.0, plot.bottom()));
        let resp = ui.interact(hit, ui.scope_id().with(("stats-bar", i)), Sense::hover());
        let fill = if resp.hovered() { p.accent } else { p.accent_fill.gamma_multiply(0.85) };
        if height > 0.0 {
            let rect = Rect::from_min_max(pos2(center - width / 2.0, plot.bottom() - height), pos2(center + width / 2.0, plot.bottom()));
            let round = (width / 2.0).min(4.0) as u8;
            let corner = CornerRadius { nw: round, ne: round, sw: 0, se: 0 };
            ui.painter().rect_filled(rect, corner, fill);
        }
        let last = i + 1 == bars.len();
        if (bars.len() - 1 - i).is_multiple_of(every) {
            let color = if last || resp.hovered() { p.text } else { p.faint };
            ui.painter().text(pos2(center, plot.bottom() + 12.0), Align2::CENTER_CENTER, &bar.label, ty::META.font(), color);
        }
        let time = if bar.secs == 0 { "nothing".to_string() } else { stats::duration(bar.secs) };
        resp.on_hover_text(format!("{} · {time}", bar.name));
    }
}

/// "1 play", "12,345 plays".
fn plays(n: u32) -> String {
    crate::ui::format::count(n as usize, "play")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_read_naturally() {
        assert_eq!((plays(1), plays(12_345)), ("1 play".into(), "12,345 plays".into()));
    }
}
