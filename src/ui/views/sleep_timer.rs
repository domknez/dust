//! The player bar's ⋯ menu: the mini player and the sleep timer (a countdown ring,
//! the time left and the choices), plus the timer's countdown chip next to it.

use super::mini_player::MINI_PLAYER_KEYS;
use crate::player::{SLEEP_FADE, Sleep, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, radius, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, Align2, CornerRadius, Id, Rect, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};
use std::time::{Duration, Instant};

/// Timed choices, in minutes.
const MINUTES: [u64; 4] = [15, 30, 45, 60];
const MENU_WIDTH: f32 = 316.0;
const RING: f32 = 84.0;
const CHOICE_HEIGHT: f32 = 40.0;
const GAP: f32 = 8.0;

/// What the sleep timer was set to, so the ring knows its full length and the
/// choice stays lit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SleepChoice {
    Minutes(u64),
    EndOfTrack,
}

impl App {
    /// ⋯ button with its menu; in front of it, the sleep timer's chip while one runs.
    pub(in crate::ui) fn more_button(&self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        let p = colors();
        let sleep = st.sleep.filter(|s| !matches!(s, Sleep::At(at) if *at <= Instant::now()));
        let choice = sleep.and(self.sleep_choice);
        let menu_id = Id::unique("more-menu");
        let resp = widgets::icon_button(ui, Icon::More, 18.0, p.dim).on_hover_text("More");
        let mut pick = None;
        let popup = egui::Popup::menu(&resp).id(menu_id).width(MENU_WIDTH).frame(widgets::popover_frame(ui.ctx()));
        // Stays open for clicks inside (the ring, the note); choices close it.
        popup.close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_width(MENU_WIDTH);
            ui.spacing_mut().item_spacing.y = 2.0;
            if widgets::shortcut_row(ui, Icon::MiniPlayer, "Mini player", MINI_PLAYER_KEYS).clicked() {
                acts.push(Action::ToggleMiniPlayer);
                ui.close();
            }
            widgets::divider(ui);
            pick = sleep_panel(ui, sleep, choice);
            if pick.is_some() {
                ui.close();
            }
            if sleep.is_some() {
                ui.ctx().request_repaint_after(Duration::from_secs(1));
            }
        });
        if let Some(s) = sleep {
            let label = match s {
                Sleep::EndOfTrack => "end".to_string(),
                Sleep::At(at) => short_left(at),
            };
            if sleep_chip(ui, &label).on_hover_text("Sleep timer").clicked() {
                egui::Popup::toggle_id(ui.ctx(), menu_id);
            }
            // Keep the countdown current.
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        }
        acts.extend(pick.map(Action::Sleep));
    }
}

