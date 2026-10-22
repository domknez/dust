//! Lyrics panel: the playing track's lyrics, the sung line highlighted and kept in
//! view when they're synced; click a line to jump there.

use crate::deezer::Lyrics;
use crate::player::{Cmd, State, Status};
use crate::ui::app::App;
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, metrics, typography as ty};
use crate::ui::tasks::{self, Task};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, RichText, Sense, Ui, pos2, vec2};
use std::time::Duration;

/// Repaint at least this often while synced lyrics play.
const MAX_WAIT: Duration = Duration::from_millis(500);

/// Lyrics of one track: loading, loaded (maybe none) or failed.
#[derive(Default)]
pub struct LyricsState {
    /// The track these are for.
    track: Option<u64>,
    task: Task<Option<Lyrics>>,
    loaded: Option<Result<Option<Lyrics>, String>>,
    /// The line last scrolled into view, so it scrolls only when the line changes.
    scrolled_to: Option<usize>,
}

impl LyricsState {
    /// The panel was opened again: bring the sung line back into view.
    pub fn reopened(&mut self) {
        self.scrolled_to = None;
    }
}

impl App {
    pub(in crate::ui) fn lyrics_panel(&mut self, ui: &mut Ui, st: &Status) {
        ui.add_space(metrics::TOP_INSET);
        self.lyrics_title(ui);
        let Some(track) = &st.track else {
            hint(ui, "Play something to see its lyrics");
            return;
        };
        if track.is_episode() {
            hint(ui, "Podcasts don't have lyrics");
            return;
        }
        self.load_lyrics(ui.ctx(), track.id);
        match &self.lyrics.loaded {
            None => hint(ui, "Loading lyrics…"),
            Some(Err(e)) => hint(ui, &format!("Couldn't load lyrics: {e}")),
            Some(Ok(None)) => hint(ui, "No lyrics for this track"),
            Some(Ok(Some(lyrics))) => {
                let lyrics = lyrics.clone();
                if lyrics.synced.is_empty() {
                    plain(ui, &lyrics);
                } else {
                    self.synced(ui, st, &lyrics);
                }
            }
        }
    }

    fn lyrics_title(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width() - 40.0, 36.0), Sense::hover());
            text_left(ui.painter(), pos2(r.left() + 4.0, r.center().y), "Lyrics", ty::PANEL_TITLE, colors().text, r.width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, Icon::Close, 14.0, colors().dim).on_hover_text("Close").clicked() {
                    self.side_panel = None;
                }
            });
        });
        ui.add_space(8.0);
    }

    /// Fetch the lyrics of `track` unless they're loaded or loading.
    fn load_lyrics(&mut self, ctx: &egui::Context, track: u64) {
        let state = &mut self.lyrics;
        if let Some(result) = tasks::poll(&mut state.task) {
            state.loaded = Some(result.map_err(|e| e.to_string()));
        }
        if state.track == Some(track) {
            return;
        }
        let Some(client) = self.client.clone() else { return };
        *state = LyricsState { track: Some(track), task: tasks::spawn(ctx, move || client.lyrics(track)), ..Default::default() };
    }

    fn synced(&mut self, ui: &mut Ui, st: &Status, lyrics: &Lyrics) {
        let ms = (st.position * 1000.0) as u32;
        let current = lyrics.line_at(ms);
        let p = colors();
        let mut seek = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            for (i, line) in lyrics.synced.iter().enumerate() {
                let now = current == Some(i);
                let color = if now {
                    p.text
                } else if current.is_some_and(|c| i < c) {
                    p.faint
                } else {
                    p.dim
                };
                let text = if line.text.trim().is_empty() { "…" } else { &line.text };
                let font = if now { ty::SECTION_TITLE.font() } else { ty::LEAD.strong().font() };
                let resp = ui.add(egui::Label::new(RichText::new(text).font(font).color(color)).wrap().sense(Sense::click()));
                if resp.clicked() {
                    seek = Some(line.at_ms);
                }
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                if now && self.lyrics.scrolled_to != Some(i) {
                    resp.scroll_to_me_animation(Some(egui::Align::Center), egui::style::ScrollAnimation::duration(0.3));
                    self.lyrics.scrolled_to = Some(i);
                }
            }
            credits(ui, lyrics);
        });
        if let Some(at) = seek {
            self.player.send(Cmd::Seek(at as f64 / 1000.0));
        }
        // Wake up for the next line rather than on a fixed tick.
        if st.state == State::Playing {
            let next = lyrics.synced.get(current.map_or(0, |c| c + 1)).map(|l| l.at_ms.saturating_sub(ms));
            let wait = next.map_or(MAX_WAIT, |ms| Duration::from_millis(ms as u64).min(MAX_WAIT));
            ui.ctx().request_repaint_after(wait.max(Duration::from_millis(30)));
        }
    }
}

fn plain(ui: &mut Ui, lyrics: &Lyrics) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        for line in &lyrics.plain {
            if line.trim().is_empty() {
                ui.add_space(10.0);
            } else {
                ui.add(egui::Label::new(RichText::new(line).font(ty::LEAD.font()).color(colors().text)).wrap());
            }
        }
        credits(ui, lyrics);
    });
}

fn credits(ui: &mut Ui, lyrics: &Lyrics) {
    ui.add_space(24.0);
    let small = |ui: &mut Ui, text: String| {
        ui.add(egui::Label::new(RichText::new(text).font(ty::CAPTION.font()).color(colors().faint)).wrap());
    };
    if !lyrics.writers.is_empty() {
        small(ui, format!("Written by {}", lyrics.writers));
    }
    if !lyrics.copyright.is_empty() {
        small(ui, lyrics.copyright.clone());
    }
    ui.add_space(16.0);
}

fn hint(ui: &mut Ui, text: &str) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(text).color(colors().dim).font(ty::TITLE.font()));
    });
}
