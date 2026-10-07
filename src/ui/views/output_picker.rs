//! The AirPlay button and its "Play on" menu.

use crate::player::Output;
use crate::ui::app::App;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, CornerRadius, Rect, RichText, Sense, Ui, Vec2, pos2, vec2};

const BUTTON_HEIGHT: f32 = 34.0;
const LABEL_MAX_WIDTH: f32 = 140.0;
const MENU_WIDTH: f32 = 260.0;

impl App {
    /// AirPlay icon (plus the speaker name when not playing locally) opening the menu.
    pub(in crate::ui) fn output_button(&mut self, ui: &mut Ui, current: &str) {
        let p = colors();
        let remote = current != Output::Local.name();
        let label_width = if remote {
            ui.painter().layout_no_wrap(current.to_string(), ty::SECONDARY.font(), p.accent).size().x.min(LABEL_MAX_WIDTH) + 8.0
        } else {
            0.0
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(BUTTON_HEIGHT + label_width, BUTTON_HEIGHT), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same((BUTTON_HEIGHT / 2.0) as u8), p.hover);
        }
        let color = if remote {
            p.accent
        } else if resp.hovered() {
            p.text
        } else {
            p.dim
        };
        let icon = Rect::from_center_size(pos2(rect.left() + BUTTON_HEIGHT / 2.0, rect.center().y), Vec2::splat(18.0));
        icons::paint(ui.painter(), icon, Icon::AirPlay, color);
        if remote {
            text_left(ui.painter(), pos2(rect.left() + 32.0, rect.center().y), current, ty::SECONDARY, p.accent, LABEL_MAX_WIDTH);
        }
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        // Opening the menu searches again, so a speaker that went missing (sleep,
        // network change) turns up within a second or two.
        if resp.clicked()
            && let Some(discovery) = &self.discovery
        {
            discovery.refresh();
        }
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(MENU_WIDTH);
            if let Some(output) = self.output_menu(ui, current, remote) {
                self.select_output(output);
                ui.close();
            }
        });
    }

    fn output_menu(&self, ui: &mut Ui, current: &str, remote: bool) -> Option<Output> {
        ui.add_space(4.0);
        ui.label(RichText::new("PLAY ON").color(colors().faint).font(ty::OVERLINE.font()));
        ui.add_space(2.0);
        let mut pick = None;
        if widgets::nav_item(ui, Icon::Computer, Output::Local.name(), !remote).clicked() {
            pick = Some(Output::Local);
        }
        let devices = self.discovery.as_ref().map(|d| d.devices()).unwrap_or_default();
        if devices.is_empty() {
            ui.label(RichText::new("Looking for AirPlay speakers…").color(colors().faint).font(ty::SMALL.font()));
        }
        for device in devices.into_iter().filter(|d| d.supported && !d.password) {
            if widgets::nav_item(ui, Icon::AirPlay, &device.name, current == device.name).clicked() {
                pick = Some(Output::AirPlay(device));
            }
        }
        pick
    }
}
