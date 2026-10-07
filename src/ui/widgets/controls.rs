//! Buttons and sliders.

use super::text::text_left;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, typography as ty};
use eframe::egui::{self, Color32, CornerRadius, Rect, Response, Sense, Ui, Vec2, pos2, vec2};

const PILL_HEIGHT: f32 = 40.0;

/// Icon-only button; brightens on hover.
pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size + 12.0), Sense::click());
    let tint = if resp.hovered() { colors().text } else { color };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(size)), icon, tint);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Round play/pause button in the text colour.
pub fn play_circle(ui: &mut Ui, playing: bool, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let grow = if resp.hovered() { 1.06 } else { 1.0 };
    ui.painter().circle_filled(rect.center(), size / 2.0 * grow, colors().text);
    let icon = if playing { Icon::Pause } else { Icon::Play };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(size * 0.42)), icon, colors().bg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Rounded button with an optional leading icon. `primary` is the filled style.
pub fn pill(ui: &mut Ui, label: &str, icon: Option<Icon>, primary: bool) -> Response {
    let style = ty::TITLE.strong();
    let galley = ui.painter().layout_no_wrap(label.to_string(), style.font(), Color32::PLACEHOLDER);
    let icon_width = if icon.is_some() { 22.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + icon_width + 36.0, PILL_HEIGHT), Sense::click());
    let p = colors();
    let (bg, fg) = match (primary, resp.hovered()) {
        (true, false) => (p.text, p.bg),
        (true, true) => (p.accent, p.bg),
        (false, false) => (p.surface, p.text),
        (false, true) => (p.raised, p.text),
    };
    ui.painter().rect_filled(rect, CornerRadius::same((PILL_HEIGHT / 2.0) as u8), bg);
    let mut x = rect.left() + 18.0;
    if let Some(i) = icon {
        icons::paint(ui.painter(), Rect::from_center_size(pos2(x + 8.0, rect.center().y), Vec2::splat(16.0)), i, fg);
        x += icon_width;
    }
    ui.painter().galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
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
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), if active { 5.0 } else { 3.0 }));
    let fill = Rect::from_min_max(track.min, pos2(track.left() + track.width() * *value, track.bottom()));
    let painter = ui.painter();
    painter.rect_filled(track, CornerRadius::same(3), p.line);
    painter.rect_filled(fill, CornerRadius::same(3), if active { p.accent } else { p.text });
    if active {
        painter.circle_filled(pos2(fill.right(), track.center().y), 6.0, p.text);
    }
    if enabled {
        resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Segmented control filling the available width; returns the clicked index.
pub fn segmented(ui: &mut Ui, items: &[(Icon, &str)], selected: usize) -> Option<usize> {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(9), colors().bg);
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
            ui.painter().rect_filled(seg, CornerRadius::same(7), colors().raised);
        }
        let color = if on || resp.hovered() { colors().text } else { colors().dim };
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
