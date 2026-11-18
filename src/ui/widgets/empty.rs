//! What a list says while there's nothing in it: what's missing, and a hint at
//! how to fill it.

use crate::ui::style::{colors, typography as ty};
use eframe::egui::{RichText, Ui};

/// Room above a side panel's empty state.
const PANEL_TOP: f32 = 40.0;

/// `title` and `hint`, from the left (on a page).
pub fn empty_state(ui: &mut Ui, title: &str, hint: &str) {
    let p = colors();
    ui.label(RichText::new(title).color(p.dim).font(ty::TITLE.font()));
    ui.add_space(4.0);
    ui.label(RichText::new(hint).color(p.faint).font(ty::SMALL.font()));
}

/// `title` and `hint`, centred a little way down (in a side panel).
pub fn empty_panel(ui: &mut Ui, title: &str, hint: &str) {
    ui.add_space(PANEL_TOP);
    ui.vertical_centered(|ui| empty_state(ui, title, hint));
}
