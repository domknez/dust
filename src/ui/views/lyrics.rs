//! Lyrics panel: the playing track's lyrics, the sung line highlighted and kept in
//! view when they're synced; click a line to jump there.

use crate::deezer::Lyrics;
use crate::player::{Cmd, State, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::load::Load;
use crate::ui::style::{colors, metrics, typography as ty};

use eframe::egui::{self, RichText, Sense, Ui};
use std::time::Duration;

/// Repaint at least this often while synced lyrics play.
const MAX_WAIT: Duration = Duration::from_millis(500);

/// Lyrics of one track: loading, loaded (maybe none) or failed.
#[derive(Default)]
pub struct LyricsState {
    /// The track these are for.
    track: Option<u64>,
    loaded: Load<Option<Lyrics>>,
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
    pub(in crate::ui) fn lyrics_panel(&mut self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        ui.add_space(metrics::TOP_INSET);
        self.side_panel_tabs(ui, acts);
        ui.add_space(8.0);
        let Some(track) = &st.track else {
            hint(ui, "Play something to see its lyrics");
            return;
        };
        if track.is_episode() {
            hint(ui, "Podcasts don't have lyrics");
            return;
        }
        self.load_lyrics(track.id);
        let LyricsState { loaded, scrolled_to, .. } = &mut self.lyrics;
        match loaded {
            Load::Idle | Load::Loading(_) => hint(ui, "Loading lyrics…"),
            Load::Failed(e) => hint(ui, &format!("Couldn't load lyrics: {e}")),
            Load::Ready(None) => hint(ui, "No lyrics for this track"),
            Load::Ready(Some(lyrics)) if lyrics.synced.is_empty() => plain(ui, lyrics),
            Load::Ready(Some(lyrics)) => synced(ui, st, lyrics, scrolled_to, acts),
        }
    }

    /// Fetch the lyrics of `track` unless they're loaded or loading.
    fn load_lyrics(&mut self, track: u64) {
        let state = &mut self.lyrics;
        // Shown in the panel; nothing else to do about an error.
        let _ = state.loaded.poll();
        if state.track == Some(track) {
            return;
        }
        let Some(client) = self.client.clone() else { return };
        *state = LyricsState { track: Some(track), ..Default::default() };
        state.loaded.start(&self.jobs, move || client.lyrics(track));
    }
}

/// Synced lyrics, the sung line lit and kept in view (`scrolled_to`: the line last
/// scrolled to).
fn synced(ui: &mut Ui, st: &Status, lyrics: &Lyrics, scrolled_to: &mut Option<usize>, acts: &mut Actions) {
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
            if now && *scrolled_to != Some(i) {
                resp.scroll_to_me_animation(Some(egui::Align::Center), egui::style::ScrollAnimation::duration(0.3));
                *scrolled_to = Some(i);
            }
        }
        credits(ui, lyrics);
    });
    if let Some(at) = seek {
        acts.push(Action::Player(Cmd::Seek(at as f64 / 1000.0)));
    }
    // Wake up for the next line rather than on a fixed tick.
    if st.state == State::Playing {
        let next = lyrics.synced.get(current.map_or(0, |c| c + 1)).map(|l| l.at_ms.saturating_sub(ms));
        let wait = next.map_or(MAX_WAIT, |ms| Duration::from_millis(ms as u64).min(MAX_WAIT));
        ui.ctx().request_repaint_after(wait.max(Duration::from_millis(30)));
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
