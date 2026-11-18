//! The account popover: who is logged in where, switching between the music
//! sources (or connecting one), Settings, log out.

use super::sidebar::avatar;
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::shortcuts;
use crate::ui::sources::MENU;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, typography as ty};
use crate::ui::widgets;
use eframe::egui::{self, Response, Sense, Ui, pos2, vec2};

const WIDTH: f32 = 272.0;

impl App {
    pub(in crate::ui) fn account_menu(&self, anchor: &Response, name: &str, acts: &mut Actions) {
        let label = self.client.as_ref().map(|c| c.label()).unwrap_or_default();
        let log_out = self.client.as_ref().map(|c| c.log_out_label()).unwrap_or_default();
        egui::Popup::menu(anchor).width(WIDTH).gap(8.0).frame(widgets::popover_frame(&anchor.ctx)).show(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            account_header(ui, name, &label);
            widgets::divider(ui);
            self.source_rows(ui, acts);
            widgets::divider(ui);
            if widgets::shortcut_row(ui, Icon::Search, "Commands…", &shortcuts::label(&shortcuts::PALETTE)).clicked() {
                acts.push(Action::OpenPalette);
                ui.close();
            }
            if widgets::shortcut_row(ui, Icon::Settings, "Settings…", &shortcuts::label(&shortcuts::SETTINGS)).clicked() {
                acts.push(Action::ShowSettings(true));
                ui.close();
            }
            widgets::divider(ui);
            if widgets::menu_row(ui, Some(Icon::LogOut), &log_out, None, false, colors().danger).clicked() {
                acts.push(Action::LogOut);
                ui.close();
            }
        });
    }

    /// "MUSIC FROM": every source, the one in use checked; one that isn't logged
    /// in offers to connect it.
    fn source_rows(&self, ui: &mut Ui, acts: &mut Actions) {
        let p = colors();
        widgets::caption(ui, "MUSIC FROM");
        let active = self.client.as_ref().map(|c| c.id());
        for row in MENU {
            match self.sources.get(row.id) {
                Some(source) => {
                    let selected = active == Some(row.id);
                    if widgets::menu_row(ui, Some(row.icon), row.id.name, Some(&source.detail()), selected, p.text).clicked() && !selected {
                        acts.push(Action::SwitchSource(source.clone()));
                        ui.close();
                    }
                }
                None => {
                    let Some((note, connect)) = (row.connect)(self) else { continue };
                    if widgets::menu_row(ui, Some(row.icon), row.id.name, Some(note), false, p.dim).clicked() {
                        acts.push(connect);
                        ui.close();
                    }
                }
            }
        }
    }
}

fn account_header(ui: &mut Ui, name: &str, label: &str) {
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    avatar(ui, pos2(head.left() + 22.0, head.center().y), 17.0, name, ty::LEAD);
    let width = head.width() - 56.0;
    let title = widgets::TextLine { text: name, style: ty::TITLE.strong(), color: colors().text };
    let subtitle = widgets::TextLine { text: label, style: ty::SMALL, color: colors().faint };
    widgets::two_lines(ui.painter(), head.left() + 48.0, head.center().y, width, title, subtitle);
}
