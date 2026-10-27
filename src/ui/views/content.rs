//! The main area: search field on top, then the current page.

use crate::player::Status;
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::state::View;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets;
use eframe::egui::{self, CornerRadius, Key, KeyboardShortcut, Modifiers, Rect, Sense, Ui, Vec2, pos2, vec2};

impl App {
    pub(in crate::ui) fn content(&mut self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        ui.add_space(metrics::TOP_INSET);
        ui.horizontal(|ui| {
            self.history_buttons(ui, acts);
            ui.add_space(6.0);
            self.search_field(ui, acts);
            // A playlist change on its way; what came of it is a toast.
            if self.library.editing.busy() {
                ui.add_space(12.0);
                ui.add(egui::Spinner::new().size(14.0));
            }
        });
        ui.add_space(20.0);
        match &self.browse.view {
            View::Home => self.home_page(ui, st, acts),
            View::Playlists => self.playlists_page(ui, acts),
            View::Search | View::Loved | View::Collection(_) => self.collection_page(ui, st, acts),
            View::Artists => self.artists_page(ui, acts),
            View::Albums => self.albums_page(ui, acts),
            View::Podcasts => self.podcasts_page(ui, acts),
            View::Artist(artist) => {
                let artist = artist.as_ref().clone();
                self.artist_view(ui, st, &artist, acts);
            }
            View::Channel(channel) => {
                let channel = channel.as_ref().clone();
                self.channel_view(ui, st, &channel, acts);
            }
        }
    }

    /// ‹ › next to the search field; dimmed when there's nowhere to go.
    fn history_buttons(&self, ui: &mut Ui, acts: &mut Actions) {
        let p = colors();
        let shortcut = if cfg!(target_os = "macos") { ("⌘←", "⌘→") } else { ("Alt+←", "Alt+→") };
        ui.spacing_mut().item_spacing.x = 2.0;
        for (icon, enabled, hint, back) in [
            (Icon::ChevronLeft, self.browse.history.can_go_back(), format!("Back ({})", shortcut.0), true),
            (Icon::ChevronRight, self.browse.history.can_go_forward(), format!("Forward ({})", shortcut.1), false),
        ] {
            let color = if enabled { p.dim } else { p.faint.gamma_multiply(0.6) };
            let resp = widgets::icon_button(ui, icon, 16.0, color);
            if enabled && resp.on_hover_text(hint).clicked() {
                acts.push(if back { Action::Back } else { Action::Forward });
            }
        }
    }

    fn search_field(&mut self, ui: &mut Ui, acts: &mut Actions) {
        let p = colors();
        let [width, height] = metrics::SEARCH_FIELD;
        let id = ui.scope_id().with("search-field");
        let (rect, pill) = ui.allocate_exact_size(vec2(width.min(ui.available_width()), height), Sense::click());
        // Clicking anywhere on the pill (icon, padding) puts the cursor in the field.
        if pill.clicked() {
            ui.memory_mut(|m| m.request_focus(id));
        }
        let focused = ui.memory(|m| m.has_focus(id));
        let corner = CornerRadius::same(radius::CONTROL);
        ui.painter().rect_filled(rect, corner, p.surface);
        // Lime outline while typing, a brighter one on hover.
        let ring = match (focused, pill.hovered()) {
            (true, _) => egui::Stroke::new(1.5, p.accent),
            (false, true) => egui::Stroke::new(1.0, p.faint),
            (false, false) => egui::Stroke::new(1.0, p.border),
        };
        ui.painter().rect_stroke(rect, corner, ring, egui::StrokeKind::Inside);
        let icon = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), Vec2::splat(14.0));
        icons::paint(ui.painter(), icon, Icon::Search, if focused { p.accent } else { p.dim });
        // The shortcut, as a key cap, while the field is idle.
        let mut right = rect.right() - 12.0;
        if !focused && self.browse.search.is_empty() {
            let keys = if cfg!(target_os = "macos") { "⌘F" } else { "Ctrl+F" };
            let width = ui.painter().layout_no_wrap(keys.to_string(), ty::META.font(), p.dim).size().x + 10.0;
            let cap = Rect::from_center_size(pos2(rect.right() - 10.0 - width / 2.0, rect.center().y), vec2(width, 20.0));
            ui.painter().rect_stroke(cap, CornerRadius::same(radius::THUMB), egui::Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
            ui.painter().text(cap.center(), egui::Align2::CENTER_CENTER, keys, ty::META.font(), p.dim);
            right = cap.left() - 8.0;
        }
        let input = Rect::from_min_max(pos2(rect.left() + 34.0, rect.top() + 9.0), pos2(right, rect.bottom() - 7.0));
        let hint = match &self.client {
            Some(c) if c.features().links => format!("Search, or paste a {} link", c.id().name),
            _ => "Search your library".to_string(),
        };
        let field = egui::TextEdit::singleline(&mut self.browse.search).id(id).hint_text(hint).frame(egui::Frame::new()).text_color(p.text);
        let edit = ui.put(input, field);
        if pill.hovered() || edit.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        if ui.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::F))) {
            edit.request_focus();
        }
        if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            acts.push(Action::Open(View::Search));
        }
    }
}
