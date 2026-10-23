//! Artist page: a header band with the name and picture, Play / Shuffle / Follow,
//! then popular tracks beside the discography (albums, singles & EPs, live &
//! compilations, as tabs), then playlists featuring the artist and related artists.

use super::collection::header_band;
use super::home::card_row;
use crate::model::{Item, Likeable, Section, TrackId};
use crate::player::{State, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::covers::Covers;
use crate::ui::format::thousands;
use crate::ui::state::{ArtistView, Coll, Source};
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, Columns, Grid, GridColumns, RowMenu, Table, text_left};
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
/// Discography section titles (see `deezer::parse::discography`) and their tabs.
const DISCOGRAPHY: [(&str, &str); 3] = [("Albums", "Albums"), ("Singles & EPs", "Singles"), ("Live & compilations", "Live")];

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
            let discography: Vec<(&str, &Section)> =
                DISCOGRAPHY.iter().filter_map(|(title, tab)| page.sections.iter().find(|s| s.title == *title).map(|s| (*tab, s))).collect();

            // Popular on the left, discography on the right; stacked when narrow.
            let two = width >= TWO_COLUMNS && !page.top.is_empty() && !discography.is_empty();
            let left_width = if two { (width - GAP) * 0.55 } else { width };
            let right_width = if two { width - GAP - left_width } else { width };
            let popular = page.top.len().min(POPULAR);
            let popular_height = if popular == 0 { 0.0 } else { 36.0 + popular as f32 * metrics::POPULAR_ROW + 34.0 };
            let tab = self.browse.discography_tab.min(discography.len().saturating_sub(1));
            let items = discography.get(tab).map_or(&[][..], |(_, s)| &s.items[..]);
            let grid_height = if discography.is_empty() { 0.0 } else { 40.0 + RELEASES.height(items.len(), right_width) };
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
                    menu: RowMenu { playlists: &self.library.playlists, removable: false },
                };
                widgets::track_rows(ui, &mut self.covers, &table, 0..popular, left.min + vec2(0.0, 28.0), left.width(), acts);
                if page.top.len() > POPULAR {
                    let at = left.min + vec2(8.0, 28.0 + popular as f32 * columns.row + 16.0);
                    let label = format!("show all {} →", page.top.len());
                    if widgets::text_link(ui, "artist-show-all", at, &label, ty::META, colors().accent, colors().text, left.width()).0 {
                        acts.push(Action::OpenCollection(top_tracks(artist)));
                    }
                }
            }
            if !discography.is_empty() {
                caption(ui, right.min, "DISCOGRAPHY");
                let labels: Vec<&str> = discography.iter().map(|(tab, _)| *tab).collect();
                if let Some(i) = tabs(ui, pos2(right.right(), right.top() + 10.0), &labels, tab) {
                    self.browse.discography_tab = i;
                }
                let grid = Rect::from_min_max(right.min + vec2(0.0, 40.0), right.max);
                RELEASES.show(ui, grid, items.len(), |ui, k| release(ui, &mut self.covers, &items[k], acts));
            }
            // Playlists featuring them, related artists.
            for (i, section) in page.sections.iter().enumerate() {
                if !DISCOGRAPHY.iter().any(|(title, _)| section.title == *title) {
                    card_row(ui, &mut self.covers, ("artist-row", i), &section.title, &section.items, st, acts);
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
            let releases: usize =
                DISCOGRAPHY.iter().filter_map(|(title, _)| pg.sections.iter().find(|s| s.title == *title)).map(|s| s.items.len()).sum();
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
        ui.scope_builder(UiBuilder::new().max_rect(buttons).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            if has_top && widgets::pill(ui, "Play", Some(Icon::Play), true).clicked() {
                acts.push(Action::PlayFrom { tracks: top.to_vec(), index: 0, shuffle: false });
            }
            if has_top && widgets::pill(ui, "Shuffle", None, false).clicked() {
                acts.push(Action::PlayFrom { tracks: top.to_vec(), index: 0, shuffle: true });
            }
            let what = Likeable::Artist(artist.id.clone());
            let following = self.library.likes.likes.contains(&what);
            let (label, icon) = if following { ("Following", Icon::Heart) } else { ("Follow", Icon::HeartOutline) };
            if widgets::pill(ui, label, Some(icon), false).clicked() {
                acts.push(Action::ToggleLike(what));
            }
        });
    }
}

/// The artist's popular tracks as a plain list ("show all").
fn top_tracks(artist: &ArtistView) -> Coll {
    Coll {
        source: Source::TopTracks(artist.id.clone()),
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
        let resp = ui.interact(rect, ui.id().with(("discography-tab", i)), Sense::click());
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
    let resp = ui.interact(rect, ui.id().with(("release", &item.id)), Sense::click());
    widgets::cover(ui, covers, item.picture.as_ref(), metrics::GRID_CARD_PX, art, radius::CARD);
    if resp.hovered() {
        let ring = art.expand(3.0);
        ui.painter().rect_stroke(ring, CornerRadius::same(radius::CONTROL), egui::Stroke::new(1.5, p.accent), egui::StrokeKind::Inside);
    }
    // "2026 · Album" → "2026".
    let year = item.subtitle.split(" · ").next().unwrap_or_default();
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
