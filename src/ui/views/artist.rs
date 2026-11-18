//! Artist page: a header band with the name and picture, Play / Shuffle / Follow,
//! then popular tracks beside the discography (albums, singles & EPs, live &
//! compilations, as tabs), then playlists featuring the artist and related artists.

use super::collection::header_band;
use super::home::card_row;
use crate::model::{Item, Likeable, ReleaseGroup, Section, TrackId};
use crate::player::{State, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::covers::Covers;
use crate::ui::crate_store::Thing;
use crate::ui::format::thousands;
use crate::ui::state::{ArtistView, Coll, CollSource, PlayMode};
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, Columns, Grid, GridColumns, Playlists, RowMenu, Table, text_left};
use eframe::egui::{self, Align2, CornerRadius, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

/// Popular tracks shown before "Show all".
const POPULAR: usize = 10;
const HEADER: f32 = 270.0;
const BUTTONS: f32 = 72.0;
/// Popular tracks and the discography side by side from this width.
const TWO_COLUMNS: f32 = 820.0;
const GAP: f32 = 32.0;
/// The discography: three columns of covers with title and year under each.
const RELEASES: Grid = Grid { columns: GridColumns::Fixed(3), gap: 12.0, row_gap: 12.0, text: 30.0 };
/// The discography's sections (see [`crate::model::discography`]) and their tabs.
const DISCOGRAPHY: [(ReleaseGroup, &str); 3] =
    [(ReleaseGroup::Albums, "Albums"), (ReleaseGroup::SinglesAndEps, "Singles"), (ReleaseGroup::LiveAndCompilations, "Live")];

impl App {
    pub(in crate::ui) fn artist_view(&mut self, ui: &mut Ui, st: &Status, artist: &ArtistView, acts: &mut Actions) {
        let now = (st.track.as_ref().map(|t| &t.id), st.state == State::Playing);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let width = ui.available_width();
            let (head, _) = ui.allocate_exact_size(vec2(width, HEADER), Sense::hover());
            self.artist_header(ui, head, artist);
            let (row, _) = ui.allocate_exact_size(vec2(width, BUTTONS), Sense::hover());
            self.artist_buttons(ui, row, artist, acts);

            let Some(page) = self.browse.artist_page.ready() else {
                ui.add_space(24.0);
                widgets::load_state(ui, self.browse.artist_page.is_loading(), self.browse.artist_page.error(), None);
                return;
            };
            let discography: Vec<(&str, &Section)> = DISCOGRAPHY
                .iter()
                .filter_map(|(group, tab)| page.sections.iter().find(|s| s.title == group.title()).map(|s| (*tab, s)))
                .collect();

            // Popular on the left, discography on the right; stacked when narrow.
            let two = width >= TWO_COLUMNS && !page.top.is_empty() && !discography.is_empty();
            let left_width = if two { (width - GAP) * 0.55 } else { width };
            let right_width = if two { width - GAP - left_width } else { width };
            let popular = page.top.len().min(POPULAR);
            let popular_height = if popular == 0 { 0.0 } else { 36.0 + popular as f32 * metrics::POPULAR_ROW + 34.0 };
            // The tabs, then the timeline: everything by year.
            let tab = self.browse.discography_tab.min(discography.len());
            let on_timeline = !discography.is_empty() && tab == discography.len();
            let years = if on_timeline { timeline(&discography) } else { Vec::new() };
            let items = discography.get(tab).map_or(&[][..], |(_, s)| &s.items[..]);
            let grid_height = match (discography.is_empty(), on_timeline) {
                (true, _) => 0.0,
                (false, true) => 40.0 + timeline_height(&years),
                (false, false) => 40.0 + RELEASES.height(items.len(), right_width),
            };
            let (area, _) = if two {
                ui.allocate_exact_size(vec2(width, popular_height.max(grid_height)), Sense::hover())
            } else {
                ui.allocate_exact_size(vec2(width, popular_height + grid_height), Sense::hover())
            };
            ui.add_space(8.0);
            let left = Rect::from_min_size(area.min + vec2(0.0, 20.0), vec2(left_width, popular_height));
            let right = if two {
                Rect::from_min_size(pos2(area.right() - right_width, area.top() + 20.0), vec2(right_width, grid_height))
            } else {
                Rect::from_min_size(pos2(area.left(), area.top() + 20.0 + popular_height), vec2(width, grid_height))
            };

            if popular > 0 {
                caption(ui, left.min, "POPULAR");
                let columns = Columns::popular(left.left(), left.width());
                let liked = |id: &TrackId| self.library.likes.likes.tracks.contains(id);
                let table = Table {
                    tracks: &page.top,
                    columns: &columns,
                    now,
                    liked: &liked,
                    menu: RowMenu {
                        playlists: Playlists { all: &self.library.playlists, recent: &self.prefs.recent_playlists },
                        removable: false,
                        mix: self.features().track_mix,
                    },
                };
                widgets::track_rows(ui, &mut self.covers, &table, None, 0..popular, left.min + vec2(0.0, 28.0), left.width(), acts);
                if page.top.len() > POPULAR {
                    let at = left.min + vec2(8.0, 28.0 + popular as f32 * columns.row + 16.0);
                    let label = format!("show all {} →", page.top.len());
                    if widgets::text_link(ui, "artist-show-all", at, &label, ty::META, colors().accent, colors().text, left.width())
                        .0
                        .clicked()
                    {
                        acts.push(Action::OpenCollection(top_tracks(artist)));
                    }
                }
            }
            if !discography.is_empty() {
                caption(ui, right.min, "DISCOGRAPHY");
                let mut labels: Vec<&str> = discography.iter().map(|(tab, _)| *tab).collect();
                labels.push("Timeline");
                if let Some(i) = tabs(ui, pos2(right.right(), right.top() + 10.0), &labels, tab) {
                    self.browse.discography_tab = i;
                }
                let grid = Rect::from_min_max(right.min + vec2(0.0, 40.0), right.max);
                if on_timeline {
                    let plays = self.prefs.history_on.then_some(&self.plays);
                    show_timeline(ui, &mut self.covers, plays, grid, &years, acts);
                } else {
                    RELEASES.show(ui, grid, items.len(), |ui, k| release(ui, &mut self.covers, &items[k], acts));
                }
            }
            // Playlists featuring them, related artists.
            for (i, section) in page.sections.iter().enumerate() {
                if !DISCOGRAPHY.iter().any(|(group, _)| section.title == group.title()) {
                    card_row(ui, &mut self.covers, ("artist-row", i), section, st, self.prefs.new_only(), acts);
                }
            }
            ui.add_space(28.0);
        });
    }

    /// Dark band: tag, fans and releases, a very large name, and the picture in a
    /// dashed lime ring on the right.
    fn artist_header(&mut self, ui: &mut Ui, head: Rect, artist: &ArtistView) {
        let p = colors();
        let page = self.browse.artist_page.ready();
        header_band(ui.painter(), head);
        // Faint vertical rules, like the mockup's backdrop.
        let mut x = head.left() + 48.0;
        while x < head.right() {
            ui.painter().vline(x, head.y_range(), egui::Stroke::new(1.0, p.bg));
            x += 48.0;
        }

        let diameter = (head.height() - 40.0).min(head.width() * 0.32);
        let center = pos2(head.right() - 30.0 - diameter / 2.0, head.center().y + 4.0);
        let art = Rect::from_center_size(center, Vec2::splat(diameter));
        let picture = page.and_then(|pg| pg.picture.clone()).or_else(|| artist.picture.clone());
        widgets::cover(ui, &mut self.covers, picture.as_ref(), metrics::HEADER_PX, art, (diameter / 2.0) as u8);
        dashed_ring(ui.painter(), center, diameter / 2.0 + 10.0, p.accent);

        let x = head.left() + 4.0;
        let width = art.left() - 24.0 - x;
        let tag = widgets::tag(ui.painter(), pos2(x, head.bottom() - 118.0), "ARTIST");
        if let Some(pg) = page {
            let releases: usize = DISCOGRAPHY
                .iter()
                .filter_map(|(group, _)| pg.sections.iter().find(|s| s.title == group.title()))
                .map(|s| s.items.len())
                .sum();
            // A music server doesn't count fans.
            let meta =
                if pg.fans > 0 { format!("{} FANS · {releases} RELEASES", thousands(pg.fans)) } else { format!("{releases} RELEASES") };
            text_left(ui.painter(), pos2(tag.right() + 10.0, tag.center().y), &meta, ty::META, p.dim, x + width - tag.right() - 10.0);
        }
        let name = page.map_or(artist.name.as_str(), |pg| pg.name.as_str());
        text_left(ui.painter(), pos2(x, head.bottom() - 58.0), name, ty::ARTIST_NAME, p.text, width);
    }

    fn artist_buttons(&self, ui: &mut Ui, row: Rect, artist: &ArtistView, acts: &mut Actions) {
        ui.painter().hline(row.x_range(), row.bottom(), egui::Stroke::new(1.0, colors().line));
        let top = self.browse.artist_page.ready().map_or(&[][..], |pg| &pg.top[..]);
        let has_top = !top.is_empty();
        let buttons = Rect::from_min_size(pos2(row.left(), row.center().y - 22.0), vec2(row.width(), 44.0));
        let what = Likeable::Artist(artist.id.clone());
        let following = self.library.likes.likes.contains(&what);
        let (follow, follow_icon) = if following { ("Following", Icon::Heart) } else { ("Follow", Icon::HeartOutline) };
        // No room for every label: the buttons after Play show only their icon.
        let full = widgets::pill_width(ui, "Play", true, true)
            + widgets::pill_width(ui, "Shuffle", false, false)
            + widgets::pill_width(ui, "Mix", true, false)
            + widgets::pill_width(ui, follow, true, false)
            + if self.prefs.crate_on { widgets::pill_width(ui, "Save", true, false) } else { 0.0 }
            + 4.0 * ui.spacing().item_spacing.x;
        let narrow = full > buttons.width();
        ui.scope_builder(UiBuilder::new().max_rect(buttons).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            if has_top && widgets::pill(ui, "Play", Some(Icon::Play), true).clicked() {
                acts.push(Action::PlayFrom { tracks: top.to_vec(), index: 0, shuffle: false });
            }
            if has_top
                && (if narrow { widgets::icon_pill(ui, Icon::Shuffle, "Shuffle") } else { widgets::pill(ui, "Shuffle", None, false) })
                    .clicked()
            {
                acts.push(Action::PlayFrom { tracks: top.to_vec(), index: 0, shuffle: true });
            }
            let mix = widgets::pill_fit(ui, "Mix", Icon::Streaming, narrow).on_hover_text("Songs by them and artists like them");
            if mix.clicked() {
                acts.push(Action::PlaySource(CollSource::ArtistMix(artist.id.clone()), PlayMode::Now));
            }
            if widgets::pill_fit(ui, follow, follow_icon, narrow).clicked() {
                acts.push(Action::ToggleLike(what));
            }
            if self.prefs.crate_on
                && widgets::pill_fit(ui, "Save", Icon::Crate, narrow).on_hover_text("Save to your Crate, to check out later").clicked()
            {
                let thing = Thing::Artist { id: artist.id.clone(), name: artist.name.clone(), picture: artist.picture.clone() };
                acts.push(Action::SaveToCrate(thing, String::new()));
            }
        });
    }
}

