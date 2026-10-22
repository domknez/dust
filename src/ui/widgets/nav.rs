//! Sidebar navigation.

use super::text::text_left;
use crate::ui::style::{colors, metrics, radius, typography as ty};
use eframe::egui::{self, CornerRadius, Response, Sense, Ui, pos2, vec2};

/// Sidebar entry: label with an optional shortcut hint on the right; the selected one
/// is drawn inverted.
pub fn sidebar_item(ui: &mut Ui, label: &str, hint: Option<&str>, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::NAV_ROW), Sense::click());
    let p = colors();
    let corner = CornerRadius::same(radius::ROW);
    let (text, hint_color) = if selected {
        ui.painter().rect_filled(rect, corner, p.text);
        (p.bg, p.pressed)
    } else {
        if resp.hovered() {
            ui.painter().rect_filled(rect, corner, p.hover);
        }
        (if resp.hovered() { p.text } else { p.dim }, p.faint)
    };
    let style = if selected { ty::ITEM.strong() } else { ty::ITEM };
    text_left(ui.painter(), pos2(rect.left() + 10.0, rect.center().y), label, style, text, rect.width() - 60.0);
    if let Some(hint) = hint {
        ui.painter().text(pos2(rect.right() - 10.0, rect.center().y), egui::Align2::RIGHT_CENTER, hint, ty::META.font(), hint_color);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
