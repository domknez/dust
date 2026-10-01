//! The Playlists page: a grid of the account's playlists.

use crate::ui::app::App;
use crate::ui::format::playlist_url;
use crate::ui::state::Coll;
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, CornerRadius, Rect, Sense, Ui, Vec2, pos2, vec2};

const GAP: f32 = 22.0;

impl App {
    pub(in crate::ui) fn playlists_page(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let p = colors();
        text_left(ui.painter(), ui.cursor().min + vec2(0.0, 18.0), "Playlists", ty::PAGE_TITLE, p.text, 400.0);
        ui.add_space(52.0);
        let mut open = None;
        let card = metrics::GRID_CARD;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let per_row = (((ui.available_width() + GAP) / (card + GAP)).floor() as usize).max(1);
            for row in self.playlists.chunks(per_row) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    for playlist in row {
                        let (rect, resp) = ui.allocate_exact_size(vec2(card, card + 54.0), Sense::click());
                        let art = Rect::from_min_size(rect.min, Vec2::splat(card));
                        widgets::cover(ui, &mut self.covers, playlist_url(playlist, metrics::GRID_CARD_PX).as_deref(), art, radius::CARD);
                        if resp.hovered() {
                            ui.painter().rect_filled(art, CornerRadius::same(radius::CARD), p.veil);
                            widgets::play_knob(ui, pos2(art.right() - 30.0, art.bottom() - 30.0), 20.0);
                        }
                        text_left(ui.painter(), pos2(rect.left(), art.bottom() + 16.0), &playlist.title, ty::TITLE.strong(), p.text, card);
                        text_left(
                            ui.painter(),
                            pos2(rect.left(), art.bottom() + 36.0),
                            &format!("{} tracks", playlist.count),
                            ty::SECONDARY,
                            p.dim,
                            card,
                        );
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            open = Some(Coll::from_playlist(playlist));
                        }
                    }
                });
                ui.add_space(18.0);
            }
        });
        if let Some(coll) = open {
            self.open_collection(&ctx, coll);
        }
    }
}