/// The artist's popular tracks as a plain list ("show all").
fn top_tracks(artist: &ArtistView) -> Coll {
    Coll {
        source: CollSource::TopTracks(artist.id.clone()),
        kind: "POPULAR TRACKS",
        title: artist.name.clone(),
        subtitle: String::new(),
        picture: artist.picture.clone(),
    }
}

/// Mono uppercase caption at the top left of a column.
fn caption(ui: &Ui, at: egui::Pos2, text: &str) {
    text_left(ui.painter(), at + vec2(0.0, 10.0), text, ty::OVERLINE, colors().dim, 200.0);
}

/// Small mono tabs, right-aligned at `right_top`; returns the one clicked.
fn tabs(ui: &Ui, right_top: egui::Pos2, labels: &[&str], selected: usize) -> Option<usize> {
    let p = colors();
    let mut x = right_top.x;
    let mut clicked = None;
    for (i, label) in labels.iter().enumerate().rev() {
        let width = ui.painter().layout_no_wrap(label.to_string(), ty::META.font(), p.text).size().x + 16.0;
        let rect = Rect::from_min_size(pos2(x - width, right_top.y - 10.0), vec2(width, 22.0));
        x -= width + 4.0;
        let resp = ui.interact(rect, ui.scope_id().with(("discography-tab", i)), Sense::click());
        let on = i == selected;
        let color = match (on, resp.hovered()) {
            (true, _) => p.bg,
            (false, true) => p.text,
            (false, false) => p.dim,
        };
        if on {
            ui.painter().rect_filled(rect, CornerRadius::same(radius::THUMB), p.text);
        }
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, ty::META.font(), color);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// One release in the discography grid: cover, title and year; a lime outline on
/// hover.
fn release(ui: &mut Ui, covers: &mut Covers, item: &Item, acts: &mut Actions) {
    let Some(coll) = Coll::from_item(item) else { return };
    let p = colors();
    let rect = ui.max_rect();
    let size = rect.width();
    let art = Rect::from_min_size(rect.min, Vec2::splat(size));
    let resp = ui.interact(rect, ui.scope_id().with(("release", &item.id)), Sense::click());
    widgets::cover(ui, covers, item.picture.as_ref(), metrics::GRID_CARD_PX, art, radius::CARD);
    if resp.hovered() {
        let ring = art.expand(3.0);
        ui.painter().rect_stroke(ring, CornerRadius::same(radius::CONTROL), egui::Stroke::new(1.5, p.accent), egui::StrokeKind::Inside);
    }
    let year = item.release.as_ref().and_then(|r| r.year.as_deref()).unwrap_or_default();
    let year_width = ui.painter().layout_no_wrap(year.to_string(), ty::META.font(), p.dim).size().x;
    let y = art.bottom() + 14.0;
    ui.painter().text(pos2(art.right(), y), Align2::RIGHT_CENTER, year, ty::META.font(), p.dim);
    text_left(ui.painter(), pos2(art.left(), y), &item.title, ty::BODY.strong(), p.text, size - year_width - 8.0);
    if let Some(mode) = widgets::queue_menu(&resp) {
        acts.push(Action::PlaySource(coll.source.clone(), mode));
    }
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        acts.push(Action::OpenCollection(coll));
    }
}

