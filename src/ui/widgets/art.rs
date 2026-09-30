//! Artwork: covers from the cache, or placeholder tiles.

use crate::ui::covers::Covers;
use crate::ui::style::colors;
use crate::ui::style::icons::{self, Icon};
use eframe::egui::{self, Color32, CornerRadius, Rect, Ui};

/// Rounded cover image, or a subtle placeholder while it loads.
pub fn cover(ui: &Ui, covers: &mut Covers, url: Option<&str>, rect: Rect, radius: u8) {
    // Off-screen artwork is neither fetched nor kept alive, so the cache can evict it.
    if !ui.is_rect_visible(rect) {
        return;
    }
    match url.and_then(|u| covers.get(u)) {
        Some(texture) => egui::Image::from_texture((texture, rect.size())).corner_radius(radius).paint_at(ui, rect),
        None => {
            ui.painter().rect_filled(rect, CornerRadius::same(radius), colors().surface);
            icons::paint(ui.painter(), Rect::from_center_size(rect.center(), rect.size() * 0.36), Icon::Note, colors().faint);
        }
    }
}

/// A rounded colour tile with an icon, for collections without artwork.
pub fn tile(ui: &Ui, rect: Rect, icon: Icon, color: Color32, radius: u8) {
    ui.painter().rect_filled(rect, CornerRadius::same(radius), color);
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), rect.size() * 0.34), icon, Color32::from_white_alpha(235));
}
