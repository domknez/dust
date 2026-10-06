//! The Artists page: a grid of the artists the account follows.

use super::home::CardPicks;
use crate::player::Status;
use crate::ui::app::App;
use crate::ui::state::Coll;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets::{self, CardClick, text_left};
use eframe::egui::{self, RichText, Ui, vec2};

const GAP: f32 = 18.0;

impl App {
    pub(in crate::ui) fn artists_page(&mut self, ui: &mut Ui, _st: &Status) {
        let ctx = ui.ctx().clone();
        let p = colors();
        text_left(ui.painter(), ui.cursor().min + vec2(0.0, 18.0), "Artists", ty::PAGE_TITLE, p.text, 400.0);
        ui.add_space(52.0);
        if self.followed.is_empty() {
            if self.followed_task.is_some() {
                ui.add(egui::Spinner::new().size(22.0));
            } else if let Some(e) = &self.list_error {
                ui.label(RichText::new(e).color(p.danger));
            } else {
                ui.label(RichText::new("Artists you follow on Deezer show up here.").color(p.dim));
            }
            return;
        }
        let mut picked = CardPicks::default();
        let card = metrics::HOME_CARD;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let per_row = (((ui.available_width() + GAP) / (card + GAP)).floor() as usize).max(1);
            for row in self.followed.chunks(per_row) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    for coll in row.iter().filter_map(Coll::from_item) {
                        match widgets::card(ui, &mut self.covers, &coll) {
                            CardClick::Open => picked.open(coll),
                            CardClick::Play(mode) => picked.play(coll.source.clone(), mode),
                            CardClick::None => {}
                        }
                    }
                });
                ui.add_space(14.0);
            }
        });
        self.apply_card_picks(&ctx, picked);
    }
}
