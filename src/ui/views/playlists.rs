//! The Playlists page: a grid of the account's playlists.

use crate::model::Playlist;
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::covers::Covers;
use crate::ui::format::playlist_url;
use crate::ui::state::{Coll, Source};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, Grid, GridColumns, text_left};
use eframe::egui::{self, CornerRadius, Rect, Sense, Ui, Vec2, pos2, vec2};

/// Covers as many as fit across, with the title and track count under each.
const PLAYLISTS: Grid = Grid { columns: GridColumns::Fit(metrics::GRID_CARD), gap: 22.0, row_gap: 24.0, text: 54.0 };

impl App {
    pub(in crate::ui) fn playlists_page(&mut self, ui: &mut Ui, acts: &mut Actions) {
        text_left(ui.painter(), ui.cursor().min + vec2(0.0, 18.0), "Playlists", ty::PAGE_TITLE, colors().text, 400.0);
        ui.add_space(52.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let playlists = &self.library.playlists;
            PLAYLISTS.add(ui, playlists.len(), |ui, k| playlist_card(ui, &mut self.covers, &playlists[k], acts));
        });
    }
}

/// One playlist: cover (a play button on hover), title and track count; opens on
/// click, queues from its right-click menu.
fn playlist_card(ui: &mut Ui, covers: &mut Covers, playlist: &Playlist, acts: &mut Actions) {
    let p = colors();
    let card = metrics::GRID_CARD;
    let (rect, resp) = ui.allocate_exact_size(vec2(card, card + 54.0), Sense::click());
    let art = Rect::from_min_size(rect.min, Vec2::splat(card));
    widgets::cover(ui, covers, playlist_url(playlist, metrics::GRID_CARD_PX).as_deref(), art, radius::CARD);
    if resp.hovered() {
        ui.painter().rect_filled(art, CornerRadius::same(radius::CARD), p.veil);
        widgets::play_knob(ui, pos2(art.right() - 30.0, art.bottom() - 30.0), 20.0);
    }
    text_left(ui.painter(), pos2(rect.left(), art.bottom() + 16.0), &playlist.title, ty::TITLE.strong(), p.text, card);
    text_left(ui.painter(), pos2(rect.left(), art.bottom() + 36.0), &format!("{} tracks", playlist.count), ty::SECONDARY, p.dim, card);
    if let Some(mode) = widgets::queue_menu(&resp) {
        acts.push(Action::PlaySource(Source::Playlist(playlist.id), mode));
    }
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        acts.push(Action::OpenCollection(Coll::from_playlist(playlist)));
    }
}
