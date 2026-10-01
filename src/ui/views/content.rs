//! The main area: search field on top, then the current page.

use crate::player::Status;
use crate::ui::app::App;
use crate::ui::state::View;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, typography as ty};
use eframe::egui::{self, CornerRadius, Key, KeyboardShortcut, Modifiers, Rect, RichText, Sense, Ui, Vec2, pos2, vec2};

impl App {
    pub(in crate::ui) fn content(&mut self, ui: &mut Ui, st: &Status) {
        ui.add_space(metrics::TOP_INSET);
        ui.horizontal(|ui| {
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
        }
    }

    fn search_field(&mut self, ui: &mut Ui) {
        let [width, height] = metrics::SEARCH_FIELD;
        let (rect, _) = ui.allocate_exact_size(vec2(width.min(ui.available_width()), height), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same((height / 2.0) as u8), colors().surface);
        let icon = Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(15.0));
        icons::paint(ui.painter(), icon, Icon::Search, colors().dim);
        let input = Rect::from_min_max(pos2(rect.left() + 38.0, rect.top() + 9.0), pos2(rect.right() - 14.0, rect.bottom() - 7.0));
        let field = egui::TextEdit::singleline(&mut self.search)
            .hint_text("Search tracks, artists, albums")
            .frame(egui::Frame::new())
            .text_color(colors().text);
        let edit = ui.put(input, field);
        if ui.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::F))) {
            edit.request_focus();
        }
        if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            let ctx = ui.ctx().clone();
            self.open_view(&ctx, View::Search);
        }
    }
}
