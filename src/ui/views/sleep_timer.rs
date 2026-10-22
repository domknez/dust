//! The player bar's "more" menu (sleep timer, mini player) and the sleep timer's
//! countdown next to it while it runs.

use super::mini_player::MINI_PLAYER_HINT;
use crate::player::{Cmd, Sleep, Status};
use crate::ui::app::App;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, Rect, Sense, Ui, Vec2, pos2, vec2};
use std::time::{Duration, Instant};

const MINUTES: [u64; 5] = [15, 30, 45, 60, 90];
const MENU_WIDTH: f32 = 220.0;

impl App {
    /// ⋯ button with its menu; in front of it, the sleep timer while one is set.
    pub(in crate::ui) fn more_button(&mut self, ui: &mut Ui, st: &Status) {
        let p = colors();
        let sleep = st.sleep.filter(|s| !matches!(s, Sleep::At(at) if *at <= Instant::now()));
        let resp = widgets::icon_button(ui, Icon::More, 18.0, p.dim).on_hover_text("More");
        let mut pick = None;
        egui::Popup::menu(&resp).width(MENU_WIDTH).frame(widgets::popover_frame(ui.ctx())).show(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            if widgets::menu_row(ui, Some(Icon::MiniPlayer), "Mini player", Some(MINI_PLAYER_HINT), false, p.text).clicked() {
                self.toggle_mini_player(ui.ctx());
                ui.close();
            }
            widgets::divider(ui);
            widgets::caption(ui, "SLEEP TIMER");
            for m in MINUTES {
                let label = if m == 60 { "1 hour".to_string() } else { format!("{m} minutes") };
                if widgets::menu_row(ui, None, &label, None, false, p.text).clicked() {
                    pick = Some(Some(Sleep::At(Instant::now() + Duration::from_secs(m * 60))));
                }
            }
            let end = sleep == Some(Sleep::EndOfTrack);
            if widgets::menu_row(ui, None, "End of track", None, end, p.text).clicked() {
                pick = Some(Some(Sleep::EndOfTrack));
            }
            if sleep.is_some() && widgets::menu_row(ui, None, "Turn off", None, false, p.dim).clicked() {
                pick = Some(None);
            }
            if pick.is_some() {
                ui.close();
            }
        });
        if let Some(s) = sleep {
            let label = match s {
                Sleep::EndOfTrack => "End of track".to_string(),
                Sleep::At(at) => left(at),
            };
            if sleep_chip(ui, &label).on_hover_text("Sleep timer · click to turn off").clicked() {
                pick = Some(None);
            }
            // Keep the countdown current.
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        }
        if let Some(sleep) = pick {
            self.player.send(Cmd::Sleep(sleep));
        }
    }
}

/// Timer icon and time left, in the accent colour.
fn sleep_chip(ui: &mut Ui, label: &str) -> egui::Response {
    let p = colors();
    let text = ui.painter().layout_no_wrap(label.to_string(), ty::CAPTION.font(), p.accent).size().x;
    let (rect, resp) = ui.allocate_exact_size(vec2(text + 30.0, 30.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(15), p.hover);
    }
    let icon = Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), Vec2::splat(15.0));
    icons::paint(ui.painter(), icon, Icon::Timer, p.accent);
    text_left(ui.painter(), pos2(rect.left() + 25.0, rect.center().y), label, ty::CAPTION, p.accent, text + 2.0);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Time left until `at`, as m:ss.
fn left(at: Instant) -> String {
    let secs = at.saturating_duration_since(Instant::now()).as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}
