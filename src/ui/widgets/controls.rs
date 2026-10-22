//! Buttons and sliders.

use super::text::text_left;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, radius, typography as ty};
use eframe::egui::{self, Color32, CornerRadius, Rect, Response, Sense, Ui, Vec2, pos2, vec2};

const PILL_HEIGHT: f32 = 40.0;

/// Icon-only button; brightens on hover.
pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size + 12.0), Sense::click());
    let tint = if resp.hovered() { colors().text } else { color };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(size)), icon, tint);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A heart button at a fixed spot that takes no layout space, for rows and bars
/// drawn at absolute positions (allocating there would grow a scroll area's
/// content on hover and make the list jump).
pub fn heart_at(ui: &Ui, center: egui::Pos2, id: impl std::hash::Hash + std::fmt::Debug, liked: bool, size: f32) -> Response {
    let rect = Rect::from_center_size(center, Vec2::splat(size + 12.0));
    let resp = ui.interact(rect, ui.id().with(("heart", id)), Sense::click());
    paint_heart(ui, rect, resp, liked, size)
}

fn paint_heart(ui: &Ui, rect: Rect, resp: Response, liked: bool, size: f32) -> Response {
    let p = colors();
    let (icon, color) = match (liked, resp.hovered()) {
        (true, _) => (Icon::Heart, p.accent),
        (false, true) => (Icon::HeartOutline, p.text),
        (false, false) => (Icon::HeartOutline, p.dim),
    };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(size)), icon, color);
    let hint = if liked { "Remove from your likes" } else { "Like" };
    resp.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Square play/pause button in the text colour; lime on hover.
pub fn play_button(ui: &mut Ui, playing: bool, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let p = colors();
    let (bg, fg) = if resp.hovered() { (p.accent_fill, p.on_accent) } else { (p.text, p.bg) };
    ui.painter().rect_filled(rect, CornerRadius::same(radius::CONTROL), bg);
    let icon = if playing { Icon::Pause } else { Icon::Play };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(size * 0.38)), icon, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Button with an optional leading icon: `primary` is lime-filled, the others are
/// outlined.
pub fn pill(ui: &mut Ui, label: &str, icon: Option<Icon>, primary: bool) -> Response {
    let style = if primary { ty::TITLE.strong() } else { ty::TITLE };
    let galley = ui.painter().layout_no_wrap(label.to_string(), style.font(), Color32::PLACEHOLDER);
    let icon_width = if icon.is_some() { 22.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + icon_width + 32.0, PILL_HEIGHT), Sense::click());
    let p = colors();
    let corner = CornerRadius::same(radius::CONTROL);
    let fg = if primary {
        let bg = if resp.hovered() { p.accent_fill.lerp_to_gamma(Color32::WHITE, 0.3) } else { p.accent_fill };
        ui.painter().rect_filled(rect, corner, bg);
        p.on_accent
    } else {
        if resp.hovered() {
            ui.painter().rect_filled(rect, corner, p.raised);
        }
        ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
        p.text
    };
    let mut x = rect.left() + 16.0;
    if let Some(i) = icon {
        icons::paint(ui.painter(), Rect::from_center_size(pos2(x + 8.0, rect.center().y), Vec2::splat(16.0)), i, fg);
        x += icon_width;
    }
    ui.painter().galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Thin progress/volume bar. `value` is 0..=1; `lime` fills it with the accent
/// (progress) rather than the text colour (volume). Reports `changed()` while dragging
/// or on click; `drag_stopped()` when a drag ends.
pub fn thin_slider(ui: &mut Ui, value: &mut f32, width: f32, enabled: bool, lime: bool) -> Response {
    let sense = if enabled { Sense::click_and_drag() } else { Sense::hover() };
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 16.0), sense);
    let active = enabled && (resp.hovered() || resp.dragged());
    if enabled
        && (resp.dragged() || resp.clicked())
        && let Some(pointer) = resp.interact_pointer_pos()
    {
        let new = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        // Only a real change counts; a held, still mouse would otherwise report one
        // every frame.
        if new != *value {
            *value = new;
            resp.mark_changed();
        }
    }
    let p = colors();
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), if active { 4.0 } else { 3.0 }));
    let fill = Rect::from_min_max(track.min, pos2(track.left() + track.width() * *value, track.bottom()));
    let painter = ui.painter();
    painter.rect_filled(track, CornerRadius::same(1), p.line);
    painter.rect_filled(fill, CornerRadius::same(1), if lime || active { p.accent } else { p.text });
    if active {
        let knob = Rect::from_center_size(pos2(fill.right(), track.center().y), Vec2::splat(10.0));
        painter.rect_filled(knob, CornerRadius::same(2), p.text);
    }
    if enabled {
        resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Segmented control filling the available width; returns the clicked index.
pub fn segmented(ui: &mut Ui, items: &[(Icon, &str)], selected: usize) -> Option<usize> {
    let p = colors();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(radius::CONTROL), p.bg);
    ui.painter().rect_stroke(rect, CornerRadius::same(radius::CONTROL), egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
    let segment_width = (rect.width() - 6.0) / items.len() as f32;
    let mut clicked = None;
    for (i, (icon, label)) in items.iter().enumerate() {
        let seg = Rect::from_min_size(
            pos2(rect.left() + 3.0 + i as f32 * segment_width, rect.top() + 3.0),
            vec2(segment_width, rect.height() - 6.0),
        );
        let resp = ui.interact(seg, ui.id().with(("segment", i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let on = i == selected;
        if on {
            ui.painter().rect_filled(seg, CornerRadius::same(5), p.text);
        }
        let color = match (on, resp.hovered()) {
            (true, _) => p.bg,
            (false, true) => p.text,
            (false, false) => p.dim,
        };
        let style = if on { ty::SECONDARY.strong() } else { ty::SECONDARY };
        let galley = ui.painter().layout_no_wrap(label.to_string(), style.font(), color);
        let x = seg.center().x - (14.0 + 6.0 + galley.size().x) / 2.0;
        icons::paint(ui.painter(), Rect::from_center_size(pos2(x + 7.0, seg.center().y), Vec2::splat(14.0)), *icon, color);
        text_left(ui.painter(), pos2(x + 20.0, seg.center().y), label, style, color, galley.size().x + 1.0);
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// Small lime tag ("ALBUM", "ARTIST") with its left edge at `pos.x`, centred on
/// `pos.y`. Returns its rect.
pub fn tag(painter: &egui::Painter, pos: egui::Pos2, text: &str) -> Rect {
    let p = colors();
    let style = ty::META.strong();
    let galley = painter.layout_no_wrap(text.to_string(), style.font(), p.on_accent);
    let rect = Rect::from_min_size(pos2(pos.x, pos.y - 9.0), vec2(galley.size().x + 12.0, 18.0));
    painter.rect_filled(rect, CornerRadius::same(radius::THUMB), p.accent_fill);
    painter.galley(pos2(rect.left() + 6.0, rect.center().y - galley.size().y / 2.0), galley, p.on_accent);
    rect
}
