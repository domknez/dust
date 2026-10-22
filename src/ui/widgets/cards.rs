//! Cards and rows that show a collection, Flow and its moods, or a queued track.

use super::art::cover;
use super::menu::queue_menu;
use super::text::{artist_links, text_left};
use crate::deezer::{ArtistRef, Track, image_url};
use crate::ui::covers::Covers;
use crate::ui::format::{cover_url, mmss};
use crate::ui::state::{Coll, PlayMode};
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use eframe::egui::{self, Align2, CornerRadius, Rect, Sense, Ui, Vec2, pos2, vec2};

/// What the listener did with a card.
pub enum CardClick {
    None,
    Open,
    Play(PlayMode),
}

/// Home card: artwork (round for artists), title, subtitle; a play button on hover
/// and a right-click queue menu.
pub fn card(ui: &mut Ui, covers: &mut Covers, coll: &Coll) -> CardClick {
    let w = metrics::HOME_CARD;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, w + metrics::CARD_TEXT), Sense::click());
    let art = Rect::from_min_size(rect.min, Vec2::splat(w));
    let corner = if coll.round() { (w / 2.0) as u8 } else { radius::CARD };
    let url = coll.picture.as_ref().map(|(k, m)| image_url(k, m, metrics::HOME_CARD_PX));
    cover(ui, covers, url.as_deref(), art, corner);
    let mut click = CardClick::None;
    if resp.hovered() {
        ui.painter().rect_filled(art, CornerRadius::same(corner), colors().veil);
        let knob = pos2(art.right() - 28.0, art.bottom() - 28.0);
        let over_knob = resp.hover_pos().is_some_and(|p| p.distance(knob) < 22.0);
        play_knob(ui, knob, if over_knob { 22.0 } else { 20.0 });
        if resp.clicked() {
            click = if over_knob { CardClick::Play(PlayMode::Now) } else { CardClick::Open };
        }
    }
    if let Some(mode) = queue_menu(&resp) {
        click = CardClick::Play(mode);
    }
    text_left(ui.painter(), pos2(rect.left(), art.bottom() + 15.0), &coll.title, ty::ITEM.strong(), colors().text, w);
    text_left(ui.painter(), pos2(rect.left(), art.bottom() + 34.0), &coll.subtitle, ty::SMALL, colors().dim, w);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    click
}

/// Lime play button drawn over artwork; `half` is half its side.
pub fn play_knob(ui: &Ui, center: egui::Pos2, half: f32) {
    let p = colors();
    ui.painter().rect_filled(Rect::from_center_size(center, Vec2::splat(half * 2.0)), CornerRadius::same(radius::CONTROL), p.accent_fill);
    icons::paint(ui.painter(), Rect::from_center_size(center, Vec2::splat(half * 0.75)), Icon::Play, p.on_accent);
}

/// Height of [`flow_panel`].
const FLOW_PANEL: f32 = 176.0;

