//! Queue panel: now playing, then next up with jump, remove and drag-to-reorder.

use crate::player::{Cmd, Status};
use crate::ui::app::App;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, CornerRadius, Rect, RichText, Sense, Ui, Vec2, pos2, vec2};

/// Actions picked in the list this frame.
#[derive(Default)]
struct Picked {
    jump: Option<usize>,
    remove: Option<usize>,
    /// Row under the pointer while dragging.
    drop_at: Option<usize>,
}

impl App {
    pub(in crate::ui) fn queue_panel(&mut self, ui: &mut Ui, st: &Status) {
        ui.add_space(metrics::TOP_INSET);
        let queue = st.queue.clone();
        let current = st.index;
        let has_current = st.track.is_some() && current < queue.len();
        let upcoming = queue.len().saturating_sub(current + 1);

        self.queue_title(ui, upcoming > 0);
        if st.flow.is_some() {
            text_left(ui.painter(), ui.cursor().min + vec2(4.0, 8.0), "Flow · keeps going", ty::SMALL, colors().accent, 300.0);
            ui.add_space(18.0);
        }
        if !has_current && upcoming == 0 {
            empty_state(ui);
            return;
        }
        if has_current {
            widgets::caption(ui, "NOW PLAYING");
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::QUEUE_ROW), Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), colors().hover);
            widgets::queue_row(ui, &mut self.covers, rect, &queue[current], true, true);
            ui.add_space(10.0);
        }
        if upcoming == 0 {
            return;
        }
        widgets::caption(ui, &format!("NEXT UP · {upcoming}"));
        let mut picked = Picked::default();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, metrics::QUEUE_ROW, upcoming, |ui, range| {
            for i in range.map(|k| current + 1 + k) {
                self.upcoming_row(ui, i, &queue[i], &mut picked);
            }
        });
        self.finish_drag(ui, picked.drop_at);
        if let Some(i) = picked.remove {
            self.player.send(Cmd::Remove(i));
        } else if let Some(i) = picked.jump {
            self.player.send(Cmd::JumpTo(i));
        }
    }

    fn queue_title(&mut self, ui: &mut Ui, can_clear: bool) {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width() - 90.0, 36.0), Sense::hover());
            text_left(ui.painter(), pos2(r.left() + 4.0, r.center().y), "Queue", ty::PANEL_TITLE, colors().text, r.width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, Icon::Close, 14.0, colors().dim).on_hover_text("Close").clicked() {
                    self.show_queue = false;
                }
                if can_clear && ui.link(RichText::new("Clear").font(ty::BODY.font())).clicked() {
                    self.player.send(Cmd::ClearUpcoming);
                }
            });
        });
    }

    fn upcoming_row(&mut self, ui: &mut Ui, i: usize, track: &crate::deezer::Track, picked: &mut Picked) {
        let p = colors();
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), metrics::QUEUE_ROW), Sense::click_and_drag());
        if resp.hovered() || self.queue_drag == Some(i) {
            ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
        }
        // A remove button replaces the duration on hover.
        let show_remove = resp.hovered() && self.queue_drag.is_none();
        widgets::queue_row(ui, &mut self.covers, rect, track, false, !show_remove);
        let remove = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(24.0));
        let over_remove = resp.hover_pos().is_some_and(|pos| remove.contains(pos));
        if show_remove {
            ui.painter().rect_filled(remove, CornerRadius::same(12), if over_remove { p.raised } else { p.hover });
            icons::paint(ui.painter(), Rect::from_center_size(remove.center(), Vec2::splat(10.0)), Icon::Close, p.dim);
        }
        if resp.drag_started() {
            self.queue_drag = Some(i);
        }
        if resp.clicked() {
            if over_remove {
                picked.remove = Some(i);
            } else {
                picked.jump = Some(i);
            }
        }
        if let Some(from) = self.queue_drag
            && ui.ctx().pointer_interact_pos().is_some_and(|pos| rect.contains(pos))
        {
            picked.drop_at = Some(i);
            let y = if from < i { rect.bottom() } else { rect.top() };
            ui.painter().hline(rect.x_range(), y, egui::Stroke::new(2.0, p.accent));
        }
    }

    fn finish_drag(&mut self, ui: &Ui, drop_at: Option<usize>) {
        let Some(from) = self.queue_drag else { return };
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        if ui.input(|i| i.pointer.any_released()) {
            if let Some(to) = drop_at
                && to != from
            {
                self.player.send(Cmd::Move(from, to));
            }
            self.queue_drag = None;
        }
    }
}

fn empty_state(ui: &mut Ui) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("Your queue is empty").color(colors().dim).font(ty::TITLE.font()));
        ui.add_space(4.0);
        ui.label(RichText::new("Right-click a track or card → Add to queue").color(colors().faint).font(ty::SMALL.font()));
    });
}
