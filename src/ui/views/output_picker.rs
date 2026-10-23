//! The AirPlay button and its "Play on" menu.

use crate::output::airplay::Device;
use crate::player::{Cmd, Output, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, radius, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, Align2, CornerRadius, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2, pos2, vec2};

const BUTTON_HEIGHT: f32 = 30.0;
const LABEL_MAX_WIDTH: f32 = 140.0;
const MENU_WIDTH: f32 = 320.0;
const ROW_HEIGHT: f32 = 58.0;
/// The volume under the playing output.
const VOLUME_ROW: f32 = 30.0;

impl App {
    /// AirPlay icon opening the menu; while a speaker plays, an outlined lime button
    /// with its name.
    pub(in crate::ui) fn output_button(&mut self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        let p = colors();
        let current = st.output.as_str();
        let remote = current != Output::Local.name();
        let name = current.to_lowercase();
        let label_width = if remote {
            ui.painter().layout_no_wrap(name.clone(), ty::META.font(), p.accent).size().x.min(LABEL_MAX_WIDTH) + 10.0
        } else {
            0.0
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(BUTTON_HEIGHT + label_width, BUTTON_HEIGHT), Sense::click());
        let corner = CornerRadius::same(radius::ROW);
        if resp.hovered() {
            ui.painter().rect_filled(rect, corner, p.hover);
        }
        if remote {
            ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
        }
        let color = if remote {
            p.accent
        } else if resp.hovered() {
            p.text
        } else {
            p.dim
        };
        let icon = Rect::from_center_size(pos2(rect.left() + BUTTON_HEIGHT / 2.0, rect.center().y), Vec2::splat(15.0));
        icons::paint(ui.painter(), icon, Icon::AirPlay, color);
        if remote {
            text_left(ui.painter(), pos2(rect.left() + 28.0, rect.center().y), &name, ty::META, p.accent, LABEL_MAX_WIDTH);
        }
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        // Opening the menu searches again, so a speaker that went missing (sleep,
        // network change) turns up within a second or two.
        if resp.clicked() {
            acts.push(Action::FindSpeakers);
        }
        // Stays open while the volume is dragged; picking a speaker closes it.
        let popup = egui::Popup::menu(&resp).width(MENU_WIDTH).frame(widgets::popover_frame(ui.ctx()));
        popup.close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_width(MENU_WIDTH);
            if let Some(output) = self.output_menu(ui, st, remote, acts) {
                acts.push(Action::Output(output));
                ui.close();
            }
        });
    }

    /// "Play on": this computer and the speakers found, one card per output; the
    /// one playing is raised and carries the volume.
    fn output_menu(&mut self, ui: &mut Ui, st: &Status, remote: bool, acts: &mut Actions) -> Option<Output> {
        let p = colors();
        let devices: Vec<Device> =
            self.discovery.as_ref().map(|d| d.devices()).unwrap_or_default().into_iter().filter(|d| d.supported && !d.password).collect();
        ui.add_space(4.0);
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
        text_left(ui.painter(), pos2(head.left() + 8.0, head.center().y), "Play on", ty::SECTION_TITLE, p.text, head.width() - 100.0);
        let count = match devices.len() {
            1 => "1 speaker".to_string(),
            n => format!("{n} speakers"),
        };
        ui.painter().text(pos2(head.right() - 8.0, head.center().y), Align2::RIGHT_CENTER, count, ty::META.font(), p.dim);
        ui.add_space(6.0);
        ui.spacing_mut().item_spacing.y = 4.0;

        let mut pick = None;
        let local = if cfg!(target_os = "macos") { "This Mac's speakers" } else { "This computer's speakers" };
        if self.output_row(ui, st, Output::Local.name(), local, !remote, acts) {
            pick = Some(Output::Local);
        }
        for device in devices {
            if self.output_row(ui, st, &device.name, &device.kind(), st.output == device.name, acts) {
                pick = Some(Output::AirPlay(device));
            }
        }
        if self.discovery.as_ref().is_none_or(|d| d.devices().is_empty()) {
            ui.add_space(4.0);
            ui.label(RichText::new("Looking for AirPlay speakers…").color(p.faint).font(ty::SMALL.font()));
        }
        ui.add_space(4.0);
        pick
    }

    /// One output: speaker tile, name and kind, a round check; the current one also
    /// shows the volume. Returns whether it was picked.
    fn output_row(&mut self, ui: &mut Ui, st: &Status, name: &str, kind: &str, current: bool, acts: &mut Actions) -> bool {
        let p = colors();
        let height = if current { ROW_HEIGHT + VOLUME_ROW } else { ROW_HEIGHT };
        let (card, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let row = Rect::from_min_size(card.min, vec2(card.width(), ROW_HEIGHT));
        let resp = ui.interact(row, ui.scope_id().with(("output", name)), Sense::click());
        let corner = CornerRadius::same(radius::PANEL);
        if current {
            ui.painter().rect_filled(card, corner, p.raised);
        } else if resp.hovered() {
            ui.painter().rect_filled(card, corner, p.hover);
        }
        let tile = Rect::from_min_size(pos2(row.left() + 10.0, row.center().y - 18.0), Vec2::splat(36.0));
        let (tile_bg, tile_fg) = if current { (p.accent_fill, p.on_accent) } else { (p.surface, p.dim) };
        ui.painter().rect_filled(tile, CornerRadius::same(radius::CONTROL), tile_bg);
        if !current {
            ui.painter().rect_stroke(tile, CornerRadius::same(radius::CONTROL), Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        }
        icons::paint(ui.painter(), Rect::from_center_size(tile.center(), Vec2::splat(18.0)), Icon::Speaker, tile_fg);
        let x = tile.right() + 12.0;
        let width = row.right() - 44.0 - x;
        text_left(ui.painter(), pos2(x, row.center().y - 9.0), name, ty::ITEM.strong(), p.text, width);
        text_left(ui.painter(), pos2(x, row.center().y + 10.0), kind, ty::SMALL, p.dim, width);
        let check = pos2(row.right() - 22.0, row.center().y);
        if current {
            ui.painter().circle_filled(check, 11.0, p.accent_fill);
            icons::paint(ui.painter(), Rect::from_center_size(check, Vec2::splat(12.0)), Icon::Check, p.on_accent);
        } else {
            ui.painter().circle_stroke(check, 10.5, Stroke::new(1.0, p.border));
        }
        if current {
            let line = Rect::from_min_max(pos2(x - 4.0, row.bottom() - 4.0), pos2(card.right() - 14.0, card.bottom() - 8.0));
            ui.scope_builder(UiBuilder::new().max_rect(line).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                icons::paint(
                    ui.painter(),
                    Rect::from_center_size(pos2(line.left() + 8.0, line.center().y), Vec2::splat(13.0)),
                    Icon::Volume,
                    p.dim,
                );
                ui.add_space(24.0);
                let mut volume = self.volume_drag.unwrap_or(st.volume);
                let slider = widgets::thin_slider(ui, &mut volume, ui.available_width(), true, false);
                if slider.changed() {
                    self.volume_drag = Some(volume);
                    acts.push(Action::Player(Cmd::Volume(volume)));
                }
                if slider.drag_stopped() || (!slider.dragged() && !slider.hovered() && self.volume_drag.is_some()) {
                    self.volume_drag = None;
                }
            });
        }
        !current && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    }
}
