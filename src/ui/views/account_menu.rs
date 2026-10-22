//! The account popover: who is logged in, Settings, log out.

use super::settings::SETTINGS_HINT;
use super::sidebar::avatar;
use crate::ui::app::App;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, Response, Sense, Ui, pos2, vec2};

const WIDTH: f32 = 272.0;

impl App {
    pub(in crate::ui) fn account_menu(&mut self, anchor: &Response, name: &str) {
        egui::Popup::menu(anchor).width(WIDTH).gap(8.0).frame(widgets::popover_frame(&anchor.ctx)).show(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            account_header(ui, name);
            widgets::divider(ui);
            if widgets::shortcut_row(ui, Icon::Settings, "Settings…", SETTINGS_HINT).clicked() {
                self.show_settings = true;
                ui.close();
            }
            widgets::divider(ui);
            if widgets::menu_row(ui, Some(Icon::LogOut), "Log out", None, false, colors().danger).clicked() {
                self.log_out();
                ui.close();
            }
        });
    }
}

fn account_header(ui: &mut Ui, name: &str) {
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    avatar(ui, pos2(head.left() + 22.0, head.center().y), 17.0, name, ty::LEAD);
    let width = head.width() - 56.0;
    text_left(ui.painter(), pos2(head.left() + 48.0, head.center().y - 8.0), name, ty::TITLE.strong(), colors().text, width);
    text_left(ui.painter(), pos2(head.left() + 48.0, head.center().y + 10.0), "Deezer account", ty::SMALL, colors().faint, width);
}
