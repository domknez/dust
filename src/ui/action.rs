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
use crate::model::{ArtistRef, Likeable, Quality, Track};
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
            Action::Open(view) => self.open_view(view),
            Action::OpenSearch => self.open_search(),
            Action::Back => self.go_back(),
            Action::Forward => self.go_forward(),
            Action::OpenArtist(artist) => self.open_artist(ArtistView::from_ref(&artist)),
            Action::OpenCollection(coll) => self.open_collection(coll),
            Action::ReloadHome => {
                self.library.home = Load::Idle;
                self.open_view(View::Home);
            }
            Action::PlayFrom { tracks, index, shuffle } => self.play_from(tracks, index, shuffle),
            Action::QueueAll(tracks) => {
                self.player.send(Cmd::Enqueue(playable(tracks)));
                self.queued_flash = Some(Instant::now());
            }
            Action::PlaySource(source, mode) => self.play_source(source, mode),
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
            Action::ToggleLike(what) => self.toggle_like(what),
            Action::TrackMenu(track, choice) => self.track_menu_choice(*track, choice),
            Action::EditPlaylist(edit) => self.edit_playlist(edit),
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
            Action::LogIn(arl) => self.log_in_with(arl),
            #[cfg(feature = "login-window")]
            Action::LogInWithBrowser => self.log_in_with_browser(),
            Action::OpenServerForm => self.open_server_form(),
            Action::ConnectServer(server) => self.connect_server(server),
            Action::SwitchSource(provider) => self.switch_source(provider),
            Action::LogOut => self.log_out(),
        }
    }

    /// Act on a track's right-click menu.
    fn track_menu_choice(&mut self, track: Track, choice: TrackMenu) {
        match choice {
            TrackMenu::Play(mode) => self.player.send(mode.command(vec![track])),
            TrackMenu::AddTo(playlist) => self.edit_playlist(Edit::Add { playlist, tracks: vec![track] }),
            TrackMenu::NewPlaylist => self.new_playlist(vec![track]),
            TrackMenu::Remove => {
                if let Some(playlist) = self.showing_own_playlist().map(|p| p.id.clone()) {
                    self.edit_playlist(Edit::Remove { playlist, tracks: vec![track.id] });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app::testing::{TestApp, deezer, ids, track};

    /// What was played (queue and start), from the commands sent.
    fn played(t: &TestApp) -> Vec<(Vec<u64>, usize)> {
        t.sent()
            .into_iter()
            .filter_map(|cmd| match cmd {
                Cmd::Play(queue, index) => Some((ids(&queue), index)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn playing_from_a_row_plays_the_playable_tracks() {
        let mut t = TestApp::new("");
        let list = vec![track(1, true), track(2, false), track(3, true)];
        t.update(Action::PlayFrom { tracks: list.clone(), index: 2, shuffle: false });
        // A row that can't play starts at the next one that can.
        t.update(Action::PlayFrom { tracks: list, index: 1, shuffle: false });
        t.update(Action::PlayFrom { tracks: vec![track(4, false)], index: 0, shuffle: false });
        assert_eq!(played(&t), [(vec![1, 3], 1), (vec![1, 3], 1)]);
    }

    #[test]
    fn shuffle_turns_shuffle_on_and_starts_anywhere() {
        let mut t = TestApp::new("");
        t.sent();
        t.update(Action::PlayFrom { tracks: vec![track(1, true), track(2, true), track(3, true)], index: 0, shuffle: true });
        assert!(matches!(t.sent()[..], [Cmd::Shuffle(true), Cmd::Play(ref queue, index)] if queue.len() == 3 && index < 3));
        assert!(t.app.prefs.shuffle, "and it's remembered");
    }

    #[test]
    fn add_to_queue_takes_the_playable_tracks_and_says_so() {
        let mut t = TestApp::new("");
        t.sent();
        t.update(Action::QueueAll(vec![track(1, true), track(2, false), track(3, true)]));
        assert!(matches!(&t.sent()[..], [Cmd::Enqueue(queued)] if ids(queued) == [1, 3]));
        assert!(t.app.queued_flash.is_some(), "the button confirms it");
    }

    #[test]
    fn opening_a_page_from_the_mini_player_brings_the_window_back() {
        let mut t = TestApp::new("");
        t.app.deezer_ready(deezer());
        t.app.mini_player = Some(egui::vec2(1200.0, 800.0));
        t.update(Action::Player(Cmd::Next));
        assert!(t.app.mini_player.is_some(), "playback stays in the mini player");
        t.update(Action::OpenArtist(ArtistRef { id: "27".into(), name: "Daft Punk".into() }));
        assert!(t.app.mini_player.is_none());
        assert!(matches!(&t.app.browse.view, View::Artist(artist) if artist.id == "27"));
        assert!(t.app.browse.artist_page.is_loading());
    }

    #[test]
    fn a_like_shows_at_once_and_is_sent_in_the_background() {
        let mut t = TestApp::new("");
        t.app.deezer_ready(deezer());
        let jobs = t.jobs.waiting();
        let song = Likeable::Track(crate::model::TrackId::from(5));
        t.update(Action::ToggleLike(song.clone()));
        assert!(t.app.is_liked(&song));
        assert_eq!(t.jobs.waiting(), jobs + 1);
    }
}
