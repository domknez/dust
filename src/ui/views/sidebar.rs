//! Sidebar: wordmark, navigation, the playlist list and the account card.

use crate::ui::app::App;
use crate::ui::format::playlist_url;
use crate::ui::state::{Coll, Source, View};
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, Align2, CornerRadius, Rect, Sense, Ui, Vec2, pos2, vec2};

/// Height kept free below the playlist list for the account card.
const FOOTER: f32 = 56.0;
/// Height of the fade between the playlist list and the account card.
const FADE: f32 = 28.0;

impl App {
    pub(in crate::ui) fn sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(metrics::TOP_INSET);
        wordmark(ui);
        ui.add_space(18.0);
        self.navigation(ui);
        self.playlist_list(ui);
        self.update_button(ui);
        self.account_card(ui);
    }

    fn navigation(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut go = None;
        if widgets::nav_item(ui, Icon::Home, "Home", self.view == View::Home).clicked() {
            go = Some(View::Home);
        }
        if widgets::nav_item(ui, Icon::Search, "Search", self.view == View::Search).clicked() {
            // Search opens empty; the query field is in the main area.
            self.view = View::Search;
            self.tracks.clear();
            self.searched.clear();
        }
        widgets::section(ui, "YOUR COLLECTION");
        if widgets::nav_item(ui, Icon::Heart, "Tracks", self.view == View::Loved).clicked() {
            go = Some(View::Loved);
        }
        if widgets::nav_item(ui, Icon::Grid, "Playlists", self.view == View::Playlists).clicked() {
            go = Some(View::Playlists);
        }
        if widgets::nav_item(ui, Icon::Disc, "Albums", self.view == View::Albums).clicked() {
            go = Some(View::Albums);
        }
        if widgets::nav_item(ui, Icon::Person, "Artists", self.view == View::Artists).clicked() {
            go = Some(View::Artists);
        }
        widgets::section(ui, "PLAYLISTS");
        if let Some(view) = go {
            self.open_view(&ctx, view);
        }
    }

    fn playlist_list(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let p = colors();
        let mut open = None;
        let height = (ui.available_height() - FOOTER - self.update_button_height()).max(0.0);
        egui::ScrollArea::vertical().max_height(height).auto_shrink([false, false]).show(ui, |ui| {
            if self.playlists_task.is_some() {
                ui.spinner();
            }
            for playlist in &self.playlists {
                let selected = matches!(&self.view, View::Collection(c) if c.source == Source::Playlist(playlist.id));
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::SIDEBAR_PLAYLIST_ROW), Sense::click());
                if selected || resp.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), if selected { p.surface } else { p.hover });
                }
                let art = Rect::from_min_size(rect.min + vec2(6.0, 6.0), Vec2::splat(metrics::LIST_ART));
                widgets::cover(ui, &mut self.covers, playlist_url(playlist, metrics::THUMB_PX).as_deref(), art, radius::THUMB);
                let x = art.right() + 12.0;
                let width = rect.right() - x - 6.0;
                let title_color = if selected { p.text } else { p.dim };
                text_left(ui.painter(), pos2(x, rect.center().y - 8.0), &playlist.title, ty::ITEM, title_color, width);
                text_left(ui.painter(), pos2(x, rect.center().y + 9.0), &format!("{} tracks", playlist.count), ty::CAPTION, p.faint, width);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    open = Some(Coll::from_playlist(playlist));
                }
            }
        });
        if let Some(coll) = open {
            self.open_collection(&ctx, coll);
        }
        // Soft fade where the list meets the account card.
        let edge = ui.cursor().min.y;
        let fade = Rect::from_min_max(pos2(ui.max_rect().left(), edge - FADE), pos2(ui.max_rect().right(), edge));
        widgets::fade_to(ui.painter(), fade, p.sidebar);
    }

    /// Account card; opens the account menu.
    fn account_card(&mut self, ui: &mut Ui) {
        let p = colors();
        ui.add_space(6.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::ACCOUNT_CARD), Sense::click());
        ui.painter().rect_filled(rect, CornerRadius::same(radius::PANEL), if resp.hovered() { p.raised } else { p.surface });
        let chevron = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(12.0));
        icons::paint(ui.painter(), chevron, Icon::Chevrons, p.faint);
        let name = self.account_name();
        avatar(ui, pos2(rect.left() + 24.0, rect.center().y), 16.0, &name, ty::TITLE);
        let width = rect.width() - 80.0;
        text_left(ui.painter(), pos2(rect.left() + 50.0, rect.center().y - 8.0), &name, ty::ITEM.strong(), p.text, width);
        text_left(ui.painter(), pos2(rect.left() + 50.0, rect.center().y + 9.0), self.quality.label(), ty::CAPTION, p.faint, width);
        self.account_menu(&resp, &name);
    }

    pub(in crate::ui) fn account_name(&self) -> String {
        self.client.as_ref().map(|c| c.name().to_string()).unwrap_or_default()
    }
}

fn wordmark(ui: &mut Ui) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
    let word = text_left(ui.painter(), pos2(row.left() + 8.0, row.center().y + 2.0), "dust", ty::WORDMARK, colors().text, 160.0);
    widgets::dust_particles(ui.painter(), pos2(word.right() - 3.0, word.top() + 6.0));
}

/// Round avatar with the name's initial.
pub(super) fn avatar(ui: &Ui, center: egui::Pos2, radius: f32, name: &str, style: ty::Text) {
    let p = colors();
    ui.painter().circle_filled(center, radius, p.accent);
    let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    ui.painter().text(center, Align2::CENTER_CENTER, initial, style.strong().font(), p.bg);
}
