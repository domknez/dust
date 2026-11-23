//! The panel on the right: *up next* (now playing, then next up with jump, remove
//! and drag-to-reorder), *history* (see [`super::play_history`]) and *crate* (see
//! [`super::crate_view`]).

use crate::player::{Cmd, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::{App, SidePanel};
use crate::ui::state::for_playlist;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, CornerRadius, Rect, Sense, Ui, Vec2, pos2, vec2};

impl App {
    pub(in crate::ui) fn queue_panel(&mut self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        ui.add_space(metrics::TOP_INSET);
        let queue = st.queue.clone();
        let current = st.index;
        let has_current = st.track.is_some() && current < queue.len();
        let upcoming = queue.len().saturating_sub(current + 1);

        self.queue_title(ui, &queue, acts);
        if st.flow.is_some() {
            let label = if self.features().flow { "Flow · keeps going" } else { "Radio · keeps going" };
            text_left(ui.painter(), ui.cursor().min + vec2(4.0, 8.0), label, ty::SMALL, colors().accent, 300.0);
            ui.add_space(18.0);
        }
        if !has_current && upcoming == 0 {
            widgets::empty_panel(ui, "Your queue is empty", "Right-click a track or card → Add to queue");
            return;
        }
        if has_current {
            widgets::caption(ui, "NOW PLAYING");
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::QUEUE_ROW), Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), colors().hover);
            acts.extend(widgets::queue_row(ui, &mut self.covers, rect, &queue[current], true, true).map(Action::OpenArtist));
            ui.add_space(10.0);
        }
        if upcoming == 0 {
            return;
        }
        // "clear" sits with what it clears: the tracks still to come.
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
        text_left(
            ui.painter(),
            pos2(row.left() + 4.0, row.center().y),
            &format!("NEXT UP · {upcoming}"),
            ty::OVERLINE,
            colors().faint,
            200.0,
        );
        let clear =
            text_button(ui, pos2(row.right() - 4.0, row.center().y), "clear", None, true).on_hover_text("Remove the tracks still to come");
        if clear.clicked() {
            acts.push(Action::Player(Cmd::ClearUpcoming));
        }
        // The row under the pointer while dragging.
        let mut drop_at = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, metrics::QUEUE_ROW, upcoming, |ui, range| {
            for i in range.map(|k| current + 1 + k) {
                self.upcoming_row(ui, i, &queue[i], &mut drop_at, acts);
            }
        });
        self.finish_drag(ui, drop_at, acts);
    }

    /// The tabs, then "+ save as playlist" for the whole queue.
    fn queue_title(&mut self, ui: &mut Ui, queue: &[crate::model::Track], acts: &mut Actions) {
        self.side_panel_tabs(ui, acts);
        if queue.is_empty() {
            return;
        }
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        let save = text_button(ui, pos2(row.left() + 4.0, row.center().y), "save as playlist", Some(Icon::Plus), false)
            .on_hover_text("Save the whole queue as a new playlist");
        if save.clicked() {
            acts.push(Action::NewPlaylist(for_playlist(queue.to_vec())));
        }
        ui.add_space(4.0);
    }

    /// up next | history | crate switch at the top of the right panel, and a close
    /// button.
    pub(in crate::ui) fn side_panel_tabs(&self, ui: &mut Ui, acts: &mut Actions) {
        let p = colors();
        ui.horizontal(|ui| {
            let (tray, _) = ui.allocate_exact_size(vec2(ui.available_width() - 40.0, 34.0), Sense::hover());
            let corner = CornerRadius::same(radius::CONTROL);
            ui.painter().rect_filled(tray, corner, p.surface);
            ui.painter().rect_stroke(tray, corner, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
            let tabs: Vec<_> = [(SidePanel::Queue, "up next"), (SidePanel::History, "history"), (SidePanel::Crate, "crate")]
                .into_iter()
                .filter(|(panel, _)| self.panel_allowed(*panel))
                .collect();
            let width = (tray.width() - 6.0) / tabs.len() as f32;
            for (i, (panel, label)) in tabs.into_iter().enumerate() {
                let tab =
                    Rect::from_min_size(pos2(tray.left() + 3.0 + i as f32 * width, tray.top() + 3.0), vec2(width, tray.height() - 6.0));
                let resp = ui.interact(tab, ui.scope_id().with(("side-tab", i)), Sense::click());
                let on = self.side_panel == Some(panel);
                let color = match (on, resp.hovered()) {
                    (true, _) => p.bg,
                    (false, true) => p.text,
                    (false, false) => p.dim,
                };
                if on {
                    ui.painter().rect_filled(tab, CornerRadius::same(radius::SEGMENT), p.text);
                }
                ui.painter().text(tab.center(), egui::Align2::CENTER_CENTER, label, ty::META.font(), color);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    acts.push(Action::ShowPanel(Some(panel)));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, Icon::Close, 14.0, p.dim).on_hover_text("Close").clicked() {
                    acts.push(Action::ShowPanel(None));
                }
            });
        });
        ui.add_space(6.0);
    }

    fn upcoming_row(&mut self, ui: &mut Ui, i: usize, track: &crate::model::Track, drop_at: &mut Option<usize>, acts: &mut Actions) {
        let p = colors();
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::QUEUE_ROW), Sense::click_and_drag());
        // Pointer anywhere in the row, also over its artist links.
        let hovered = ui.rect_contains_pointer(rect);
        if hovered || self.queue_drag == Some(i) {
            ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
        }
        // A remove button replaces the duration on hover.
        let show_remove = hovered && self.queue_drag.is_none();
        acts.extend(widgets::queue_row(ui, &mut self.covers, rect, track, false, !show_remove).map(Action::OpenArtist));
        // A lime dot on the cover: never heard before.
        if self.prefs.history_on && !track.is_episode() && !self.plays.has_heard(self.data_key(), &track.id) {
            let corner = pos2(rect.left() + 6.0 + metrics::LIST_ART, rect.center().y - metrics::LIST_ART / 2.0);
            ui.painter().circle(corner, 4.5, p.accent, egui::Stroke::new(2.0, p.surface));
            let dot = Rect::from_center_size(corner, Vec2::splat(14.0));
            ui.interact(dot, ui.scope_id().with(("new-to-you", i)), Sense::hover()).on_hover_text("New to you: not in your play history");
        }
        let remove = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(24.0));
        let over_remove = resp.hover_pos().is_some_and(|pos| remove.contains(pos));
        if show_remove {
            ui.painter().rect_filled(
                remove,
                CornerRadius::same((remove.height() / 2.0) as u8),
                if over_remove { p.raised } else { p.hover },
            );
            icons::paint(ui.painter(), Rect::from_center_size(remove.center(), Vec2::splat(10.0)), Icon::Close, p.dim);
        }
        if resp.drag_started() {
            self.queue_drag = Some(i);
        }
        if resp.clicked() {
            acts.push(Action::Player(if over_remove { Cmd::Remove(i) } else { Cmd::JumpTo(i) }));
        }
        if let Some(from) = self.queue_drag
            && ui.ctx().pointer_interact_pos().is_some_and(|pos| rect.contains(pos))
        {
            *drop_at = Some(i);
            let y = if from < i { rect.bottom() } else { rect.top() };
            ui.painter().hline(rect.x_range(), y, egui::Stroke::new(2.0, p.accent));
        }
    }

    fn finish_drag(&mut self, ui: &Ui, drop_at: Option<usize>, acts: &mut Actions) {
        let Some(from) = self.queue_drag else { return };
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        if ui.input(|i| i.pointer.any_released()) {
            if let Some(to) = drop_at
                && to != from
            {
                acts.push(Action::Player(Cmd::Move(from, to)));
            }
            self.queue_drag = None;
        }
    }
}

/// A small text button at `at` (its left edge, or right edge with `right`), with an
/// icon in front: dim, lit on hover.
fn text_button(ui: &Ui, at: egui::Pos2, label: &str, icon: Option<Icon>, right: bool) -> egui::Response {
    let p = colors();
    let galley = ui.painter().layout_no_wrap(label.to_string(), ty::META.font(), egui::Color32::PLACEHOLDER);
    let icon_width = if icon.is_some() { 16.0 } else { 0.0 };
    let width = galley.size().x + icon_width;
    let left = if right { at.x - width } else { at.x };
    let rect = Rect::from_min_size(pos2(left - 4.0, at.y - 11.0), vec2(width + 8.0, 22.0));
    let resp = ui.interact(rect, ui.scope_id().with(("text-button", label)), Sense::click());
    let color = if resp.hovered() { p.text } else { p.dim };
    if let Some(icon) = icon {
        icons::paint(ui.painter(), Rect::from_center_size(pos2(left + 5.0, at.y), Vec2::splat(10.0)), icon, color);
    }
    ui.painter().galley(pos2(left + icon_width, at.y - galley.size().y / 2.0), galley, color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