/// Ring, time left and the choices. Returns a new choice (None = turn off).
fn sleep_panel(ui: &mut Ui, sleep: Option<Sleep>, choice: Option<SleepChoice>) -> Option<Option<SleepChoice>> {
    let p = colors();
    let mut pick = None;
    ui.add_space(8.0);
    let (top, _) = ui.allocate_exact_size(vec2(ui.available_width(), RING + 8.0), Sense::hover());
    let center = pos2(top.left() + 8.0 + RING / 2.0, top.center().y);
    // The ring drains as the time runs out.
    let left = match sleep {
        Some(Sleep::At(at)) => Some(at.saturating_duration_since(Instant::now())),
        _ => None,
    };
    let full = match choice {
        Some(SleepChoice::Minutes(m)) => Duration::from_secs(m * 60),
        _ => Duration::from_secs(1),
    };
    let share = match (sleep, left) {
        (Some(Sleep::EndOfTrack), _) => 1.0,
        (_, Some(left)) => (left.as_secs_f32() / full.as_secs_f32()).clamp(0.0, 1.0),
        _ => 0.0,
    };
    ring(ui.painter(), center, RING / 2.0 - 3.0, share);
    let icon_color = if sleep.is_some() { p.text } else { p.dim };
    icons::paint(ui.painter(), Rect::from_center_size(center, Vec2::splat(26.0)), Icon::Timer, icon_color);

    let x = center.x + RING / 2.0 + 18.0;
    let width = top.right() - x;
    let (big, note) = match (sleep, left) {
        (Some(Sleep::At(_)), Some(left)) => (clock(left), format!("Fades out over {} s, then pauses", SLEEP_FADE.as_secs())),
        (Some(Sleep::EndOfTrack), _) => ("End".to_string(), "Stops when this track ends".to_string()),
        _ => ("Off".to_string(), "Pick when the music stops".to_string()),
    };
    text_left(ui.painter(), pos2(x, center.y - 12.0), &big, ty::PAGE_TITLE, p.text, width);
    let note_rect = Rect::from_min_size(pos2(x, center.y + 10.0), vec2(width, 34.0));
    ui.put(note_rect, egui::Label::new(egui::RichText::new(note).font(ty::SMALL.font()).color(p.dim)).wrap());

    // 15 min · 30 min · 45 min / 1 hour · End of track
    ui.add_space(12.0);
    let full_width = ui.available_width();
    let third = (full_width - 2.0 * GAP) / 3.0;
    let (grid, _) = ui.allocate_exact_size(vec2(full_width, 2.0 * CHOICE_HEIGHT + GAP), Sense::hover());
    let mut cells: Vec<(Rect, SleepChoice, String)> = MINUTES
        .iter()
        .enumerate()
        .map(|(k, &m)| {
            let (col, row) = ((k % 3) as f32, (k / 3) as f32);
            let rect = Rect::from_min_size(grid.min + vec2(col * (third + GAP), row * (CHOICE_HEIGHT + GAP)), vec2(third, CHOICE_HEIGHT));
            let label = if m == 60 { "1 hour".to_string() } else { format!("{m} min") };
            (rect, SleepChoice::Minutes(m), label)
        })
        .collect();
    let end = Rect::from_min_size(grid.min + vec2(third + GAP, CHOICE_HEIGHT + GAP), vec2(2.0 * third + GAP, CHOICE_HEIGHT));
    cells.push((end, SleepChoice::EndOfTrack, "End of track".to_string()));
    for (rect, this, label) in cells {
        let resp = ui.interact(rect, ui.scope_id().with(("sleep-choice", label.as_str())), Sense::click());
        let on = choice == Some(this);
        let (bg, fg) = match (on, resp.hovered()) {
            (true, _) => (p.accent_fill, p.on_accent),
            (false, true) => (p.pressed, p.text),
            (false, false) => (p.raised, p.text),
        };
        ui.painter().rect_filled(rect, CornerRadius::same(radius::CONTROL), bg);
        let style = if on { ty::TITLE.strong() } else { ty::TITLE };
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, style.font(), fg);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            pick = Some(Some(this));
        }
    }
    if sleep.is_some() {
        ui.add_space(10.0);
        let (off, resp) = ui.allocate_exact_size(vec2(full_width, CHOICE_HEIGHT), Sense::click());
        let corner = CornerRadius::same(radius::CONTROL);
        if resp.hovered() {
            ui.painter().rect_filled(off, corner, p.raised);
        }
        ui.painter().rect_stroke(off, corner, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
        ui.painter().text(off.center(), Align2::CENTER_CENTER, "Turn off timer", ty::TITLE.font(), p.text);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            pick = Some(None);
        }
    }
    ui.add_space(6.0);
    pick
}

/// A track ring with a lime arc for the `share` of time left, clockwise from the top.
fn ring(painter: &egui::Painter, center: egui::Pos2, radius: f32, share: f32) {
    let p = colors();
    painter.circle_stroke(center, radius, Stroke::new(5.0, p.line));
    if share <= 0.0 {
        return;
    }
    let start = -std::f32::consts::FRAC_PI_2;
    let sweep = std::f32::consts::TAU * share;
    let n = (64.0 * share).ceil().max(2.0) as usize;
    let points = (0..=n).map(|i| start + sweep * i as f32 / n as f32).map(|a| center + vec2(a.cos(), a.sin()) * radius).collect();
    painter.add(Shape::line(points, Stroke::new(5.0, p.accent)));
}

/// Stopwatch and time left, in the accent colour.
fn sleep_chip(ui: &mut Ui, label: &str) -> egui::Response {
    let p = colors();
    let text = ui.painter().layout_no_wrap(label.to_string(), ty::META.font(), p.accent).size().x;
    let (rect, resp) = ui.allocate_exact_size(vec2(text + 30.0, 30.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
    }
    let icon = Rect::from_center_size(pos2(rect.left() + 13.0, rect.center().y), Vec2::splat(14.0));
    icons::paint(ui.painter(), icon, Icon::Timer, p.accent);
    text_left(ui.painter(), pos2(rect.left() + 24.0, rect.center().y), label, ty::META, p.accent, text + 2.0);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// "28:14".
fn clock(left: Duration) -> String {
    let secs = left.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// The chip's short form: "28m", or "45s" in the last minute.
fn short_left(at: Instant) -> String {
    let secs = at.saturating_duration_since(Instant::now()).as_secs();
    if secs >= 60 { format!("{}m", secs.div_ceil(60)) } else { format!("{secs}s") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_left_as_text() {
        assert_eq!(clock(Duration::from_secs(28 * 60 + 14)), "28:14");
        assert_eq!(clock(Duration::from_secs(5)), "0:05");
        let soon = Instant::now() + Duration::from_millis(45_500);
        assert_eq!(short_left(soon), "45s");
        let later = Instant::now() + Duration::from_secs(27 * 60 + 30);
        assert_eq!(short_left(later), "28m");
    }
}
