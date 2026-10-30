//! Mini player: the same window shrunk to a borderless, always-on-top strip with
//! the cover, the track and where it plays, transport and a progress line along the
//! bottom. Drag anywhere to move it; narrow, it keeps only the cover, title, times
//! and play (previous / next come back on hover).

use super::player_bar::{TICK, transport_buttons};
use crate::player::{Cmd, Output, State, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::format::mmss;
use crate::ui::state::Coll;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{
    self, CornerRadius, KeyboardShortcut, Modifiers, Rect, Sense, Stroke, Ui, UiBuilder, Vec2, ViewportCommand, WindowLevel, pos2, vec2,
};

/// The mini player shortcut, as menus show it.
pub(in crate::ui) const MINI_PLAYER_KEYS: &str = if cfg!(target_os = "macos") { "⌘⇧M" } else { "Ctrl+Shift+M" };
const FULL_WINDOW_HINT: &str = if cfg!(target_os = "macos") { "Full window (⌘⇧M)" } else { "Full window (Ctrl+Shift+M)" };
const TOGGLE_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), egui::Key::M);

const PAD: f32 = 10.0;
/// Controls width: previous, play/pause, next.
const TRANSPORT: f32 = 116.0;
const PLAY: f32 = 40.0;

impl App {
    /// Switch between the full window and the mini player, restoring the full
    /// window's size on the way back.
    pub(in crate::ui) fn toggle_mini_player(&mut self, ctx: &egui::Context) {
        let commands = match self.mini_player.take() {
            Some(full_size) => vec![
                ViewportCommand::Decorations(true),
                ViewportCommand::WindowLevel(WindowLevel::Normal),
                ViewportCommand::Resizable(true),
                ViewportCommand::MaxInnerSize(Vec2::splat(100_000.0)),
                ViewportCommand::MinInnerSize(metrics::MIN_WINDOW_SIZE.into()),
                ViewportCommand::InnerSize(full_size),
            ],
            None => {
                let size = ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()));
                self.mini_player = Some(size.unwrap_or(metrics::WINDOW_SIZE.into()));
                let [width, height] = metrics::MINI_PLAYER_SIZE;
                vec![
                    ViewportCommand::Decorations(false),
                    ViewportCommand::MinInnerSize(vec2(metrics::MINI_PLAYER_MIN_WIDTH, height)),
                    ViewportCommand::MaxInnerSize(vec2(metrics::MINI_PLAYER_MAX_WIDTH, height)),
                    ViewportCommand::InnerSize(vec2(width, height)),
                    ViewportCommand::Resizable(true),
                    ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop),
                ]
            }
        };
        commands.into_iter().for_each(|c| ctx.send_viewport_cmd(c));
    }

    /// ⌘⇧M / Ctrl+Shift+M switches modes.
    pub(in crate::ui) fn mini_player_shortcut(&self, ctx: &egui::Context, acts: &mut Actions) {
        if ctx.input_mut(|i| i.consume_shortcut(&TOGGLE_SHORTCUT)) {
            acts.push(Action::ToggleMiniPlayer);
        }
    }

    pub(in crate::ui) fn mini_player(&mut self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        let full = ui.max_rect();
        let p = colors();
        // Registered first so every control drawn on top of it wins the pointer.
        let background = ui.interact(full, ui.scope_id().with("mini-drag"), Sense::click_and_drag());
        if background.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        ui.painter().rect_stroke(full, CornerRadius::ZERO, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);

        let compact = full.width() < metrics::MINI_PLAYER_COMPACT;
        let hovered = ui.rect_contains_pointer(full);
        let cy = full.center().y - 1.0;
        let art = Rect::from_min_size(pos2(full.left() + PAD, cy - metrics::MINI_PLAYER_ART / 2.0), Vec2::splat(metrics::MINI_PLAYER_ART));
        let mut right = full.right() - 12.0;
        if !compact {
            let expand = Rect::from_center_size(pos2(right - 14.0, cy), Vec2::splat(28.0));
            ui.scope_builder(UiBuilder::new().max_rect(expand), |ui| {
                if widgets::icon_button(ui, Icon::Expand, 14.0, p.dim).on_hover_text(FULL_WINDOW_HINT).clicked() {
                    acts.push(Action::ToggleMiniPlayer);
                }
            });
            right = expand.left() - 8.0;
        }
        // Narrow: just play, with previous / next while the pointer is over the strip.
        let skip = !compact || hovered;
        let controls = Rect::from_min_max(pos2(right - if skip { TRANSPORT } else { PLAY }, cy - 20.0), pos2(right, cy + 20.0));
        if skip {
            transport_buttons(ui, controls, st, false, acts);
        } else {
            ui.scope_builder(UiBuilder::new().max_rect(controls), |ui| {
                if widgets::play_button(ui, st.state == State::Playing, PLAY).clicked() {
                    acts.push(Action::Player(Cmd::Toggle));
                }
            });
        }

        let x = art.right() + 12.0;
        let width = controls.left() - 12.0 - x;
        match &st.track {
            Some(track) => {
                widgets::cover(ui, &mut self.covers, track.art.as_ref(), metrics::NOW_PLAYING_PX, art, radius::ROW);
                let title_pos = pos2(x, cy - 9.0);
                match Coll::origin_of(track) {
                    // The title opens the album (or podcast) in the full window.
                    Some(album) => {
                        if widgets::text_link(ui, "mini-title", title_pos, &track.title, ty::TITLE.strong(), p.text, p.text, width)
                            .0
                            .clicked()
                        {
                            acts.push(Action::OpenCollection(album));
                        }
                    }
                    None => {
                        text_left(ui.painter(), title_pos, &track.title, ty::TITLE.strong(), p.text, width);
                    }
                }
                let line = pos2(x, cy + 10.0);
                if compact {
                    let times = format!("{} / {}", mmss(st.position), mmss(track.duration as f64));
                    text_left(ui.painter(), line, &times, ty::META, p.dim, width);
                } else {
                    // "Artist · on Office" while a speaker plays.
                    let (artist, end) = widgets::artist_links(ui, line, &track.artists, &track.artist, ty::SECONDARY, p.dim, p.text, width);
                    acts.extend(artist.map(Action::OpenArtist));
                    if st.output != Output::Local.name() {
                        let on = format!(" · on {}", st.output);
                        text_left(ui.painter(), pos2(end, line.y), &on, ty::SECONDARY, p.dim, x + width - end);
                    }
                }
            }
            None => {
                widgets::tile(ui, art, Icon::Note, p.surface, radius::ROW);
                text_left(ui.painter(), pos2(x, cy), "Nothing playing", ty::SECONDARY, p.dim, width);
            }
        }
        if st.state == State::Loading {
            ui.put(art, egui::Spinner::new().size(18.0));
        }
        let art_click = ui.interact(art, ui.scope_id().with("mini-art"), Sense::click()).on_hover_text("Double-click for the full window");
        if art_click.double_clicked() {
            acts.push(Action::ToggleMiniPlayer);
        }
        self.progress_line(ui, full, st, acts);

        if st.state == State::Playing {
            ui.ctx().request_repaint_after(TICK);
        }
    }

    /// Lime line along the bottom edge; click or drag along it to seek.
    fn progress_line(&mut self, ui: &mut Ui, full: Rect, st: &Status, acts: &mut Actions) {
        let p = colors();
        let duration = st.track.as_ref().map_or(0.0, |t| t.duration as f64);
        let strip = Rect::from_min_max(pos2(full.left(), full.bottom() - 8.0), full.max);
        let resp = ui.interact(strip, ui.scope_id().with("mini-seek"), Sense::click_and_drag());
        let pointer = resp.interact_pointer_pos().map(|pt| ((pt.x - full.left()) / full.width()).clamp(0.0, 1.0));
        if duration > 0.0
            && resp.dragged()
            && let Some(f) = pointer
        {
            self.seek_drag = Some(f);
        }
        if duration > 0.0
            && (resp.drag_stopped() || resp.clicked())
            && let Some(f) = pointer.or(self.seek_drag)
        {
            acts.push(Action::Player(Cmd::Seek(f as f64 * duration)));
            self.seek_drag = None;
        }
        let shown = match self.seek_drag {
            Some(f) => f,
            None if duration > 0.0 => (st.position / duration).clamp(0.0, 1.0) as f32,
            None => 0.0,
        };
        let height = if resp.hovered() || resp.dragged() { 4.0 } else { 3.0 };
        let line = Rect::from_min_max(pos2(full.left(), full.bottom() - height), full.max);
        ui.painter().rect_filled(line, CornerRadius::ZERO, p.line);
        ui.painter().rect_filled(Rect::from_min_size(line.min, vec2(line.width() * shown, height)), CornerRadius::ZERO, p.accent);
        if duration > 0.0 {
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
    }
}
