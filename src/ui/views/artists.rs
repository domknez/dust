//! The Artists, Albums and Podcasts pages: grids of the artists and podcasts the
//! account follows and the albums it likes.

use crate::deezer::Item;
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::covers::Covers;
use crate::ui::load::Load;
use crate::ui::state::Coll;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets::{self, CardClick, Grid, GridColumns, text_left};
use eframe::egui::{self, Ui, vec2};

/// Home-sized cards, as many as fit across.
const CARDS: Grid = Grid { columns: GridColumns::Fit(metrics::HOME_CARD), gap: 18.0, row_gap: 20.0, text: metrics::CARD_TEXT };

impl App {
    pub(in crate::ui) fn artists_page(&mut self, ui: &mut Ui, acts: &mut Actions) {
        card_grid(ui, &mut self.covers, "Artists", &self.library.followed, "Artists you follow show up here.", acts);
    }

    pub(in crate::ui) fn albums_page(&mut self, ui: &mut Ui, acts: &mut Actions) {
        card_grid(ui, &mut self.covers, "Albums", &self.library.favorite_albums, "Albums you like show up here.", acts);
    }

    pub(in crate::ui) fn podcasts_page(&mut self, ui: &mut Ui, acts: &mut Actions) {
        let empty = "Podcasts you follow on Deezer show up here. Find more with Search.";
        card_grid(ui, &mut self.covers, "Podcasts", &self.library.favorite_shows, empty, acts);
    }
}

/// Page title over a wrapping grid of cards (open on click, play button, queue menu);
/// `empty` says what would be here when there's nothing.
fn card_grid(ui: &mut Ui, covers: &mut Covers, title: &str, items: &Load<Vec<Item>>, empty: &str, acts: &mut Actions) {
    let p = colors();
    text_left(ui.painter(), ui.cursor().min + vec2(0.0, 18.0), title, ty::PAGE_TITLE, p.text, 400.0);
    ui.add_space(52.0);
    let Some(items) = items.ready().filter(|items| !items.is_empty()) else {
        widgets::load_state(ui, items.is_loading(), items.error(), Some(empty));
        return;
    };
    let colls: Vec<Coll> = items.iter().filter_map(Coll::from_item).collect();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        CARDS.add(ui, colls.len(), |ui, k| match widgets::card(ui, covers, &colls[k]) {
            CardClick::Open => acts.push(Action::OpenCollection(colls[k].clone())),
            CardClick::Play(mode) => acts.push(Action::PlaySource(colls[k].source.clone(), mode)),
            CardClick::None => {}
        });
    });
}
