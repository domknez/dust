//! What a page shows while its content isn't there.

use crate::ui::style::colors;
use eframe::egui::{self, RichText, Ui};

/// A spinner while the page loads, why it didn't, or `empty` (if given) when it
/// loaded with nothing to show.
pub fn load_state(ui: &mut Ui, loading: bool, error: Option<&str>, empty: Option<&str>) {
    let p = colors();
    if loading {
        ui.add(egui::Spinner::new().size(22.0));
    } else if let Some(e) = error {
        ui.label(RichText::new(e).color(p.danger));
    } else if let Some(empty) = empty {
        ui.label(RichText::new(empty).color(p.dim));
    }
}
