//! Popover menus and their rows.

use super::text::text_left;
use crate::ui::state::PlayMode;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, radius, typography as ty};
use eframe::egui::{self, Color32, CornerRadius, Response, Sense, Ui, Vec2, pos2, vec2};

const CONTEXT_MENU_WIDTH: f32 = 190.0;

/// Frame for our popovers.
pub fn popover_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::new()
        .fill(colors().surface)
        .stroke(egui::Stroke::new(1.0, colors().line))
        .corner_radius(CornerRadius::same(radius::POPOVER))
        .inner_margin(egui::Margin::same(10))
        .shadow(ctx.global_style().visuals.popup_shadow)
}

/// Menu entry: optional icon, title, optional subtitle, check mark when selected.
pub fn menu_row(ui: &mut Ui, icon: Option<Icon>, title: &str, subtitle: Option<&str>, checked: bool, color: Color32) -> Response {
    let height = if subtitle.is_some() { 46.0 } else { 36.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(radius::CONTROL - 1), colors().raised);
    }
    let mut x = rect.left() + 10.0;
    if let Some(i) = icon {
        icons::paint(ui.painter(), egui::Rect::from_center_size(pos2(x + 8.0, rect.center().y), Vec2::splat(16.0)), i, color);
        x += 28.0;
    }
    let width = rect.right() - x - 34.0;
    let painter = ui.painter();
    match subtitle {
        Some(sub) => {
            let style = if checked { ty::ITEM.strong() } else { ty::ITEM };
            text_left(painter, pos2(x, rect.center().y - 8.0), title, style, color, width);
            text_left(painter, pos2(x, rect.center().y + 10.0), sub, ty::CAPTION, colors().faint, width);
        }
        None => {
            text_left(painter, pos2(x, rect.center().y), title, ty::ITEM, color, width);
        }
    }
    if checked {
        icons::paint(painter, egui::Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(14.0)), Icon::Check, colors().accent);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Uppercase caption inside menus and panels.
pub fn caption(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    text_left(ui.painter(), pos2(rect.left() + 4.0, rect.center().y), text, ty::OVERLINE, colors().faint, rect.width());
}

/// Hairline divider with vertical breathing room.
pub fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 13.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, egui::Stroke::new(1.0, colors().line));
}

/// Right-click menu offering Play next / Add to queue.
pub fn queue_menu(resp: &Response) -> Option<PlayMode> {
    let mut pick = None;
    resp.context_menu(|ui| {
        // Menu rows fill the available width; cap it or the menu spans the window.
        ui.set_min_width(CONTEXT_MENU_WIDTH);
        ui.set_max_width(CONTEXT_MENU_WIDTH);
        if menu_row(ui, Some(Icon::Play), "Play next", None, false, colors().text).clicked() {
            pick = Some(PlayMode::Next);
            ui.close();
        }
        if menu_row(ui, Some(Icon::Queue), "Add to queue", None, false, colors().text).clicked() {
            pick = Some(PlayMode::Queue);
            ui.close();
        }
    });
    pick
}
