//! The Artists, Albums and Podcasts pages: grids of the artists and podcasts the
//! account follows and the albums it likes.

use super::home::CardPicks;
use crate::deezer::Item;
use crate::ui::app::App;
use crate::ui::covers::Covers;
use crate::ui::load::Load;
use crate::ui::state::Coll;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets::{self, CardClick, text_left};
use eframe::egui::{self, RichText, Ui, vec2};

const GAP: f32 = 18.0;

impl App {
    pub(in crate::ui) fn artists_page(&mut self, ui: &mut Ui) {
        let mut picked = CardPicks::default();
        card_grid(ui, &mut self.covers, "Artists", &self.library.followed, "Artists you follow show up here.", &mut picked);
        let ctx = ui.ctx().clone();
        self.apply_card_picks(&ctx, picked);
    }

    pub(in crate::ui) fn albums_page(&mut self, ui: &mut Ui) {
        let mut picked = CardPicks::default();
        card_grid(ui, &mut self.covers, "Albums", &self.library.favorite_albums, "Albums you like show up here.", &mut picked);
        let ctx = ui.ctx().clone();
        self.apply_card_picks(&ctx, picked);
    }

    pub(in crate::ui) fn podcasts_page(&mut self, ui: &mut Ui) {
        let empty = "Podcasts you follow on Deezer show up here. Find more with Search.";
        let mut picked = CardPicks::default();
        card_grid(ui, &mut self.covers, "Podcasts", &self.library.favorite_shows, empty, &mut picked);
        let ctx = ui.ctx().clone();
        self.apply_card_picks(&ctx, picked);
    }
}

/// Page title over a wrapping grid of cards (open on click, play button, queue menu);
/// `empty` says what would be here when there's nothing.
fn card_grid(ui: &mut Ui, covers: &mut Covers, title: &str, items: &Load<Vec<Item>>, empty: &str, picked: &mut CardPicks) {
    let p = colors();
    text_left(ui.painter(), ui.cursor().min + vec2(0.0, 18.0), title, ty::PAGE_TITLE, p.text, 400.0);
    ui.add_space(52.0);
    let Some(items) = items.ready().filter(|items| !items.is_empty()) else {
        if items.is_loading() {
            ui.add(egui::Spinner::new().size(22.0));
        } else if let Some(e) = items.error() {
            ui.label(RichText::new(e).color(p.danger));
        } else {
            ui.label(RichText::new(empty).color(p.dim));
        }
        return;
    };
    let card = metrics::HOME_CARD;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let per_row = (((ui.available_width() + GAP) / (card + GAP)).floor() as usize).max(1);
        for row in items.chunks(per_row) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for coll in row.iter().filter_map(Coll::from_item) {
                    match widgets::card(ui, covers, &coll) {
                        CardClick::Open => picked.open(coll),
                        CardClick::Play(mode) => picked.play(coll.source.clone(), mode),
                        CardClick::None => {}
                    }
                }
            });
            ui.add_space(14.0);
        }
    });
}
