//! Cards and rows that show a collection, a Flow mood or a queued track.

use super::art::cover;
use super::menu::queue_menu;
use super::text::{line, text_left};
use crate::deezer::{Item, Track, image_url};
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
    let (rect, resp) = ui.allocate_exact_size(vec2(w, w + 50.0), Sense::click());
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

/// Round play button drawn over artwork.
pub fn play_knob(ui: &Ui, center: egui::Pos2, radius: f32) {
    ui.painter().circle_filled(center, radius, colors().text);
    icons::paint(ui.painter(), Rect::from_center_size(center, Vec2::splat(radius * 0.75)), Icon::Play, colors().bg);
}

/// Round Flow mood tile; returns the mood to play when clicked (None = plain Flow).
pub fn flow_card(ui: &mut Ui, covers: &mut Covers, it: &Item, active: bool) -> Option<Option<String>> {
    let d = metrics::FLOW_TILE;
    // Leave room around the artwork for the selection ring so scroll areas don't clip it.
    let pad = 6.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(d + 2.0 * pad, d + 2.0 * pad + 30.0), Sense::click());
    let art = Rect::from_min_size(rect.min + Vec2::splat(pad), Vec2::splat(d));
    let url = it.picture.as_ref().map(|(k, m)| image_url(k, m, metrics::FLOW_TILE_PX));
    cover(ui, covers, url.as_deref(), art, (d / 2.0) as u8);
    let p = colors();
    if active || resp.hovered() {
        let ring = if active { p.accent } else { p.text.gamma_multiply(0.6) };
        ui.painter().circle_stroke(art.center(), d / 2.0 + 3.0, egui::Stroke::new(2.5, ring));
    }
    let (style, color) = if active { (ty::BODY.strong(), p.accent) } else { (ty::BODY, p.text) };
    let galley = line(ui.painter(), &it.title, style, color, d + 10.0);
    ui.painter().galley(pos2(art.center().x - galley.size().x / 2.0, art.bottom() + pad + 8.0), galley, color);
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    resp.clicked().then(|| (it.id != "default").then(|| it.id.clone()))
}

/// One track in the queue panel.
pub fn queue_row(ui: &Ui, covers: &mut Covers, rect: Rect, t: &Track, current: bool, show_time: bool) {
    let art = Rect::from_min_size(pos2(rect.left() + 6.0, rect.center().y - metrics::LIST_ART / 2.0), Vec2::splat(metrics::LIST_ART));
    cover(ui, covers, cover_url(t, metrics::THUMB_PX).as_deref(), art, radius::THUMB);
    let x = art.right() + 12.0;
    let width = rect.right() - x - 52.0;
    let p = colors();
    let painter = ui.painter();
    text_left(painter, pos2(x, rect.center().y - 8.0), &t.title, ty::ITEM, if current { p.accent } else { p.text }, width);
    text_left(painter, pos2(x, rect.center().y + 10.0), &t.artist, ty::SMALL, p.dim, width);
    if show_time {
        painter.text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, mmss(t.duration as f64), ty::SMALL.font(), p.faint);
    }
}
