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
const SEARCH_KEYS: &str = if cfg!(target_os = "macos") { "⌘F" } else { "Ctrl+F" };
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
        ui.spacing_mut().item_spacing.y = 1.0;
        if widgets::sidebar_item(ui, "Home", None, self.view == View::Home).clicked() {
            go = Some(View::Home);
        }
        if widgets::sidebar_item(ui, "Search", Some(SEARCH_KEYS), self.view == View::Search).clicked() {
            // Search opens empty; the query field is in the main area.
            let current = self.current_visit();
            self.history.leave(current, &crate::ui::history::Visit { view: View::Search, query: String::new() });
            self.view = View::Search;
            self.tracks.clear();
            self.searched.clear();
        }
        for (label, view) in [
            ("Loved tracks", View::Loved),
            ("Playlists", View::Playlists),
            ("Albums", View::Albums),
            ("Artists", View::Artists),
            ("Podcasts", View::Podcasts),
        ] {
            let selected = self.view == view;
            if widgets::sidebar_item(ui, label, None, selected).clicked() {
                go = Some(view);
            }
        }
        ui.spacing_mut().item_spacing.y = 6.0;
        // "PLAYLISTS / n" with a + for a new, empty playlist.
        ui.add_space(18.0);
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
        let caption = format!("PLAYLISTS / {}", self.playlists.len());
        text_left(ui.painter(), pos2(row.left() + 10.0, row.center().y), &caption, ty::OVERLINE, colors().dim, row.width() - 40.0);
        let plus = Rect::from_center_size(pos2(row.right() - 12.0, row.center().y), Vec2::splat(22.0));
        let resp = ui.interact(plus, ui.id().with("new-playlist"), Sense::click());
        let color = if resp.hovered() { colors().text } else { colors().faint };
        icons::paint(ui.painter(), Rect::from_center_size(plus.center(), Vec2::splat(12.0)), Icon::Plus, color);
        if resp.on_hover_text("New playlist").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.new_playlist(Vec::new());
        }
        ui.add_space(4.0);
        if let Some(view) = go {
            self.open_view(&ctx, view);
        }
    }

    fn playlist_list(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let p = colors();
        let mut open = None;
        let mut queue = None;
        let height = (ui.available_height() - FOOTER - self.update_button_height()).max(0.0);
        egui::ScrollArea::vertical().max_height(height).auto_shrink([false, false]).show(ui, |ui| {
            if self.playlists_task.is_some() {
                ui.spinner();
            }
            ui.spacing_mut().item_spacing.y = 1.0;
            for playlist in &self.playlists {
                let selected = matches!(&self.view, View::Collection(c) if c.source == Source::Playlist(playlist.id));
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::SIDEBAR_PLAYLIST_ROW), Sense::click());
                if selected || resp.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), if selected { p.raised } else { p.hover });
                }
                let art_size = metrics::SIDEBAR_ART;
                let art = Rect::from_min_size(pos2(rect.left() + 10.0, rect.center().y - art_size / 2.0), Vec2::splat(art_size));
                widgets::cover(ui, &mut self.covers, playlist_url(playlist, metrics::THUMB_PX).as_deref(), art, 3);
                let count = playlist.count.to_string();
                let count_width = ui.painter().layout_no_wrap(count.clone(), ty::META.font(), p.faint).size().x;
                ui.painter().text(pos2(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, count, ty::META.font(), p.faint);
                let x = art.right() + 10.0;
                let width = rect.right() - x - count_width - 18.0;
                let title_color = if selected || resp.hovered() { p.text } else { p.dim };
                text_left(ui.painter(), pos2(x, rect.center().y), &playlist.title, ty::BODY, title_color, width);
                if let Some(mode) = widgets::queue_menu(&resp) {
                    queue = Some((Source::Playlist(playlist.id), mode));
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    open = Some(Coll::from_playlist(playlist));
                }
            }
        });
        if let Some(coll) = open {
            self.open_collection(&ctx, coll);
        }
        if let Some((source, mode)) = queue {
            self.play_source(&ctx, source, mode);
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
        let corner = CornerRadius::same(radius::CONTROL);
        if resp.hovered() {
            ui.painter().rect_filled(rect, corner, p.hover);
        }
        ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        let chevron = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(12.0));
        icons::paint(ui.painter(), chevron, Icon::Chevrons, p.faint);
        let name = self.account_name();
        avatar(ui, pos2(rect.left() + 24.0, rect.center().y), 14.0, &name, ty::META);
        let width = rect.width() - 80.0;
        text_left(ui.painter(), pos2(rect.left() + 48.0, rect.center().y - 8.0), &name, ty::BODY.strong(), p.text, width);
        let quality = self.quality.label().to_lowercase();
        text_left(ui.painter(), pos2(rect.left() + 48.0, rect.center().y + 9.0), &quality, ty::META, p.faint, width);
        self.account_menu(&resp, &name);
    }

    pub(in crate::ui) fn account_name(&self) -> String {
        self.client.as_ref().map(|c| c.name().to_string()).unwrap_or_default()
    }
}

/// `dust_` in mono, the underscore in lime like a cursor.
fn wordmark(ui: &mut Ui) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    let word = text_left(ui.painter(), pos2(row.left() + 10.0, row.center().y), "dust", ty::WORDMARK, colors().text, 160.0);
    text_left(ui.painter(), pos2(word.right(), row.center().y), "_", ty::WORDMARK, colors().accent, 20.0);
}

/// Square lime tile with the name's initials.
pub(super) fn avatar(ui: &Ui, center: egui::Pos2, half: f32, name: &str, style: ty::Text) {
    let p = colors();
    let tile = Rect::from_center_size(center, Vec2::splat(half * 2.0));
    ui.painter().rect_filled(tile, CornerRadius::same(radius::ROW), p.accent_fill);
    ui.painter().text(center, Align2::CENTER_CENTER, initials(name), style.strong().font(), p.on_accent);
}

/// Up to two initials: "Domagoj Knez" gives "DK", "domagoj" gives "D".
fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).take(2).flat_map(char::to_uppercase).collect()
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn initials_of_names() {
        assert_eq!(initials("Domagoj Knez"), "DK");
        assert_eq!(initials("domagoj"), "D");
        assert_eq!(initials("a b c"), "AB");
        assert_eq!(initials(""), "");
    }
}
