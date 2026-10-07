//! The main area: search field on top, then the current page.

use crate::player::Status;
use crate::ui::app::App;
use crate::ui::state::View;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::widgets;
use eframe::egui::{self, CornerRadius, Key, KeyboardShortcut, Modifiers, Rect, RichText, Sense, Ui, Vec2, pos2, vec2};

impl App {
    pub(in crate::ui) fn content(&mut self, ui: &mut Ui, st: &Status) {
        ui.add_space(metrics::TOP_INSET);
        ui.horizontal(|ui| {
            self.history_buttons(ui);
            ui.add_space(6.0);
            self.search_field(ui);
            if let Some(e) = &st.error {
                ui.add_space(12.0);
                ui.label(RichText::new(e).color(colors().danger).font(ty::SECONDARY.font()));
            }
        });
        ui.add_space(20.0);
        match &self.view {
            View::Home => self.home_page(ui, st),
            View::Playlists => self.playlists_page(ui),
            View::Search | View::Loved | View::Collection(_) => self.collection_page(ui, st),
            View::Artists => self.artists_page(ui),
            View::Albums => self.albums_page(ui),
            View::Artist(artist) => {
                let artist = artist.as_ref().clone();
                self.artist_view(ui, st, &artist);
            }
        }
    }

    /// ‹ › next to the search field; dimmed when there's nowhere to go.
    fn history_buttons(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let p = colors();
        let shortcut = if cfg!(target_os = "macos") { ("⌘←", "⌘→") } else { ("Alt+←", "Alt+→") };
        ui.spacing_mut().item_spacing.x = 2.0;
        for (icon, enabled, hint, back) in [
            (Icon::ChevronLeft, self.history.can_go_back(), format!("Back ({})", shortcut.0), true),
            (Icon::ChevronRight, self.history.can_go_forward(), format!("Forward ({})", shortcut.1), false),
        ] {
            let color = if enabled { p.dim } else { p.faint.gamma_multiply(0.6) };
            let resp = widgets::icon_button(ui, icon, 16.0, color);
            if enabled && resp.on_hover_text(hint).clicked() {
                if back {
                    self.go_back(&ctx);
                } else {
                    self.go_forward(&ctx);
                }
            }
        }
    }

    fn search_field(&mut self, ui: &mut Ui) {
        let p = colors();
        let [width, height] = metrics::SEARCH_FIELD;
        let id = ui.id().with("search-field");
        let (rect, pill) = ui.allocate_exact_size(vec2(width.min(ui.available_width()), height), Sense::click());
        // Clicking anywhere on the pill (icon, padding) puts the cursor in the field.
        if pill.clicked() {
            ui.memory_mut(|m| m.request_focus(id));
        }
        let focused = ui.memory(|m| m.has_focus(id));
        let radius = CornerRadius::same((height / 2.0) as u8);
        ui.painter().rect_filled(rect, radius, if focused { p.raised } else { p.surface });
        // Accent ring while typing, a faint one on hover.
        let ring = match (focused, pill.hovered()) {
            (true, _) => Some(egui::Stroke::new(1.5, p.accent)),
            (false, true) => Some(egui::Stroke::new(1.0, p.line)),
            (false, false) => None,
        };
        if let Some(stroke) = ring {
            ui.painter().rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
        }
        let icon = Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(15.0));
        icons::paint(ui.painter(), icon, Icon::Search, if focused { p.accent } else { p.dim });
        let input = Rect::from_min_max(pos2(rect.left() + 38.0, rect.top() + 9.0), pos2(rect.right() - 14.0, rect.bottom() - 7.0));
        let field = egui::TextEdit::singleline(&mut self.search)
            .id(id)
            .hint_text("Search, or paste a Deezer link")
            .frame(egui::Frame::new())
            .text_color(p.text);
        let edit = ui.put(input, field);
        if pill.hovered() || edit.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        if ui.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::F))) {
            edit.request_focus();
        }
        if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            let ctx = ui.ctx().clone();
            self.open_view(&ctx, View::Search);
        }
    }
}
