//! Custom-painted widgets for the Tidal-like look.

use super::covers::Covers;
use super::icons::{self, Icon};
use super::theme::*;
use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{self, Color32, CornerRadius, FontId, Galley, Painter, Pos2, Rect, Response, Sense, Ui, Vec2, vec2};
use std::sync::Arc;

/// Single-line galley cut with "…" at `max_width`.
pub fn line(painter: &Painter, text: &str, font: FontId, color: Color32, max_width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = TextWrapping { max_width: max_width.max(1.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    painter.layout_job(job)
}

/// Paint single-line text with its left edge at `pos.x`, vertically centred on `pos.y`.
pub fn text_left(painter: &Painter, pos: Pos2, text: &str, font: FontId, color: Color32, max_width: f32) -> Rect {
    let g = line(painter, text, font, color, max_width);
    let top = pos2_(pos.x, pos.y - g.size().y / 2.0);
    let rect = Rect::from_min_size(top, g.size());
    painter.galley(top, g, color);
    rect
}

fn pos2_(x: f32, y: f32) -> Pos2 {
    egui::pos2(x, y)
}

pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size + 12.0), Sense::click());
    let tint = if resp.hovered() { c().text } else { color };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(size)), icon, tint);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// White circular play/pause button.
pub fn play_circle(ui: &mut Ui, playing: bool, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let scale = if resp.hovered() { 1.06 } else { 1.0 };
    ui.painter().circle_filled(rect.center(), size / 2.0 * scale, c().text);
    let icon = if playing { Icon::Pause } else { Icon::Play };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(size * 0.42)), icon, c().bg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn pill(ui: &mut Ui, label: &str, icon: Option<Icon>, primary: bool) -> Response {
    let font = bold(14.0);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
    let size = vec2(galley.size().x + icon_w + 36.0, 40.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let (bg, fg) = match (primary, resp.hovered()) {
        (true, false) => (c().text, c().bg),
        (true, true) => (c().accent, c().bg),
        (false, false) => (c().surface, c().text),
        (false, true) => (c().raised, c().text),
    };
    ui.painter().rect_filled(rect, CornerRadius::same(20), bg);
    let mut x = rect.left() + 18.0;
    if let Some(i) = icon {
        icons::paint(ui.painter(), Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), Vec2::splat(16.0)), i, fg);
        x += icon_w;
    }
    ui.painter().galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn nav_item(ui: &mut Ui, icon: Icon, label: &str, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, CornerRadius::same(6), c().surface);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), c().hover);
    }
    let color = if selected || resp.hovered() { c().text } else { c().dim };
    icons::paint(ui.painter(), Rect::from_center_size(egui::pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(17.0)), icon, color);
    let font = if selected { bold(14.0) } else { regular(14.0) };
    text_left(ui.painter(), egui::pos2(rect.left() + 40.0, rect.center().y), label, font, color, rect.width() - 48.0);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Thin progress/volume bar. `value` is 0..=1. Reports `changed()` while dragging or
/// on click; `drag_stopped()` when a drag ends.
pub fn thin_slider(ui: &mut Ui, value: &mut f32, width: f32, enabled: bool) -> Response {
    let sense = if enabled { Sense::click_and_drag() } else { Sense::hover() };
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 16.0), sense);
    let active = enabled && (resp.hovered() || resp.dragged());
    if enabled
        && (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        *value = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        resp.mark_changed();
    }
    let h = if active { 5.0 } else { 3.0 };
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), h));
    let p = ui.painter();
    p.rect_filled(track, CornerRadius::same(3), c().line);
    let fill = Rect::from_min_max(track.min, egui::pos2(track.left() + track.width() * *value, track.bottom()));
    p.rect_filled(fill, CornerRadius::same(3), if active { c().accent } else { c().text });
    if active {
        p.circle_filled(egui::pos2(fill.right(), track.center().y), 6.0, c().text);
    }
    if enabled {
        resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Rounded cover image, or a subtle placeholder while it loads.
pub fn cover(ui: &Ui, covers: &mut Covers, url: Option<&str>, rect: Rect, radius: u8) {
    match url.and_then(|u| covers.get(u)) {
        Some(tex) => {
            egui::Image::from_texture((tex, rect.size())).corner_radius(radius).paint_at(ui, rect);
        }
        None => {
            ui.painter().rect_filled(rect, CornerRadius::same(radius), c().surface);
            icons::paint(ui.painter(), Rect::from_center_size(rect.center(), rect.size() * 0.36), Icon::Note, c().faint);
        }
    }
}

/// A rounded colour tile with an icon, for collections without artwork.
pub fn tile(ui: &Ui, rect: Rect, icon: Icon, color: Color32, radius: u8) {
    ui.painter().rect_filled(rect, CornerRadius::same(radius), color);
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), rect.size() * 0.34), icon, Color32::from_white_alpha(235));
}
