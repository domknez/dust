//! Queue panel: now playing, then next up with jump, remove and drag-to-reorder.

use crate::player::{Cmd, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::{App, SidePanel};
use crate::ui::state::for_playlist;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, CornerRadius, Rect, RichText, Sense, Ui, Vec2, pos2, vec2};

impl App {
    pub(in crate::ui) fn queue_panel(&mut self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        ui.add_space(metrics::TOP_INSET);
        let queue = st.queue.clone();
        let current = st.index;
        let has_current = st.track.is_some() && current < queue.len();
        let upcoming = queue.len().saturating_sub(current + 1);

        self.queue_title(ui, upcoming > 0, &queue, acts);
        if st.flow.is_some() {
            let label = if self.features().flow { "Flow · keeps going" } else { "Radio · keeps going" };
            text_left(ui.painter(), ui.cursor().min + vec2(4.0, 8.0), label, ty::SMALL, colors().accent, 300.0);
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
            acts.extend(widgets::queue_row(ui, &mut self.covers, rect, &queue[current], true, true).map(Action::OpenArtist));
            ui.add_space(10.0);
        }
        if upcoming == 0 {
            return;
        }
        widgets::caption(ui, &format!("NEXT UP · {upcoming}"));
        // The row under the pointer while dragging.
        let mut drop_at = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, metrics::QUEUE_ROW, upcoming, |ui, range| {
            for i in range.map(|k| current + 1 + k) {
                self.upcoming_row(ui, i, &queue[i], &mut drop_at, acts);
            }
        });
        self.finish_drag(ui, drop_at, acts);
    }

    fn queue_title(&mut self, ui: &mut Ui, can_clear: bool, queue: &[crate::model::Track], acts: &mut Actions) {
        self.side_panel_tabs(ui, acts);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if can_clear && ui.link(RichText::new("clear").font(ty::META.font())).clicked() {
                    acts.push(Action::Player(Cmd::ClearUpcoming));
                }
                let save =
                    ui.link(RichText::new("save as playlist").font(ty::META.font())).on_hover_text("Save the queue as a new playlist");
                if !queue.is_empty() && save.clicked() {
                    acts.push(Action::NewPlaylist(for_playlist(queue.to_vec())));
                }
            });
        });
    }

    /// queue | lyrics switch at the top of the right panel, and a close button.
    pub(in crate::ui) fn side_panel_tabs(&self, ui: &mut Ui, acts: &mut Actions) {
        let p = colors();
        ui.horizontal(|ui| {
            let (tray, _) = ui.allocate_exact_size(vec2(ui.available_width() - 40.0, 34.0), Sense::hover());
            let corner = CornerRadius::same(radius::CONTROL);
            ui.painter().rect_filled(tray, corner, p.surface);
            ui.painter().rect_stroke(tray, corner, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
            let tabs = [(SidePanel::Queue, "queue"), (SidePanel::Lyrics, "lyrics")];
            let width = (tray.width() - 6.0) / tabs.len() as f32;
            for (i, (panel, label)) in tabs.into_iter().enumerate() {
                let tab =
                    Rect::from_min_size(pos2(tray.left() + 3.0 + i as f32 * width, tray.top() + 3.0), vec2(width, tray.height() - 6.0));
                let resp = ui.interact(tab, ui.id().with(("side-tab", i)), Sense::click());
                let on = self.side_panel == Some(panel);
                let color = match (on, resp.hovered()) {
                    (true, _) => p.bg,
                    (false, true) => p.text,
                    (false, false) => p.dim,
                };
                if on {
                    ui.painter().rect_filled(tab, CornerRadius::same(5), p.text);
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

fn empty_state(ui: &mut Ui) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("Your queue is empty").color(colors().dim).font(ty::TITLE.font()));
        ui.add_space(4.0);
        ui.label(RichText::new("Right-click a track or card → Add to queue").color(colors().faint).font(ty::SMALL.font()));
    });
}
