//! The *crate* tab of the panel on the right: save what's playing, jot a name
//! down, tick off what's been heard.

use super::*;

impl App {
    pub(in crate::ui) fn crate_panel(&mut self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        ui.add_space(metrics::TOP_INSET);
        self.side_panel_tabs(ui, acts);
        ui.add_space(4.0);
        self.save_playing_row(ui, st, acts);
        ui.add_space(8.0);
        if let Some(note) = jot_field(ui, "crate-tab-jot", &mut self.crates.view.tab_jot) {
            acts.push(Action::CrateNote(note));
        }
        ui.add_space(14.0);
        let fresh: Vec<Entry> = self.crates.store.entries().iter().filter(|e| !e.heard).cloned().collect();
        let heard = self.crates.store.entries().len() - fresh.len();
        let p = colors();
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
        text_left(ui.painter(), pos2(row.left() + 4.0, row.center().y), &format!("NEW · {}", fresh.len()), ty::OVERLINE, p.faint, 120.0);
        if self.crate_playable() > 0 {
            let label = "play the new ones";
            let galley = ui.painter().layout_no_wrap(label.into(), ty::CAPTION.font(), p.dim);
            let width = galley.size().x + 16.0;
            let area = Rect::from_min_size(pos2(row.right() - 4.0 - width, row.top()), vec2(width, row.height()));
            let resp = ui.interact(area, ui.scope_id().with("crate-play-new"), Sense::click());
            let color = if resp.hovered() { p.accent } else { p.dim };
            let icon = Rect::from_center_size(pos2(area.left() + 5.0, row.center().y), Vec2::splat(9.0));
            icons::paint(ui.painter(), icon, Icon::Play, color);
            ui.painter().text(pos2(area.right(), row.center().y), Align2::RIGHT_CENTER, label, ty::CAPTION.font(), color);
            let hint = format!("The new tracks; {EACH} from each new album and artist. Tick each when you've heard it");
            if resp.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                acts.push(Action::PlayCrate);
            }
        }
        let current = self.client.as_ref().map(|c| c.id().key).unwrap_or_default();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if fresh.is_empty() {
                widgets::empty_panel(
                    ui,
                    "Nothing new to check out",
                    &format!("{} saves what's playing", shortcuts::label(&shortcuts::SAVE_TO_CRATE)),
                );
            }
            for entry in &fresh {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let hovered = ui.rect_contains_pointer(rect);
                if hovered {
                    ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
                }
                let tick = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(26.0));
                entry_row(ui, &mut self.covers, rect.with_max_x(tick.left() - 4.0), entry, current);
                let over_tick = resp.hover_pos().is_some_and(|pos| tick.contains(pos));
                tick_box(ui, tick, over_tick);
                if resp.clicked() {
                    acts.push(if over_tick {
                        Action::Crate(Change::Heard { id: entry.id.clone(), heard: true })
                    } else {
                        Action::OpenCrate(Some(entry.id.clone()), None)
                    });
                }
                acts.extend(entry_menu(&resp, entry));
                resp.on_hover_text(if over_tick { "Mark as heard" } else { "Open in your Crate" })
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
            }
            if heard > 0 {
                ui.add_space(8.0);
                let label = RichText::new(format!("heard · {heard} ›")).font(ty::META.font());
                if ui.link(label).clicked() {
                    acts.push(Action::OpenCrate(None, Some(Filter::Heard)));
                }
            }
            ui.add_space(10.0);
        });
    }

    /// "Save what's playing", with the track and the shortcut; "In your Crate" once
    /// it's there.
    fn save_playing_row(&self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        let p = colors();
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click());
        let track = st.track.as_ref().filter(|t| !t.is_episode());
        let source = self.client.as_ref().map(|c| c.id().key).unwrap_or_default();
        let saved = track.and_then(|t| self.crates.store.find(source, &Thing::Track(t.clone()))).is_some_and(|e| !e.heard);
        let enabled = track.is_some() && !saved;
        let corner = CornerRadius::same(radius::CONTROL);
        ui.painter().rect_filled(rect, corner, if enabled && resp.hovered() { p.raised } else { p.surface });
        ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        let icon = Rect::from_center_size(pos2(rect.left() + 24.0, rect.center().y), Vec2::splat(18.0));
        let (icon_kind, icon_color) = if saved { (Icon::Check, p.accent) } else { (Icon::Crate, if enabled { p.accent } else { p.faint }) };
        icons::paint(ui.painter(), icon, icon_kind, icon_color);
        let x = icon.right() + 14.0;
        let width = rect.right() - 64.0 - x;
        let title = if saved { "In your Crate" } else { "Save what's playing" };
        text_left(
            ui.painter(),
            pos2(x, rect.center().y - 8.0),
            title,
            ty::ITEM.strong(),
            if enabled || saved { p.text } else { p.dim },
            width,
        );
        let line = track.map_or_else(|| "Nothing playing".to_string(), |t| format!("{} · {}", t.title, t.artist));
        text_left(ui.painter(), pos2(x, rect.center().y + 10.0), &line, ty::SMALL, p.dim, width);
        ui.painter().text(
            pos2(rect.right() - 12.0, rect.center().y),
            Align2::RIGHT_CENTER,
            shortcuts::label(&shortcuts::SAVE_TO_CRATE),
            ty::META.font(),
            p.faint,
        );
        if enabled && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            acts.extend(self.save_playing(st));
        }
    }

    // ------------------------------------------------------------ the page
}
