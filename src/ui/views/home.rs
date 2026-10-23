//! Home: a greeting, the Flow panel with its moods, then Deezer's recommendation
//! sections as horizontally scrolling rows of cards.

use crate::deezer::{Item, Section};
use crate::player::{Cmd, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::covers::Covers;
use crate::ui::state::Coll;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets::{self, CardClick, text_left};
use eframe::egui::{self, Sense, Ui, pos2, vec2};

/// Height [`card_row`] takes: spacing, section title and one row of cards.
pub(in crate::ui) const CARD_ROW_HEIGHT: f32 = 26.0 + 20.0 + 10.0 + metrics::HOME_CARD + metrics::CARD_TEXT;

impl App {
    pub(in crate::ui) fn home_page(&mut self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.home_header(ui) {
                acts.push(Action::ReloadHome);
            }
            let home = &self.library.home;
            let Some(sections) = home.ready().filter(|h| !h.is_empty()) else {
                ui.add_space(24.0);
                widgets::load_state(ui, home.is_loading(), home.error(), None);
                return;
            };
            for i in flow_first(sections) {
                let section = &sections[i];
                card_row(ui, &mut self.covers, ("home-row", i), &section.title, &section.items, st, acts);
            }
            ui.add_space(24.0);
        });
    }

    /// Mono line with a refresh button, then a greeting. Returns whether refresh was
    /// clicked.
    fn home_header(&self, ui: &mut Ui) -> bool {
        let p = colors();
        let mut reload = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let (r, _) = ui.allocate_exact_size(vec2(196.0, 22.0), Sense::hover());
            let line = if self.features().flow { "recommended by deezer".to_string() } else { format!("from {}", self.source_host()) };
            text_left(ui.painter(), pos2(r.left(), r.center().y), &line, ty::META, p.dim, r.width());
            reload = widgets::icon_button(ui, Icon::Refresh, 12.0, p.dim).on_hover_text("Refresh").clicked();
        });
        let name = self.account_name();
        let first = name.split_whitespace().next().unwrap_or_default();
        let greeting = if first.is_empty() { "Hello".to_string() } else { format!("Hello, {first}") };
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
        text_left(ui.painter(), pos2(r.left(), r.center().y), &greeting, ty::PAGE_TITLE, p.text, r.width());
        reload
    }
}

/// A titled, horizontally scrolling row of cards; a section of Flow moods becomes
/// the Flow panel. Draws nothing when none of `items` can be shown.
pub(in crate::ui) fn card_row(
    ui: &mut Ui,
    covers: &mut Covers,
    id: impl std::hash::Hash + std::fmt::Debug,
    title: &str,
    items: &[Item],
    st: &Status,
    acts: &mut Actions,
) {
    // Deezer's Flow, or a music server's Radio (random songs by genre) in its place.
    let is_radio = items.iter().all(|it| it.kind == "radio");
    let is_flow = is_radio || items.iter().all(|it| it.kind == "flow");
    let items: Vec<&Item> = items.iter().filter(|it| is_flow || Coll::from_item(it).is_some()).collect();
    if items.is_empty() {
        return;
    }
    if is_flow {
        ui.add_space(18.0);
        let moods: Vec<(&str, &str)> = items.iter().filter(|it| it.id != "default").map(|it| (it.id.as_str(), it.title.as_str())).collect();
        if let Some(mood) = widgets::flow_panel(ui, &moods, st.flow.as_deref(), is_radio) {
            acts.push(Action::Player(Cmd::PlayFlow(mood)));
        }
        return;
    }
    ui.add_space(26.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    text_left(ui.painter(), pos2(r.left(), r.center().y), &title.to_uppercase(), ty::OVERLINE, colors().dim, r.width());
    ui.add_space(10.0);
    egui::ScrollArea::horizontal()
        .id_salt(id)
        .auto_shrink([false, true])
        // Bar visibility animation on these rows never settled and kept the UI
        // redrawing while idle; trackpad and shift-scroll still work.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for item in items {
                    if let Some(coll) = Coll::from_item(item) {
                        match widgets::card(ui, covers, &coll) {
                            CardClick::Open => acts.push(Action::OpenCollection(coll)),
                            CardClick::Play(mode) => acts.push(Action::PlaySource(coll.source.clone(), mode)),
                            CardClick::None => {}
                        }
                    }
                }
            });
        });
}

/// Section order with the Flow section first, as on Deezer.
fn flow_first(sections: &[Section]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..sections.len()).collect();
    order.sort_by_key(|&i| !sections[i].items.iter().any(|it| it.kind == "flow" || it.kind == "radio"));
    order
}
