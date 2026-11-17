//! What the listener did. Views push [`Action`]s into [`Actions`] while they draw;
//! once the frame is drawn, [`App::update`] applies them: the one place that
//! navigates, decides what to play and calls the music source.
//!
//! State that belongs to a single widget (text being typed, a slider being dragged,
//! a page's open tab) is still changed in place while drawing.

use super::app::{App, SidePanel};
use super::crate_store::{Change, Thing};
use super::load::Load;
use super::playlist_edit::{Dialog, Edit};
use super::state::{ArtistView, Coll, CollSource, PlayMode, View, playable};
use super::style::Appearance;
use super::switches::Switch;
use super::views::Section;
use super::views::{Export, Filter, SleepChoice, reveal_log};
use super::widgets::TrackMenu;
use crate::model::{ArtistRef, Likeable, Quality, Track};
use crate::player::{Cmd, Output, Repeat, Sleep};
use crate::source::Shared;
use crate::subsonic::Server;
use eframe::egui;
use std::path::PathBuf;
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
    PlaySource(CollSource, PlayMode),
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
    TrackMenu(Vec<Track>, TrackMenu),
    EditPlaylist(Edit),
    /// Ask for a name, then create a playlist holding these tracks.
    NewPlaylist(Vec<Track>),
    /// Ask before renaming or deleting an own playlist.
    PlaylistDialog(Dialog),

    // Crate
    /// Save to the Crate; the text says where it was found.
    SaveToCrate(Thing, String),
    /// A note into the Crate.
    CrateNote(String),
    /// Change (or remove) an entry in the Crate.
    Crate(Change),
    /// The Crate page, with this entry shown and/or this filter.
    OpenCrate(Option<String>, Option<Filter>),
    /// Play the Crate's new tracks, albums and artists.
    PlayCrate,
    /// Search the source for this.
    SearchFor(String),
    /// Sync the Crate and history through this folder; None keeps them here only.
    SyncFolder(Option<PathBuf>),
    ExportCrate(Export),
    /// Bring back the daily backup in this folder.
    RestoreBackup(PathBuf),

    // Window
    /// Open the panel on the right (at its tab shown last), or close it.
    TogglePanel,
    /// The lyrics page, or back from it.
    ToggleLyrics,
    /// Show this panel on the right; None closes it.
    ShowPanel(Option<SidePanel>),
    ToggleMiniPlayer,
    ShowSettings(bool),
    /// Show this part of Settings.
    SettingsSection(Section),
    /// The command palette.
    OpenPalette,
    /// Fold the sidebar's playlist list away, or bring it back.
    ToggleSidebarPlaylists,

    // Settings
    Quality(Quality),
    Appearance(Appearance),
    ToggleReportListens,
    /// New releases go in the Crate, or not.
    ReleaseAlerts(bool),
    /// Turn a part of dust on or off.
    Switch(Switch, bool),
    /// Flow leaves out the tracks heard before, or not.
    FlowNewOnly(bool),
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
    SwitchSource(Shared),
    /// Log out of the source in use (or disconnect the server).
    LogOut,
}

