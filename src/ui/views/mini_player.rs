//! Mini player: the same window shrunk to a small always-on-top strip with the
//! cover, track, transport and a seek bar. Drag anywhere to move it.

use super::player_bar::TICK;
use crate::player::{State, Status};
use crate::ui::app::App;
use crate::ui::format::cover_url;
use crate::ui::state::Coll;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, KeyboardShortcut, Modifiers, Rect, Sense, Ui, Vec2, ViewportCommand, WindowLevel, pos2};

/// Hover text for the buttons that switch modes.
pub(in crate::ui) const MINI_PLAYER_HINT: &str =
    if cfg!(target_os = "macos") { "Mini player (⌘⇧M)" } else { "Mini player (Ctrl+Shift+M)" };
const FULL_WINDOW_HINT: &str = if cfg!(target_os = "macos") { "Full window (⌘⇧M)" } else { "Full window (Ctrl+Shift+M)" };
const TOGGLE_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), egui::Key::M);

const PAD: f32 = 16.0;
/// Controls width: previous, play/pause, next.
const TRANSPORT: f32 = 112.0;

impl App {
    /// Switch between the full window and the mini player, restoring the full
    /// window's size on the way back.
    pub(in crate::ui) fn toggle_mini_player(&mut self, ctx: &egui::Context) {
        let commands = match self.mini_player.take() {
            Some(full_size) => vec![
                ViewportCommand::WindowLevel(WindowLevel::Normal),
                ViewportCommand::Resizable(true),
                ViewportCommand::MinInnerSize(metrics::MIN_WINDOW_SIZE.into()),
                ViewportCommand::InnerSize(full_size),
            ],
            None => {
                let size = ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()));
                self.mini_player = Some(size.unwrap_or(metrics::WINDOW_SIZE.into()));
                let mini = Vec2::from(metrics::MINI_PLAYER_SIZE);
                vec![
                    ViewportCommand::MinInnerSize(mini),
                    ViewportCommand::InnerSize(mini),
                    ViewportCommand::Resizable(false),
                    ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop),
                ]
            }
        };
        commands.into_iter().for_each(|c| ctx.send_viewport_cmd(c));
    }

    /// ⌘⇧M / Ctrl+Shift+M switches modes.
    pub(in crate::ui) fn mini_player_shortcut(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&TOGGLE_SHORTCUT)) {
            self.toggle_mini_player(ctx);
        }
    }

    pub(in crate::ui) fn mini_player(&mut self, ui: &mut Ui, st: &Status) {
        let full = ui.max_rect();
        let p = colors();
        // Registered first so every control drawn on top of it wins the pointer.
        let background = ui.interact(full, ui.id().with("mini-drag"), Sense::click_and_drag());
        if background.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }

        let top = full.top() + metrics::MINI_PLAYER_INSET;
        let art = Rect::from_min_size(pos2(full.left() + PAD, top), Vec2::splat(metrics::MINI_PLAYER_ART));
        let expand = Rect::from_center_size(pos2(full.right() - PAD - 12.0, art.center().y), Vec2::splat(28.0));
        let controls = Rect::from_min_max(pos2(expand.left() - 8.0 - TRANSPORT, art.top()), pos2(expand.left() - 8.0, art.bottom()));

        match &st.track {
            Some(track) => {
                widgets::cover(ui, &mut self.covers, cover_url(track, metrics::NOW_PLAYING_PX).as_deref(), art, radius::THUMB);
                let x = art.right() + 12.0;
                let width = controls.left() - 8.0 - x;
                let title_pos = pos2(x, art.center().y - 10.0);
                match Coll::album_of(track) {
                    // The title opens the album in the full window.
                    Some(album) => {
                        if widgets::text_link(ui, "mini-title", title_pos, &track.title, ty::TITLE.strong(), p.text, p.text, width).0 {
                            self.pending_album = Some(album);
                        }
                    }
                    None => {
                        text_left(ui.painter(), title_pos, &track.title, ty::TITLE.strong(), p.text, width);
                    }
                }
                let pos = pos2(x, art.center().y + 10.0);
                if let (Some(artist), _) =
                    widgets::artist_links(ui, pos, &track.artists, &track.artist, ty::SECONDARY, p.dim, p.text, width)
                {
                    self.pending_artist = Some(artist);
                }
            }
            None => {
                widgets::tile(ui, art, Icon::Note, p.surface, radius::THUMB);
                text_left(ui.painter(), pos2(art.right() + 12.0, art.center().y), "Nothing playing", ty::SECONDARY, p.dim, 140.0);
            }
        }
        if st.state == State::Loading {
            ui.put(art, egui::Spinner::new().size(18.0));
        }
        let art_click = ui.interact(art, ui.id().with("mini-art"), Sense::click()).on_hover_text("Double-click for the full window");
        if art_click.double_clicked() {
            self.toggle_mini_player(ui.ctx());
        }

        self.transport_buttons(ui, controls, st);
        ui.scope_builder(egui::UiBuilder::new().max_rect(expand), |ui| {
            if widgets::icon_button(ui, Icon::Expand, 14.0, p.dim).on_hover_text(FULL_WINDOW_HINT).clicked() {
                self.toggle_mini_player(ui.ctx());
            }
        });

        let seek = Rect::from_min_max(pos2(full.left() + PAD, art.bottom() + 8.0), pos2(full.right() - PAD, art.bottom() + 24.0));
        self.seek_slider(ui, seek, st);

        if st.state == State::Playing {
            ui.ctx().request_repaint_after(TICK);
        }
    }
}
