//! Login screen: "Log in with Deezer" (webview) or paste an ARL cookie.

use crate::ui::app::App;
use crate::ui::style::{colors, metrics, typography as ty};
use eframe::egui::{self, Key, Margin, RichText, Sense, Ui, Vec2, vec2};

impl App {
    pub(in crate::ui) fn login_screen(&mut self, ui: &mut Ui) {
        let p = colors();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.sidebar)).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.3).max(40.0));
                ui.add(egui::Image::from_texture((self.logo.id(), Vec2::splat(metrics::LOGO))));
                ui.add_space(10.0);
                let (rect, _) = ui.allocate_exact_size(vec2(200.0, 56.0), Sense::hover());
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "dust", ty::DISPLAY.font(), p.text);
                ui.label(RichText::new("Your music, nothing else.").color(p.dim).font(ty::LEAD.font()));
                ui.add_space(36.0);
                if self.login.is_some() {
                    ui.spinner();
                    return;
                }
                self.login_options(ui);
                if let Some(e) = &self.login_error {
                    ui.add_space(12.0);
                    ui.label(RichText::new(e).color(p.danger));
                }
            });
        });
    }

    fn login_options(&mut self, ui: &mut Ui) {
        let p = colors();
        #[cfg(feature = "login-window")]
        {
            if crate::ui::widgets::pill(ui, "Log in with Deezer", None, true).clicked() {
                self.log_in_with_browser(ui.ctx());
            }
            ui.add_space(10.0);
            ui.label(RichText::new("Deezer Premium required").color(p.faint).font(ty::SMALL.font()));
            ui.add_space(18.0);
            if ui.link(RichText::new("Paste ARL cookie instead").font(ty::BODY.font())).clicked() {
                self.show_arl_input = !self.show_arl_input;
            }
        }
        #[cfg(not(feature = "login-window"))]
        {
            self.show_arl_input = true;
        }
        if self.show_arl_input {
            ui.add_space(10.0);
            let field = egui::TextEdit::singleline(&mut self.arl_input)
                .password(true)
                .hint_text("arl")
                .desired_width(340.0)
                .margin(Margin::symmetric(12, 8));
            let edit = ui.add(field);
            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !self.arl_input.trim().is_empty() {
                let arl = self.arl_input.trim().to_string();
                self.log_in_with(ui.ctx(), arl);
            }
            ui.label(RichText::new("deezer.com → DevTools → Application → Cookies → arl").color(p.faint).font(ty::SMALL.font()));
        }
    }
}