impl Action {
    /// Whether it shows a page (in the full window, so the mini player gives way).
    fn opens_page(&self) -> bool {
        matches!(
            self,
            Action::Open(_)
                | Action::OpenSearch
                | Action::Back
                | Action::Forward
                | Action::OpenArtist(_)
                | Action::OpenCollection(_)
                | Action::OpenCrate(..)
                | Action::SearchFor(_)
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

    /// What was pushed, for tests to look at.
    #[cfg(test)]
    pub fn into_vec(self) -> Vec<Action> {
        self.0
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
        // Something turned off in Settings (a shortcut, a menu drawn before).
        if !self.allows(&action) {
            return;
        }
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
            Action::TrackMenu(tracks, choice) => self.track_menu_choice(ctx, tracks, choice),
            Action::SaveToCrate(thing, from) => self.save_to_crate(ctx, thing, from),
            Action::CrateNote(note) => self.crate_note(note),
            Action::Crate(change) => self.crate_box.change(change),
            Action::OpenCrate(entry, filter) => self.open_crate(entry, filter),
            Action::PlayCrate => self.play_crate(),
            Action::SearchFor(query) => self.search_for(query),
            Action::SyncFolder(folder) => self.set_sync_folder(ctx, folder),
            Action::ExportCrate(export) => self.export_crate(ctx, export),
            Action::RestoreBackup(dir) => self.restore_backup(ctx, dir),
            Action::EditPlaylist(edit) => self.edit_playlist(edit),
            Action::NewPlaylist(tracks) => self.new_playlist(tracks),
            Action::PlaylistDialog(dialog) => self.library.editing.dialog = Some(dialog),
            Action::TogglePanel => self.toggle_side_panel(),
            Action::ToggleLyrics => self.toggle_lyrics(),
            Action::ShowPanel(panel) => self.show_side_panel(panel),
            Action::ToggleMiniPlayer => self.toggle_mini_player(ctx),
            Action::ShowSettings(show) => self.show_settings = show,
            Action::SettingsSection(section) => self.settings_section = section,
            Action::Switch(switch, on) => self.set_switch(ctx, switch, on),
            Action::OpenPalette => self.palette = Some(Default::default()),
            Action::ToggleSidebarPlaylists => self.prefs.update(|p| p.sidebar_playlists = !p.sidebar_playlists),
            Action::Quality(quality) => self.set_quality(quality),
            Action::Appearance(appearance) => self.set_appearance(appearance),
            Action::ToggleReportListens => self.toggle_report_listens(),
            Action::ReleaseAlerts(on) => self.prefs.update(|p| p.release_alerts = on),
            Action::FlowNewOnly(on) => {
                self.prefs.update(|p| p.flow_new_only = on);
                self.send_skip_heard();
            }
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
            Action::SwitchSource(source) => self.switch_source(source),
            Action::LogOut => self.log_out(),
        }
    }

    /// Act on the right-click menu of a track, or of the tracks picked together.
    fn track_menu_choice(&mut self, ctx: &egui::Context, tracks: Vec<Track>, choice: TrackMenu) {
        match choice {
            TrackMenu::Play(mode) => {
                let tracks = playable(tracks);
                if !tracks.is_empty() {
                    self.player.send(mode.command(tracks));
                }
            }
            TrackMenu::AddTo(playlist) => self.edit_playlist(Edit::Add { playlist, tracks }),
            TrackMenu::NewPlaylist => self.new_playlist(tracks),
            TrackMenu::Mix => {
                if let Some(track) = tracks.first() {
                    self.play_source(CollSource::TrackMix(track.id.to_string()), PlayMode::Now);
                }
            }
            TrackMenu::Crate => {
                for track in tracks.into_iter().filter(|t| !t.is_episode()) {
                    self.save_to_crate(ctx, Thing::Track(track), String::new());
                }
            }
            TrackMenu::Remove => {
                if let Some(playlist) = self.showing_own_playlist().map(|p| p.id.clone()) {
                    let tracks = tracks.into_iter().map(|t| t.id).collect();
                    self.edit_playlist(Edit::Remove { playlist, tracks });
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
            SleepChoice::EndOfAlbum => Sleep::EndOfAlbum,
            SleepChoice::EndOfQueue => Sleep::EndOfQueue,
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
    fn a_track_saved_twice_is_one_entry_new_again() {
        let mut t = TestApp::new("");
        t.app.source_ready(deezer());
        let save = || Action::SaveToCrate(Thing::Track(track(1, true)), "Flow · chill".into());
        t.update(save());
        t.update(save());
        let entries = t.app.crate_box.entries().to_vec();
        assert_eq!(entries.len(), 1);
        assert_eq!((entries[0].source.as_str(), entries[0].from.as_str()), ("deezer", "Flow · chill"));
        t.update(Action::Crate(Change::Heard { id: entries[0].id.clone(), heard: true }));
        assert_eq!(t.app.crate_box.new_count(), 0);
        t.update(save());
        assert_eq!((t.app.crate_box.entries().len(), t.app.crate_box.new_count()), (1, 1), "saved again: new again");
        t.update(Action::CrateNote("  ".into()));
        t.update(Action::CrateNote("Elder, from Ana".into()));
        assert_eq!(t.app.crate_box.entries()[0].title(), "Elder, from Ana", "newest first");
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
        t.app.source_ready(deezer());
        t.app.mini_player = Some(egui::vec2(1200.0, 800.0));
        t.update(Action::Player(Cmd::Next));
        assert!(t.app.mini_player.is_some(), "playback stays in the mini player");
        t.update(Action::OpenArtist(ArtistRef { id: "27".into(), name: "Daft Punk".into() }));
        assert!(t.app.mini_player.is_none());
        assert!(matches!(&t.app.browse.view, View::Artist(artist) if artist.id == "27"));
        assert!(t.app.browse.artist_page.is_loading());
    }

    #[test]
    fn the_playlist_added_to_comes_first_in_the_menu() {
        use crate::model::{Playlist, PlaylistId};
        let mut t = TestApp::new("");
        t.app.source_ready(deezer());
        let playlist = |id: u64| Playlist { id: id.into(), title: format!("P{id}"), count: 0, owned: true, private: false, picture: None };
        t.update(Action::TrackMenu(vec![track(1, true)], TrackMenu::AddTo(playlist(5))));
        t.update(Action::EditPlaylist(Edit::Add { playlist: playlist(9), tracks: vec![track(2, true)] }));
        t.update(Action::TrackMenu(vec![track(3, true)], TrackMenu::AddTo(playlist(5))));
        assert_eq!(t.app.prefs.recent_playlists, [PlaylistId::from(5), PlaylistId::from(9)]);
    }

    #[test]
    fn a_like_shows_at_once_and_is_sent_in_the_background() {
        let mut t = TestApp::new("");
        t.app.source_ready(deezer());
        let jobs = t.jobs.waiting();
        let song = Likeable::Track(crate::model::TrackId::from(5));
        t.update(Action::ToggleLike(song.clone()));
        assert!(t.app.is_liked(&song));
        assert_eq!(t.jobs.waiting(), jobs + 1);
    }
}
