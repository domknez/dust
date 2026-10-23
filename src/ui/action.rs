//! What the listener did. Views push [`Action`]s into [`Actions`] while they draw;
//! once the frame is drawn, [`App::update`] applies them: the one place that
//! navigates, decides what to play and calls the music source.
//!
//! State that belongs to a single widget (text being typed, a slider being dragged,
//! a page's open tab) is still changed in place while drawing.

use super::app::{App, SidePanel};
use super::state::{ArtistView, Coll};
use super::views::SleepChoice;
use crate::deezer::{ArtistRef, Likeable, Track};
use crate::player::{Cmd, Output, Repeat};
use eframe::egui;

pub enum Action {
    // Pages
    OpenArtist(ArtistRef),
    /// A collection's tracks; an artist gets their page.
    OpenCollection(Coll),

    // Playback
    /// Anything else for the player: pause, skip, seek, volume, queue edits.
    Player(Cmd),
    Shuffle(bool),
    Repeat(Repeat),
    /// Set the sleep timer, or turn it off.
    Sleep(Option<SleepChoice>),
    Output(Output),
    /// Look for speakers again (the output menu opened).
    FindSpeakers,

    // Library
    ToggleLike(Likeable),
    /// Ask for a name, then create a playlist holding these tracks.
    NewPlaylist(Vec<Track>),

    // Window
    /// Open a panel on the right, or close it if it's the one showing.
    TogglePanel(SidePanel),
    /// Show this panel on the right; None closes it.
    ShowPanel(Option<SidePanel>),
    ToggleMiniPlayer,
}

impl Action {
    /// Whether it shows a page (in the full window, so the mini player gives way).
    fn opens_page(&self) -> bool {
        matches!(self, Action::OpenArtist(_) | Action::OpenCollection(_))
    }
}

/// What was done this frame, in order.
#[derive(Default)]
pub struct Actions(Vec<Action>);

impl Actions {
    pub fn push(&mut self, action: Action) {
        self.0.push(action);
    }
}

impl Extend<Action> for Actions {
    fn extend<I: IntoIterator<Item = Action>>(&mut self, actions: I) {
        self.0.extend(actions);
    }
}

impl App {
    /// Apply the frame's actions, then draw again to show what they changed.
    pub(super) fn apply(&mut self, ctx: &egui::Context, actions: Actions) {
        if actions.0.is_empty() {
            return;
        }
        for action in actions.0 {
            self.update(ctx, action);
        }
        ctx.request_repaint();
    }

    /// Apply one thing the listener did.
    pub(super) fn update(&mut self, ctx: &egui::Context, action: Action) {
        // Opening something from the mini player shows it in the full window.
        if self.mini_player.is_some() && action.opens_page() {
            self.toggle_mini_player(ctx);
        }
        match action {
            Action::OpenArtist(artist) => self.open_artist(ctx, ArtistView::from_ref(&artist)),
            Action::OpenCollection(coll) => self.open_collection(ctx, coll),
            Action::Player(cmd) => self.player.send(cmd),
            Action::Shuffle(on) => self.set_shuffle(on),
            Action::Repeat(repeat) => self.set_repeat(repeat),
            Action::Sleep(choice) => self.set_sleep(choice),
            Action::Output(output) => self.select_output(output),
            Action::FindSpeakers => {
                if let Some(discovery) = &self.discovery {
                    discovery.refresh();
                }
            }
            Action::ToggleLike(what) => self.toggle_like(ctx, what),
            Action::NewPlaylist(tracks) => self.new_playlist(tracks),
            Action::TogglePanel(panel) => self.toggle_side_panel(panel),
            Action::ShowPanel(panel) => {
                self.side_panel = panel;
                self.lyrics.reopened();
            }
            Action::ToggleMiniPlayer => self.toggle_mini_player(ctx),
        }
    }
}
