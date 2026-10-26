//! A page of sections besides Home: Explore, New releases, a genre or mood, or all
//! of one section ("see all"). Rows of cards as on Home; grids for pages of genres
//! and for a section on its own.

use super::home::{card_row, has_card, item_card, section_title};
use crate::model::{Item, Section};
use crate::player::Status;
use crate::ui::action::Actions;
use crate::ui::app::App;
use crate::ui::covers::Covers;
use crate::ui::state::ChannelView;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets::{self, Grid, GridColumns, text_left};
use eframe::egui::{self, Sense, Ui, pos2, vec2};

/// Home-sized cards, as many as fit across.
const CARDS: Grid = Grid { columns: GridColumns::Fit(metrics::HOME_CARD), gap: 16.0, row_gap: 20.0, text: metrics::CARD_TEXT };

impl App {
    pub(in crate::ui) fn channel_view(&mut self, ui: &mut Ui, st: &Status, channel: &ChannelView, acts: &mut Actions) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let page = &self.browse.channel;
            let title = match page.ready() {
                Some(page) if channel.title.is_empty() => page.title.as_str(),
                _ => channel.title.as_str(),
            };
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
            text_left(ui.painter(), pos2(r.left(), r.center().y), title, ty::PAGE_TITLE, colors().text, r.width());
            let Some(page) = page.ready().filter(|page| !page.sections.is_empty()) else {
                ui.add_space(24.0);
                widgets::load_state(ui, page.is_loading(), page.error(), Some("Nothing here right now."));
                return;
            };
            // A "see all" page is one section: all of it, as a grid.
            let alone = page.sections.len() == 1;
            for (i, section) in page.sections.iter().enumerate() {
                if section.grid || alone {
                    card_grid(ui, &mut self.covers, section, acts);
                } else {
                    card_row(ui, &mut self.covers, ("channel-row", i), section, st, acts);
                }
            }
            ui.add_space(24.0);
        });
    }
}

/// A section as its title over a wrapping grid of cards.
fn card_grid(ui: &mut Ui, covers: &mut Covers, section: &Section, acts: &mut Actions) {
    let items: Vec<&Item> = section.items.iter().filter(|it| has_card(it)).collect();
    if items.is_empty() {
        return;
    }
    ui.add_space(26.0);
    section_title(ui, section, acts);
    ui.add_space(10.0);
    CARDS.add(ui, items.len(), |ui, k| item_card(ui, covers, items[k], acts));
}
