//! Sidebar navigation.

use super::text::text_left;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use eframe::egui::{self, CornerRadius, Rect, Response, Sense, Ui, Vec2, pos2, vec2};

/// Navigation entry; the selected one gets a surface, a blue icon and an indicator bar.
pub fn nav_item(ui: &mut Ui, icon: Icon, label: &str, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::NAV_ROW), Sense::click());
    let p = colors();
    if selected {
        ui.painter().rect_filled(rect, CornerRadius::same(radius::CONTROL), p.surface);
        let bar = Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, 16.0));
        ui.painter().rect_filled(bar, CornerRadius::same(2), p.accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(radius::CONTROL), p.hover);
    }
    let color = if selected || resp.hovered() { p.text } else { p.dim };
    let icon_color = if selected { p.accent } else { color };
    icons::paint(ui.painter(), Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(17.0)), icon, icon_color);
    let style = if selected { ty::TITLE.strong() } else { ty::TITLE };
    text_left(ui.painter(), pos2(rect.left() + 40.0, rect.center().y), label, style, color, rect.width() - 48.0);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Uppercase heading between sidebar groups.
pub fn section(ui: &mut Ui, label: &str) {
    ui.add_space(18.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    text_left(ui.painter(), pos2(rect.left() + 10.0, rect.center().y), label, ty::OVERLINE, colors().faint, rect.width());
    ui.add_space(4.0);
}
