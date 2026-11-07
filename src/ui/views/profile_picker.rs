//! "Who's listening?": the members of a Deezer family plan, shown after logging in
//! with the plan owner's session and from the account menu's "Switch profile…".

use super::sidebar::avatar;
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::style::{colors, radius, typography as ty};
use crate::ui::widgets::text_left;
use eframe::egui::{self, CornerRadius, RichText, Sense, Ui, Vec2, pos2, vec2};

const ROW_WIDTH: f32 = 320.0;
const ROW_HEIGHT: f32 = 56.0;

impl App {
    pub(in crate::ui) fn profile_picker(&mut self, ui: &mut Ui, acts: &mut Actions) {
        let p = colors();
        let Some(family) = self.login.family.clone() else {
            // Nothing to pick from (logged out meanwhile).
            acts.push(Action::CancelProfilePicker);
            return;
        };
        let current = self.client.as_ref().and(family.profile());
        let can_cancel = self.client.is_some();
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.sidebar)).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.25).max(40.0));
                ui.add(egui::Image::from_texture((self.logo.id(), Vec2::splat(48.0))));
                ui.add_space(14.0);
                ui.label(RichText::new("Who's listening?").color(p.text).font(ty::LEAD.font()));
                ui.add_space(24.0);
                if self.login.task.is_loading() {
                    ui.spinner();
                    return;
                }
                for profile in family.profiles() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ROW_WIDTH, ROW_HEIGHT), Sense::click());
                    let corner = CornerRadius::same(radius::CONTROL);
                    if resp.hovered() {
                        ui.painter().rect_filled(rect, corner, p.raised);
                    }
                    let selected = current == Some(profile.id);
                    if selected {
                        ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                    }
                    avatar(ui, pos2(rect.left() + 12.0 + 18.0, rect.center().y), 18.0, &profile.name, ty::LEAD);
                    let style = if selected { ty::TITLE.strong() } else { ty::TITLE };
                    text_left(ui.painter(), pos2(rect.left() + 58.0, rect.center().y), &profile.name, style, p.text, ROW_WIDTH - 74.0);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        acts.push(Action::PickProfile(profile.id));
                    }
                    ui.add_space(6.0);
                }
                if can_cancel {
                    ui.add_space(14.0);
                    if ui.link(RichText::new("Cancel").font(ty::BODY.font())).clicked() {
                        acts.push(Action::CancelProfilePicker);
                    }
                }
                if let Some(e) = &self.login.error {
                    ui.add_space(12.0);
                    ui.label(RichText::new(e).color(p.danger));
                }
            });
        });
    }
}