const YEAR_HEAD: f32 = 34.0;
const TIMELINE_ROW: f32 = 44.0;
/// The line above the timeline: how much of it was heard.
const TIMELINE_SUMMARY: f32 = 28.0;

/// Every release of the discography by year, newest first; ones without a year
/// last.
fn timeline<'a>(discography: &[(&str, &'a Section)]) -> Vec<(String, Vec<&'a Item>)> {
    let mut years: Vec<(String, Vec<&Item>)> = Vec::new();
    for (_, section) in discography {
        for item in &section.items {
            let year = item.release.as_ref().and_then(|r| r.year.clone()).unwrap_or_default();
            match years.iter_mut().find(|(y, _)| *y == year) {
                Some((_, items)) => items.push(item),
                None => years.push((year, vec![item])),
            }
        }
    }
    // Newest first; "" (no year) sorts last.
    years.sort_by(|a, b| match (a.0.is_empty(), b.0.is_empty()) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => b.0.cmp(&a.0),
    });
    years
}

fn timeline_height(years: &[(String, Vec<&Item>)]) -> f32 {
    TIMELINE_SUMMARY + years.iter().map(|(_, items)| YEAR_HEAD + items.len() as f32 * TIMELINE_ROW).sum::<f32>()
}

/// The timeline: how many releases were heard, then each year with its releases,
/// each marked heard (a track of it is in the play history) or not; without
/// `plays` (the history turned off), only the years and releases.
fn show_timeline(
    ui: &mut Ui,
    covers: &mut Covers,
    plays: Option<&crate::ui::plays::Plays>,
    area: Rect,
    years: &[(String, Vec<&Item>)],
    acts: &mut Actions,
) {
    let p = colors();
    let all: usize = years.iter().map(|(_, items)| items.len()).sum();
    let heard_album = |id: &str| plays.is_some_and(|p| p.has_heard_album(id));
    let heard = years.iter().flat_map(|(_, items)| items).filter(|item| heard_album(&item.id)).count();
    let summary = if plays.is_some() { format!("You've heard {heard} of {all} releases") } else { format!("{all} releases") };
    text_left(ui.painter(), pos2(area.left(), area.top() + 8.0), &summary, ty::SMALL, p.dim, area.width());
    let mut y = area.top() + TIMELINE_SUMMARY;
    for (year, items) in years {
        let label = if year.is_empty() { "Undated" } else { year.as_str() };
        text_left(ui.painter(), pos2(area.left(), y + YEAR_HEAD / 2.0 + 4.0), label, ty::SECTION_TITLE, p.text, area.width());
        ui.painter().hline((area.left() + 70.0)..=area.right(), y + YEAR_HEAD / 2.0 + 4.0, egui::Stroke::new(1.0, p.line));
        y += YEAR_HEAD;
        for item in items {
            let rect = Rect::from_min_size(pos2(area.left(), y), vec2(area.width(), TIMELINE_ROW));
            y += TIMELINE_ROW;
            let Some(coll) = Coll::from_item(item) else { continue };
            let resp = ui.interact(rect, ui.scope_id().with(("timeline", &item.id)), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
            }
            let art = Rect::from_min_size(pos2(rect.left() + 6.0, rect.center().y - 17.0), Vec2::splat(34.0));
            widgets::cover(ui, covers, item.picture.as_ref(), metrics::THUMB_PX, art, radius::THUMB);
            let was_heard = heard_album(&item.id);
            let (mark, mark_color) = match (plays, was_heard) {
                (None, _) => ("", p.faint),
                (_, true) => ("heard", p.accent),
                (_, false) => ("not heard", p.faint),
            };
            let mark_width = ui.painter().layout_no_wrap(mark.into(), ty::META.font(), mark_color).size().x;
            ui.painter().text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, mark, ty::META.font(), mark_color);
            let x = art.right() + 12.0;
            let width = rect.right() - x - mark_width - 20.0;
            let kind = item.release.as_ref().map_or("", |r| r.kind.label());
            let title_color = if was_heard || plays.is_none() { p.text } else { p.dim };
            let title = widgets::Line { text: &item.title, style: ty::BODY.strong(), color: title_color };
            widgets::two_lines(
                ui.painter(),
                x,
                rect.center().y,
                width,
                title,
                widgets::Line { text: kind, style: ty::CAPTION, color: p.faint },
            );
            if let Some(mode) = widgets::queue_menu(&resp) {
                acts.push(Action::PlaySource(coll.source.clone(), mode));
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                acts.push(Action::OpenCollection(coll));
            }
        }
    }
}

