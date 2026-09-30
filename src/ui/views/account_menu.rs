//! The account popover: appearance, streaming quality, listen sharing, log out.

use super::sidebar::avatar;
use crate::deezer::Quality;
use crate::ui::app::App;
use crate::ui::style::icons::Icon;
use crate::ui::style::{Appearance, colors, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, Response, Sense, Ui, pos2, vec2};

const WIDTH: f32 = 272.0;

impl App {
    pub(in crate::ui) fn account_menu(&mut self, anchor: &Response, name: &str) {
        egui::Popup::menu(anchor).width(WIDTH).gap(8.0).frame(widgets::popover_frame(&anchor.ctx)).show(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            account_header(ui, name);
            widgets::divider(ui);
            self.appearance_picker(ui);
            ui.add_space(8.0);
            self.quality_picker(ui);
            ui.add_space(8.0);
            widgets::caption(ui, "LISTENING");
            let sub = "History, Flow and Last.fm scrobbling via Deezer";
            if widgets::menu_row(ui, None, "Share listening with Deezer", Some(sub), self.report_listens, colors().text).clicked() {
                self.toggle_report_listens();
            }
            widgets::divider(ui);
            if widgets::menu_row(ui, Some(Icon::LogOut), "Log out", None, false, colors().danger).clicked() {
                self.log_out();
                ui.close();
            }
        });
    }

    fn appearance_picker(&mut self, ui: &mut Ui) {
        widgets::caption(ui, "APPEARANCE");
        let options = Appearance::ALL.map(|a| {
            let icon = match a {
                Appearance::System => Icon::Computer,
                Appearance::Dark => Icon::Moon,
                Appearance::Light => Icon::Sun,
            };
            (icon, a.label())
        });
        let current = Appearance::ALL.iter().position(|a| *a == self.appearance).unwrap_or(0);
        if let Some(i) = widgets::segmented(ui, &options, current) {
            self.set_appearance(Appearance::ALL[i]);
        }
    }

    fn quality_picker(&mut self, ui: &mut Ui) {
        widgets::caption(ui, "STREAMING QUALITY");
        for quality in Quality::ALL {
            let description = match quality {
                Quality::Mp3_128 => "Saves data",
                Quality::Mp3_320 => "High quality",
                Quality::Flac => "Lossless · needs a HiFi plan",
            };
            if widgets::menu_row(ui, None, quality.label(), Some(description), self.quality == quality, colors().text).clicked() {
                self.set_quality(quality);
            }
        }
    }
}

fn account_header(ui: &mut Ui, name: &str) {
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    avatar(ui, pos2(head.left() + 22.0, head.center().y), 17.0, name, ty::LEAD);
    let width = head.width() - 56.0;
    text_left(ui.painter(), pos2(head.left() + 48.0, head.center().y - 8.0), name, ty::TITLE.strong(), colors().text, width);
    text_left(ui.painter(), pos2(head.left() + 48.0, head.center().y + 10.0), "Deezer account", ty::SMALL, colors().faint, width);
}
