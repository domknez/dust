//! The account popover: who is logged in where, switching between Deezer and a
//! Navidrome server, Settings, log out.

use super::settings::SETTINGS_HINT;
use super::sidebar::avatar;
use crate::provider::Provider;
use crate::ui::app::App;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, Response, Sense, Ui, pos2, vec2};

const WIDTH: f32 = 272.0;

impl App {
    pub(in crate::ui) fn account_menu(&mut self, anchor: &Response, name: &str) {
        let label = self.client.as_ref().map(Provider::label).unwrap_or_default();
        let service = self.client.as_ref().map_or("", Provider::service);
        egui::Popup::menu(anchor).width(WIDTH).gap(8.0).frame(widgets::popover_frame(&anchor.ctx)).show(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            account_header(ui, name, &label);
            widgets::divider(ui);
            self.source_rows(ui);
            widgets::divider(ui);
            if widgets::shortcut_row(ui, Icon::Settings, "Settings…", SETTINGS_HINT).clicked() {
                self.show_settings = true;
                ui.close();
            }
            widgets::divider(ui);
            let log_out = if service == "Deezer" { "Log out of Deezer".to_string() } else { format!("Disconnect from {service}") };
            if widgets::menu_row(ui, Some(Icon::LogOut), &log_out, None, false, colors().danger).clicked() {
                self.log_out(ui.ctx());
                ui.close();
            }
        });
    }

    /// "MUSIC FROM": Deezer and the Navidrome server, the one in use checked; a
    /// source that isn't set up offers to set it up.
    fn source_rows(&mut self, ui: &mut Ui) {
        let p = colors();
        widgets::caption(ui, "MUSIC FROM");
        let active = self.client.as_ref().map(Provider::key);
        let mut pick = None;
        if let Some(deezer) = self.sources.deezer.clone() {
            let provider = Provider::Deezer(deezer);
            let selected = active == Some("");
            if widgets::menu_row(ui, None, "Deezer", Some(&provider.name()), selected, p.text).clicked() && !selected {
                pick = Some(provider);
            }
        } else {
            self.deezer_login_row(ui);
        }
        match self.sources.server.clone() {
            Some(server) => {
                let provider = Provider::Subsonic(server);
                let key = provider.key();
                let sub = provider.label().trim_start_matches("Navidrome · ").to_string();
                if widgets::menu_row(ui, None, "Navidrome", Some(&sub), active == Some(key), p.text).clicked() && active != Some(key) {
                    pick = Some(provider);
                }
            }
            None => {
                let sub = if self.sources.connecting() { "Connecting…" } else { "Connect a server…" };
                if widgets::menu_row(ui, Some(Icon::Plus), "Navidrome", Some(sub), false, p.dim).clicked() {
                    self.open_server_form();
                    ui.close();
                }
            }
        }
        if let Some(provider) = pick {
            self.switch_source(ui.ctx(), provider);
            ui.close();
        }
    }
}

impl App {
    /// "Log in…" for Deezer when only a server is connected.
    #[cfg(feature = "login-window")]
    fn deezer_login_row(&mut self, ui: &mut Ui) {
        if widgets::menu_row(ui, None, "Deezer", Some("Log in…"), false, colors().dim).clicked() {
            self.log_in_with_browser(ui.ctx());
            ui.close();
        }
    }

    #[cfg(not(feature = "login-window"))]
    fn deezer_login_row(&mut self, _ui: &mut Ui) {}
}

fn account_header(ui: &mut Ui, name: &str, label: &str) {
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    avatar(ui, pos2(head.left() + 22.0, head.center().y), 17.0, name, ty::LEAD);
    let width = head.width() - 56.0;
    text_left(ui.painter(), pos2(head.left() + 48.0, head.center().y - 8.0), name, ty::TITLE.strong(), colors().text, width);
    text_left(ui.painter(), pos2(head.left() + 48.0, head.center().y + 10.0), label, ty::SMALL, colors().faint, width);
}