/// A dashed circle around the artist's picture.
fn dashed_ring(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: egui::Color32) {
    let dashes = 72;
    let step = std::f32::consts::TAU / dashes as f32;
    for k in 0..dashes {
        let a = k as f32 * step;
        let (from, to) = (a, a + step * 0.35);
        let point = |t: f32| center + vec2(t.cos(), t.sin()) * radius;
        painter.line_segment([point(from), point(to)], egui::Stroke::new(1.5, color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ItemKind, ReleaseInfo, ReleaseKind};

    fn item(id: &str, kind: ReleaseKind, year: &str) -> Item {
        let release = ReleaseInfo::new(kind, year);
        Item { kind: ItemKind::Album, id: id.into(), title: id.into(), subtitle: release.subtitle(), picture: None, release: Some(release) }
    }

    #[test]
    fn the_timeline_goes_by_year_newest_first() {
        let albums = Section::new("Albums", vec![item("a", ReleaseKind::Album, "2024"), item("b", ReleaseKind::Album, "2019")]);
        let singles = Section::new("Singles & EPs", vec![item("c", ReleaseKind::Single, "2024"), item("d", ReleaseKind::Single, "")]);
        let years = timeline(&[("Albums", &albums), ("Singles", &singles)]);
        let shown: Vec<(&str, Vec<&str>)> =
            years.iter().map(|(y, items)| (y.as_str(), items.iter().map(|i| i.id.as_str()).collect())).collect();
        assert_eq!(shown, [("2024", vec!["a", "c"]), ("2019", vec!["b"]), ("", vec!["d"])]);
    }
}
