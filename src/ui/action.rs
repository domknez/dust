//! What the listener did. Views push [`Action`]s into [`Actions`] while they draw;
//! once the frame is drawn, [`App::update`] applies them: the one place that
//! navigates, decides what to play and calls the music source.
//!
//! State that belongs to a single widget (text being typed, a slider being dragged,
//! a page's open tab) is still changed in place while drawing.

use super::app::{App, SidePanel};
use super::load::Load;
use super::playlist_edit::{Dialog, Edit};
use super::state::{ArtistView, Coll, PlayMode, Source, View, playable};
use super::style::Appearance;
use super::views::{SleepChoice, reveal_log};
use super::widgets::TrackMenu;
use crate::deezer::{ArtistRef, Likeable, Quality, Track};
use crate::player::{Cmd, Output, Repeat, Sleep};
use crate::provider::Provider;
use crate::subsonic::Server;
use eframe::egui;
use std::time::{Duration, Instant};

pub enum Action {
    // Pages
    /// Go to a page, remembering the current one for Back.
    Open(View),
    /// The Search page, empty (the sidebar's Search).
    OpenSearch,
    Back,
    Forward,
    OpenArtist(ArtistRef),
    /// A collection's tracks; an artist gets their page.
    OpenCollection(Coll),
    /// Load the home page again.
    ReloadHome,

    // Playback
    /// Play `tracks` from `index`, the playable ones only; with `shuffle`, shuffle
    /// goes on and a random one starts.
    PlayFrom {
        tracks: Vec<Track>,
        index: usize,
        shuffle: bool,
    },
    /// A list's "Add to queue": its playable tracks, confirmed on the button.
    QueueAll(Vec<Track>),
    /// Fetch a collection's tracks, then play or queue them.
    PlaySource(Source, PlayMode),
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
    /// A choice in a track's right-click menu.
    TrackMenu(Box<Track>, TrackMenu),
    EditPlaylist(Edit),
    /// Ask for a name, then create a playlist holding these tracks.
    NewPlaylist(Vec<Track>),
    /// Ask before renaming or deleting an own playlist.
    PlaylistDialog(Dialog),

    // Window
    /// Open a panel on the right, or close it if it's the one showing.
    TogglePanel(SidePanel),
    /// Show this panel on the right; None closes it.
    ShowPanel(Option<SidePanel>),
    ToggleMiniPlayer,
    ShowSettings(bool),
    /// Fold the sidebar's playlist list away, or bring it back.
    ToggleSidebarPlaylists,

    // Settings
    Quality(Quality),
    Appearance(Appearance),
    ToggleReportListens,
    AutoUpdate(bool),
    CheckForUpdates,
    /// The sidebar's update button: install and restart, or open the download page.
    InstallUpdate,
    /// Show the log file in the file manager.
    RevealLog,

    // Sources
    /// Log in to Deezer with a pasted ARL.
    LogIn(String),
    /// Log in to Deezer in a browser window.
    #[cfg(feature = "login-window")]
    LogInWithBrowser,
    OpenServerForm,
    ConnectServer(Server),
    SwitchSource(Provider),
    /// Log out of the source in use (or disconnect the server).
    LogOut,
}

impl Action {
    /// Whether it shows a page (in the full window, so the mini player gives way).
    fn opens_page(&self) -> bool {
        matches!(
            self,
            Action::Open(_) | Action::OpenSearch | Action::Back | Action::Forward | Action::OpenArtist(_) | Action::OpenCollection(_)
        )
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
            Action::Open(view) => self.open_view(ctx, view),
            Action::OpenSearch => self.open_search(),
            Action::Back => self.go_back(ctx),
            Action::Forward => self.go_forward(ctx),
            Action::OpenArtist(artist) => self.open_artist(ctx, ArtistView::from_ref(&artist)),
            Action::OpenCollection(coll) => self.open_collection(ctx, coll),
            Action::ReloadHome => {
                self.library.home = Load::Idle;
                self.open_view(ctx, View::Home);
            }
            Action::PlayFrom { tracks, index, shuffle } => self.play_from(tracks, index, shuffle),
            Action::QueueAll(tracks) => {
                self.player.send(Cmd::Enqueue(playable(tracks)));
                self.queued_flash = Some(Instant::now());
            }
            Action::PlaySource(source, mode) => self.play_source(ctx, source, mode),
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
            Action::TrackMenu(track, choice) => self.track_menu_choice(ctx, *track, choice),
            Action::EditPlaylist(edit) => self.edit_playlist(ctx, edit),
            Action::NewPlaylist(tracks) => self.new_playlist(tracks),
            Action::PlaylistDialog(dialog) => self.library.editing.dialog = Some(dialog),
            Action::TogglePanel(panel) => self.toggle_side_panel(panel),
            Action::ShowPanel(panel) => {
                self.side_panel = panel;
                self.lyrics.reopened();
            }
            Action::ToggleMiniPlayer => self.toggle_mini_player(ctx),
            Action::ShowSettings(show) => self.show_settings = show,
            Action::ToggleSidebarPlaylists => self.prefs.update(|p| p.sidebar_playlists = !p.sidebar_playlists),
            Action::Quality(quality) => self.set_quality(quality),
            Action::Appearance(appearance) => self.set_appearance(appearance),
            Action::ToggleReportListens => self.toggle_report_listens(),
            Action::AutoUpdate(on) => {
                self.updates.set_auto(on);
                self.prefs.update(|p| p.auto_update = on);
            }
            Action::CheckForUpdates => self.updates.check_now(ctx),
            Action::InstallUpdate => self.install_update(ctx),
            Action::RevealLog => reveal_log(),
            Action::LogIn(arl) => self.log_in_with(ctx, arl),
            #[cfg(feature = "login-window")]
            Action::LogInWithBrowser => self.log_in_with_browser(ctx),
            Action::OpenServerForm => self.open_server_form(),
            Action::ConnectServer(server) => self.connect_server(ctx, server),
            Action::SwitchSource(provider) => self.switch_source(ctx, provider),
            Action::LogOut => self.log_out(ctx),
        }
    }

    /// Act on a track's right-click menu.
    fn track_menu_choice(&mut self, ctx: &egui::Context, track: Track, choice: TrackMenu) {
        match choice {
            TrackMenu::Play(mode) => self.player.send(mode.command(vec![track])),
            TrackMenu::AddTo(playlist) => self.edit_playlist(ctx, Edit::Add { playlist, tracks: vec![track] }),
            TrackMenu::NewPlaylist => self.new_playlist(vec![track]),
            TrackMenu::Remove => {
                if let Some(playlist) = self.showing_own_playlist() {
                    self.edit_playlist(ctx, Edit::Remove { playlist: playlist.id, tracks: vec![track.id] });
                }
            }
        }
    }

    /// Set the sleep timer (None turns it off).
    fn set_sleep(&mut self, choice: Option<SleepChoice>) {
        self.sleep_choice = choice;
        let sleep = choice.map(|c| match c {
            SleepChoice::Minutes(m) => Sleep::At(Instant::now() + Duration::from_secs(m * 60)),
            SleepChoice::EndOfTrack => Sleep::EndOfTrack,
        });
        self.player.send(Cmd::Sleep(sleep));
    }
}
