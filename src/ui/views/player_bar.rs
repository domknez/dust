//! Player bar: now playing on the left, transport and progress in the middle,
//! queue, output and volume on the right.

use super::mini_player::MINI_PLAYER_HINT;
use crate::player::{Cmd, State, Status};
use crate::ui::app::App;
use crate::ui::format::{cover_url, mmss};
use crate::ui::state::Coll;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, Align2, Rect, Ui, UiBuilder, Vec2, pos2, vec2};
use std::time::Duration;

/// Share of the bar width for each side column (capped).
const SIDE_SHARE: f32 = 0.3;
const SIDE_MAX: f32 = 360.0;
const PROGRESS_MAX_WIDTH: f32 = 560.0;
const VOLUME_WIDTH: f32 = 104.0;
/// Progress refresh while playing.
pub(in crate::ui) const TICK: Duration = Duration::from_millis(500);

impl App {
    pub(in crate::ui) fn player_bar(&mut self, ui: &mut Ui, st: &Status) {
        let full = ui.max_rect();
        ui.painter().hline(full.x_range(), full.top(), egui::Stroke::new(1.0, colors().line));
        let side = (full.width() * SIDE_SHARE).min(SIDE_MAX);
        let left = Rect::from_min_max(full.min, pos2(full.left() + side, full.bottom()));
        let right = Rect::from_min_max(pos2(full.right() - side, full.top()), full.max);
        let center = Rect::from_min_max(pos2(left.right() + 16.0, full.top()), pos2(right.left() - 16.0, full.bottom()));

        self.now_playing(ui, left, st);
        ui.scope_builder(UiBuilder::new().max_rect(center), |ui| {
            self.transport(ui, center, st);
            self.progress(ui, center, st);
        });
        ui.scope_builder(UiBuilder::new().max_rect(right).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            self.volume(ui, st);
            ui.add_space(10.0);
            self.output_button(ui, &st.output);
            let color = if self.show_queue { colors().accent } else { colors().dim };
            if widgets::icon_button(ui, Icon::Queue, 18.0, color).on_hover_text("Queue").clicked() {
                self.show_queue = !self.show_queue;
            }
            if widgets::icon_button(ui, Icon::MiniPlayer, 18.0, colors().dim).on_hover_text(MINI_PLAYER_HINT).clicked() {
                self.toggle_mini_player(ui.ctx());
            }
        });
        if st.state == State::Playing {
            ui.ctx().request_repaint_after(TICK);
        }
    }

    fn now_playing(&mut self, ui: &mut Ui, area: Rect, st: &Status) {
        let art_size = metrics::NOW_PLAYING_ART;
        if let Some(track) = &st.track {
            let art = Rect::from_min_size(pos2(area.left(), area.center().y - art_size / 2.0), Vec2::splat(art_size));
            widgets::cover(ui, &mut self.covers, cover_url(track, metrics::NOW_PLAYING_PX).as_deref(), art, radius::THUMB);
            let x = art.right() + 14.0;
            let width = area.right() - x;
            // Cover and title open the album.
            let album = Coll::album_of(track);
            let cover = ui.interact(art, ui.id().with("now-playing-cover"), egui::Sense::click());
            let title_pos = pos2(x, area.center().y - 9.0);
            let title_clicked = match &album {
                Some(_) => {
                    widgets::text_link(
                        ui,
                        "now-playing-title",
                        title_pos,
                        &track.title,
                        ty::TITLE.strong(),
                        colors().text,
                        colors().text,
                        width,
                    )
                    .0
                }
                None => {
                    text_left(ui.painter(), title_pos, &track.title, ty::TITLE.strong(), colors().text, width);
                    false
                }
            };
            if album.is_some() && (title_clicked || cover.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()) {
                self.pending_album = album;
            }
            let pos = pos2(x, area.center().y + 11.0);
            if let (Some(artist), _) =
                widgets::artist_links(ui, pos, &track.artists, &track.artist, ty::SECONDARY, colors().dim, colors().text, width)
            {
                self.pending_artist = Some(artist);
            }
        }
        if st.state == State::Loading {
            let spot = Rect::from_center_size(pos2(area.left() + art_size / 2.0, area.center().y), Vec2::splat(art_size));
            ui.put(spot, egui::Spinner::new().size(20.0));
        }
    }

    pub(in crate::ui) fn transport(&mut self, ui: &mut Ui, area: Rect, st: &Status) {
        let controls = Rect::from_center_size(pos2(area.center().x, area.top() + 30.0), vec2(160.0, 40.0));
        self.transport_buttons(ui, controls, st);
    }

    /// Previous, play/pause and next, laid out left to right in `controls`.
    pub(in crate::ui) fn transport_buttons(&mut self, ui: &mut Ui, controls: Rect, st: &Status) {
        ui.scope_builder(UiBuilder::new().max_rect(controls).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if widgets::icon_button(ui, Icon::Prev, 16.0, colors().dim).clicked() {
                self.player.send(Cmd::Prev);
            }
            if widgets::play_circle(ui, st.state == State::Playing, 38.0).clicked() {
                self.player.send(Cmd::Toggle);
            }
            if widgets::icon_button(ui, Icon::Next, 16.0, colors().dim).clicked() {
                self.player.send(Cmd::Next);
            }
        });
    }

    fn progress(&mut self, ui: &mut Ui, area: Rect, st: &Status) {
        let duration = st.track.as_ref().map_or(0.0, |t| t.duration as f64);
        let row = Rect::from_center_size(pos2(area.center().x, area.top() + 64.0), vec2(area.width().min(PROGRESS_MAX_WIDTH), 16.0));
        let position = self.seek_drag.map(|f| f as f64 * duration).unwrap_or(st.position).min(duration);
        let (font, color) = (ty::CAPTION.font(), colors().dim);
        ui.painter().text(pos2(row.left(), row.center().y), Align2::RIGHT_CENTER, mmss(position), font.clone(), color);
        ui.painter().text(pos2(row.right(), row.center().y), Align2::LEFT_CENTER, mmss(duration), font, color);
        let slider = Rect::from_min_max(pos2(row.left() + 10.0, row.top()), pos2(row.right() - 10.0, row.bottom()));
        self.seek_slider(ui, slider, st);
    }

    /// Click or drag to seek; the position follows the drag until release.
    pub(in crate::ui) fn seek_slider(&mut self, ui: &mut Ui, slider: Rect, st: &Status) {
        let duration = st.track.as_ref().map_or(0.0, |t| t.duration as f64);
        let position = self.seek_drag.map(|f| f as f64 * duration).unwrap_or(st.position).min(duration);
        ui.scope_builder(UiBuilder::new().max_rect(slider), |ui| {
            let mut fraction = if duration > 0.0 { (position / duration) as f32 } else { 0.0 };
            let resp = widgets::thin_slider(ui, &mut fraction, slider.width(), duration > 0.0);
            if resp.dragged() {
                self.seek_drag = Some(fraction);
            }
            if resp.drag_stopped() || (resp.clicked() && !resp.dragged()) {
                self.player.send(Cmd::Seek(fraction as f64 * duration));
                self.seek_drag = None;
            }
        });
    }

    fn volume(&mut self, ui: &mut Ui, st: &Status) {
        let mut volume = self.volume_drag.unwrap_or(st.volume);
        let resp = widgets::thin_slider(ui, &mut volume, VOLUME_WIDTH, true);
        if resp.changed() {
            self.volume_drag = Some(volume);
            self.player.send(Cmd::Volume(volume));
        }
        if resp.drag_stopped() || (!resp.dragged() && self.volume_drag.is_some() && !resp.hovered()) {
            self.volume_drag = None;
        }
        let muted = volume <= 0.001;
        if widgets::icon_button(ui, if muted { Icon::Mute } else { Icon::Volume }, 18.0, colors().dim).on_hover_text("Mute").clicked() {
            if muted {
                self.player.send(Cmd::Volume(self.unmuted_volume.max(0.1)));
            } else {
                self.unmuted_volume = volume;
                self.player.send(Cmd::Volume(0.0));
            }
        }
    }
}