/// The Flow panel: title, a lime play button for plain Flow and the moods as a
/// segmented row (`moods` are (id, label); the playing one is lit). Returns plain
/// Flow (None) or a mood's id when one was picked.
pub fn flow_panel(ui: &mut Ui, moods: &[(&str, &str)], playing: Option<&str>) -> Option<Option<String>> {
    let p = colors();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), FLOW_PANEL), Sense::hover());
    let painter = ui.painter_at(rect);
    let corner = CornerRadius::same(radius::PANEL);
    painter.rect_filled(rect, corner, p.surface);
    sound_bars(&painter, Rect::from_min_max(pos2(rect.center().x, rect.top()), rect.max));
    painter.rect_stroke(rect, corner, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
    let left = rect.left() + 20.0;
    text_left(&painter, pos2(left, rect.top() + 26.0), "flow · endless, just for you", ty::META, p.dim, rect.width() - 100.0);
    text_left(&painter, pos2(left, rect.top() + 66.0), "Flow", ty::HERO, p.text, 200.0);
    let mut pick = None;

    let play = Rect::from_min_size(pos2(rect.right() - 68.0, rect.top() + 20.0), Vec2::splat(48.0));
    let resp = ui.interact(play, ui.id().with("flow-play"), Sense::click()).on_hover_text("Play Flow");
    let fill = if resp.hovered() { p.accent_fill.lerp_to_gamma(egui::Color32::WHITE, 0.3) } else { p.accent_fill };
    painter.rect_filled(play, CornerRadius::same(radius::CONTROL), fill);
    icons::paint(&painter, Rect::from_center_size(play.center(), Vec2::splat(20.0)), Icon::Play, p.on_accent);
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        pick = Some(None);
    }

    // Moods: chips in a tray along the bottom, as many as fit.
    let chip_h = 28.0;
    let tray_top = rect.bottom() - 20.0 - chip_h - 6.0;
    let mut x = left + 3.0;
    let mut chips = Vec::new();
    for (id, label) in moods {
        let label = label.to_lowercase();
        let width = painter.layout_no_wrap(label.clone(), ty::META.font(), p.text).size().x + 20.0;
        if x + width > rect.right() - 23.0 {
            break;
        }
        chips.push((Rect::from_min_size(pos2(x, tray_top + 3.0), vec2(width, chip_h)), *id, label));
        x += width + 4.0;
    }
    if let Some(last) = chips.last() {
        let tray = Rect::from_min_max(pos2(left, tray_top), pos2(last.0.right() + 3.0, tray_top + chip_h + 6.0));
        painter.rect_filled(tray, CornerRadius::same(radius::CONTROL), p.bg);
        painter.rect_stroke(tray, CornerRadius::same(radius::CONTROL), egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
    }
    for (chip, id, label) in chips {
        let resp = ui.interact(chip, ui.id().with(("flow-mood", id)), Sense::click());
        let on = playing == Some(id);
        let (bg, fg) = match (on, resp.hovered()) {
            (true, _) => (Some(p.accent_fill), p.on_accent),
            (false, true) => (Some(p.raised), p.text),
            (false, false) => (None, p.dim),
        };
        if let Some(bg) = bg {
            painter.rect_filled(chip, CornerRadius::same(5), bg);
        }
        painter.text(chip.center(), Align2::CENTER_CENTER, label, ty::META.font(), fg);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            pick = Some(Some(id.to_string()));
        }
    }
    pick
}

/// Faint lime bars, like a still of a level meter, behind the Flow panel.
fn sound_bars(painter: &egui::Painter, area: Rect) {
    let lime = colors().accent;
    let mut x = area.left() + 24.0;
    let mut i = 0.0_f32;
    while x < area.right() - 8.0 {
        let level = (0.5 + 0.5 * (i * 0.55).sin() * (i * 0.17).cos()).abs();
        let h = area.height() * (0.15 + 0.6 * level);
        let bar = Rect::from_center_size(pos2(x, area.center().y), vec2(4.0, h));
        painter.rect_filled(bar, CornerRadius::ZERO, lime.gamma_multiply(0.08 + 0.18 * level));
        x += 12.0;
        i += 1.0;
    }
}

/// One track in the queue panel. Returns the artist whose name was clicked.
pub fn queue_row(ui: &Ui, covers: &mut Covers, rect: Rect, t: &Track, current: bool, show_time: bool) -> Option<ArtistRef> {
    let art = Rect::from_min_size(pos2(rect.left() + 6.0, rect.center().y - metrics::LIST_ART / 2.0), Vec2::splat(metrics::LIST_ART));
    cover(ui, covers, cover_url(t, metrics::THUMB_PX).as_deref(), art, radius::THUMB);
    let x = art.right() + 12.0;
    let width = rect.right() - x - 52.0;
    let p = colors();
    let painter = ui.painter();
    text_left(painter, pos2(x, rect.center().y - 8.0), &t.title, ty::ITEM, if current { p.accent } else { p.text }, width);
    if show_time {
        painter.text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, mmss(t.duration as f64), ty::META.font(), p.dim);
    }
    artist_links(ui, pos2(x, rect.center().y + 10.0), &t.artists, &t.artist, ty::SMALL, p.dim, p.text, width).0
}
