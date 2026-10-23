//! The [`App`]: UI state, its lifecycle (login, background loads, settings) and the
//! top-level panel layout. Screens are drawn by the modules in `views/`.

use super::action::{Action, Actions};
use super::browse::Browse;
use super::covers::Covers;
use super::history::Visit;
use super::library::Library;
use super::load::Jobs;
use super::notice::Notice;
use super::now_playing::NowPlaying;
use super::sources::{Login, Sources};
use super::state::{ArtistView, Coll, PlayMode, Source, View, playable, playable_from};
use super::style::{self, Appearance, colors, metrics};
use super::updates::Updates;
use crate::credentials;
use crate::deezer::{self, Deezer, Link};
use crate::model::{Error, Quality, Track};
use crate::output::airplay::Discovery;
use crate::output::airplay::dacp::{self, Remote};
use crate::player::saved::QueueFiles;
use crate::player::{Cmd, Flow, Output, PlayerHandle, Repeat, Status};
use crate::provider::Provider;
use crate::settings::Prefs;
use eframe::egui::{self, Key, Margin, Ui};

/// What the panel on the right shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidePanel {
    Queue,
    Lyrics,
}

impl App {
    /// Open `panel` on the right, or close it if it's the one showing.
    pub(super) fn toggle_side_panel(&mut self, panel: SidePanel) {
        self.side_panel = (self.side_panel != Some(panel)).then_some(panel);
        self.lyrics.reopened();
    }
}

/// Field visibility is `pub(super)`: the views in `ui::views` read the state and
/// report what the listener did as [actions](super::action); nothing outside `ui`
/// sees it.
pub struct App {
    pub(super) player: PlayerHandle,
    /// Where background loads run.
    pub(super) jobs: Jobs,
    /// Where each source's queue is saved between runs.
    pub(super) queues: QueueFiles,
    /// The music source in use (Deezer or a Navidrome server).
    pub(super) client: Option<Provider>,
    /// Both sources' logins.
    pub(super) sources: Sources,
    /// The login screen's state and a Deezer login on its way.
    pub(super) login: Login,

    // Content
    /// The source's playlists, likes and other lists.
    pub(super) library: Library,
    /// The page on screen and its content.
    pub(super) browse: Browse,

    // Interaction
    /// What the sleep timer was last set to (for its ring and lit choice).
    pub(super) sleep_choice: Option<super::views::SleepChoice>,
    /// The panel open on the right, if any.
    pub(super) side_panel: Option<SidePanel>,
    /// The Settings window is open.
    pub(super) show_settings: bool,
    pub(super) lyrics: super::views::LyricsState,
    /// Queue row being dragged (index into the queue).
    pub(super) queue_drag: Option<usize>,
    pub(super) seek_drag: Option<f32>,
    pub(super) volume_drag: Option<f32>,
    pub(super) unmuted_volume: f32,
    /// In mini player mode: the full window's size, restored on the way back.
    pub(super) mini_player: Option<egui::Vec2>,

    // Settings
    /// Saved preferences (quality, theme, the sidebar's playlist list, the last
    /// volume written...); change them with [`Prefs::update`].
    pub(super) prefs: Prefs,
    /// Speaker (by name) to reselect once discovery finds it.
    pub(super) restore_output: Option<String>,

    // Services (the optional ones may be unavailable, and tests go without)
    pub(super) discovery: Option<Discovery>,
    pub(super) covers: Covers,
    pub(super) logo: egui::TextureHandle,
    remote_control: Option<dacp::Server>,
    now_playing: Option<NowPlaying>,
    pub(super) updates: Updates,
    /// What just happened away from the page on screen (a playlist changed, a like
    /// that didn't go through).
    pub(super) notice: Notice,
    /// When a whole list was last added to the queue (for the button's confirmation).
    pub(super) queued_flash: Option<std::time::Instant>,
    /// What the saved queue file last reflected (queue snapshot, index, Flow);
    /// None until the saved queue was offered to the player after login, so the
    /// empty startup queue never overwrites it.
    pub(super) queue_saved: Option<(usize, usize, Option<Flow>)>,
    /// A restore was sent and the player hasn't shown the queue yet.
    pub(super) restore_pending: bool,
}

impl App {
    /// The app as it starts: the player, background threads, speaker discovery,
    /// remote control, media keys, and the stored logins.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        let prefs = Prefs::load();
        style::install_fonts(ctx);
        style::apply(ctx, prefs.theme.is_dark(ctx));
        crate::update::clean_up();

        let player = PlayerHandle::spawn(ctx.clone(), prefs.volume);
        let mut app = Self::with(ctx, prefs, player, Jobs::threads(ctx), QueueFiles::in_settings_dir());
        let repaint = ctx.clone();
        app.discovery =
            Discovery::start(move || repaint.request_repaint()).inspect_err(|e| log_warn!("AirPlay discovery unavailable: {e}")).ok();
        app.remote_control = start_remote_control(app.player.clone());
        app.now_playing = NowPlaying::start(cc, app.player.clone());
        if let Some(arl) = credentials::load() {
            app.login.task.start(&app.jobs, move || Deezer::login(&arl));
        }
        app.start_sources();
        app
    }

    /// The app's state around `player`, with nothing started: background work goes
    /// to `jobs` and queues are saved to `queues`. Tests build the app this way.
    pub(super) fn with(ctx: &egui::Context, prefs: Prefs, player: PlayerHandle, jobs: Jobs, queues: QueueFiles) -> Self {
        player.send(Cmd::Shuffle(prefs.shuffle));
        player.send(Cmd::Repeat(prefs.repeat));
        Self {
            player,
            jobs,
            queues,
            client: None,
            sources: Sources::new(),
            login: Login::default(),
            library: Library::default(),
            browse: Browse::default(),
            sleep_choice: None,
            side_panel: None,
            show_settings: false,
            lyrics: Default::default(),
            queue_drag: None,
            seek_drag: None,
            volume_drag: None,
            unmuted_volume: prefs.volume,
            mini_player: None,
            restore_output: prefs.output.clone(),
            updates: Updates::new(prefs.auto_update),
            prefs,
            discovery: None,
            covers: Covers::new(ctx),
            logo: crate::icon::logo_texture(ctx),
            remote_control: None,
            now_playing: None,
            notice: Notice::default(),
            queued_flash: None,
            queue_saved: None,
            restore_pending: false,
        }
    }

    // ------------------------------------------------------------ session

    pub(super) fn log_in_with(&mut self, arl: String) {
        self.login.error = None;
        self.login.remember = true;
        self.login.task.start(&self.jobs, move || Deezer::login(&arl));
    }

    #[cfg(feature = "login-window")]
    pub(super) fn log_in_with_browser(&mut self) {
        self.login.error = None;
        self.login.remember = true;
        self.login.task.start(&self.jobs, || Deezer::login(&crate::login::obtain_arl().map_err(Error::Other)?));
    }

    fn on_logged_in(&mut self, client: Deezer) {
        if std::mem::take(&mut self.login.remember) {
            credentials::save(client.arl());
        }
        self.login.arl.clear();
        self.deezer_ready(client);
    }

    // ------------------------------------------------------------ navigation

    /// The Search page as the sidebar opens it: empty, with nothing from the page
    /// before (its list, a load or error); the query field is in the main area.
    pub(super) fn open_search(&mut self) {
        let current = self.current_visit();
        self.browse.history.leave(current, &Visit { view: View::Search, query: String::new() });
        self.browse.view = View::Search;
        self.browse.clear_list();
        self.browse.searched.clear();
    }

    /// Go to `view`, remembering the current page for Back.
    pub(super) fn open_view(&mut self, view: View) {
        let query = if view == View::Search { self.browse.search.trim().to_string() } else { String::new() };
        let current = self.current_visit();
        self.browse.history.leave(current, &Visit { view: view.clone(), query });
        self.show_view(view);
    }

    /// Switch the main area to `view`, loading its content in the background.
    pub(super) fn show_view(&mut self, view: View) {
        let Some(client) = self.client.clone() else { return };
        self.browse.view = view.clone();
        let library = &mut self.library;
        match &view {
            // Cached lists: loaded the first time they're shown (or after a reset).
            View::Home if library.home.wants_load() => library.home.start(&self.jobs, move || client.home()),
            View::Albums if library.favorite_albums.wants_load() => {
                library.favorite_albums.start(&self.jobs, move || client.favorite_albums())
            }
            View::Podcasts if library.favorite_shows.wants_load() => {
                library.favorite_shows.start(&self.jobs, move || client.favorite_shows())
            }
            View::Artists if library.followed.wants_load() => library.followed.start(&self.jobs, move || client.favorite_artists()),
            View::Home | View::Albums | View::Podcasts | View::Artists => {}
            View::Artist(artist) => {
                let id = artist.id.clone();
                self.browse.artist_page.start(&self.jobs, move || client.artist(&id));
            }
            View::Playlists | View::Search | View::Loved | View::Collection(_) => self.load_list(client, view),
        }
    }

    /// A page with a track list: empty it and load the new one.
    fn load_list(&mut self, client: Provider, view: View) {
        let browse = &mut self.browse;
        browse.clear_list();
        match view {
            View::Home | View::Playlists | View::Artists | View::Albums | View::Podcasts | View::Artist(_) => {}
            View::Search if browse.search.trim().is_empty() => {}
            // A pasted Deezer link opens what it points at instead of searching.
            View::Search if client.features().links && deezer::is_link(&browse.search) => {
                let link = browse.search.trim().to_string();
                browse.searched.clear();
                browse.link_load.start(&self.jobs, move || client.resolve_link(&link));
            }
            View::Search => {
                let query = browse.search.trim().to_string();
                browse.searched = query.clone();
                browse.search_load.start(&self.jobs, move || client.search(&query));
            }
            View::Loved => browse.tracks_load.start(&self.jobs, move || client.loved().map(|t| (t, None))),
            View::Collection(coll) => {
                browse.tracks_load.start(&self.jobs, move || match &coll.source {
                    Source::Album(id) => client.album_page(id).map(|(t, info)| (t, Some(info))),
                    source => source.fetch(&client).map(|t| (t, None)),
                });
            }
        }
    }

    fn open_link(&mut self, link: Link) {
        self.browse.search.clear();
        match link {
            Link::Album { id, title, artist, picture } => {
                self.open_collection(Coll { source: Source::Album(id), kind: "ALBUM", title, subtitle: artist, picture });
            }
            Link::Artist { id } => self.open_artist(ArtistView { id, name: String::new(), picture: None }),
            Link::Playlist { id, title, picture } => {
                self.open_collection(Coll { source: Source::Playlist(id), kind: "PLAYLIST", title, subtitle: String::new(), picture });
            }
        }
    }

    /// Open a collection; artists get their full page instead of a track list.
    pub(super) fn open_collection(&mut self, coll: Coll) {
        if let Source::Artist(id) = &coll.source {
            return self.open_artist(ArtistView { id: id.clone(), name: coll.title.clone(), picture: coll.picture.clone() });
        }
        self.open_view(View::Collection(Box::new(coll)));
    }

    pub(super) fn open_artist(&mut self, artist: ArtistView) {
        self.browse.discography_tab = 0;
        self.open_view(View::Artist(Box::new(artist)));
    }

    /// Play `tracks` from `start`: the playable ones, from where `start` lands among
    /// them; with `shuffle`, shuffle goes on and a random one starts.
    pub(super) fn play_from(&mut self, tracks: Vec<Track>, start: usize, shuffle: bool) {
        let Some((queue, index)) = playable_from(tracks, start) else { return };
        let index = if shuffle {
            self.set_shuffle(true);
            fastrand::usize(..queue.len())
        } else {
            index
        };
        self.player.send(Cmd::Play(queue, index));
    }

    /// Fetch a collection and play/queue it without opening it.
    pub(super) fn play_source(&mut self, source: Source, mode: PlayMode) {
        let Some(client) = self.client.clone() else { return };
        self.browse.quick_play.start(&self.jobs, move || source.fetch(&client).map(|t| (t, mode)));
    }

    // ------------------------------------------------------------ settings

    pub(super) fn set_quality(&mut self, quality: Quality) {
        self.player.send(Cmd::Quality(quality));
        self.prefs.update(|p| p.quality = quality);
    }

    pub(super) fn set_appearance(&mut self, appearance: Appearance) {
        self.prefs.update(|p| p.theme = appearance);
    }

    pub(super) fn toggle_report_listens(&mut self) {
        self.prefs.update(|p| p.report_listens = !p.report_listens);
        self.player.send(Cmd::ReportListens(self.prefs.report_listens));
    }

    pub(super) fn set_shuffle(&mut self, on: bool) {
        self.player.send(Cmd::Shuffle(on));
        self.prefs.update(|p| p.shuffle = on);
    }

    pub(super) fn set_repeat(&mut self, repeat: Repeat) {
        self.player.send(Cmd::Repeat(repeat));
        self.prefs.update(|p| p.repeat = repeat);
    }

    pub(super) fn select_output(&mut self, output: Output) {
        let remembered = match &output {
            Output::Local => None,
            Output::AirPlay(d) => Some(d.name.clone()),
        };
        self.prefs.update(|p| p.output = remembered);
        self.restore_output = None;
        self.player.send(Cmd::Output(output));
    }

    /// Save the queue of source `key` as it is now (unless its restore is still on
    /// its way, which would save an empty queue over it).
    pub(super) fn save_queue_now(&mut self, key: &str) {
        if self.queue_saved.is_some() && !self.restore_pending {
            self.queues.save(&self.player.status(), key);
        }
    }

    /// Keep `queue.json` in step with the queue (after the restore was offered).
    pub(super) fn save_queue_if_changed(&mut self, st: &Status) {
        // Until the player shows the restored queue, its (empty) queue isn't news.
        if self.restore_pending {
            if st.queue.is_empty() {
                return;
            }
            self.restore_pending = false;
        }
        let Some(last) = &self.queue_saved else { return };
        let key = (std::sync::Arc::as_ptr(&st.queue) as usize, st.index, st.flow.clone());
        if *last != key {
            if let Some(client) = &self.client {
                self.queues.save(st, client.key());
            }
            self.queue_saved = Some(key);
        }
    }

    /// Save volume once it settles (not on every slider step) and reselect the
    /// remembered speaker as soon as it shows up on the network.
    fn persist_playback(&mut self, st: &Status) {
        if self.volume_drag.is_none() && (st.volume - self.prefs.volume).abs() > 0.001 {
            self.prefs.update(|p| p.volume = st.volume);
        }
        if let Some(name) = &self.restore_output
            && let Some(device) = self.discovery.as_ref().and_then(|d| d.devices().into_iter().find(|d| &d.name == name))
        {
            if device.supported && !device.password {
                self.player.send(Cmd::Output(Output::AirPlay(device)));
            }
            self.restore_output = None;
        }
    }

    // ------------------------------------------------------------ frame

    /// Collect what background loads finished since the last frame.
    fn poll_loads(&mut self, ctx: &egui::Context) {
        self.poll_login();
        self.poll_sources();
        self.poll_likes(ctx);
        self.poll_playlist_edits(ctx);
        let mut failed = self.library.poll();
        let link = self.browse.poll(&self.library.playlists, &mut failed);
        self.poll_quick_play(ctx, &mut failed);
        // Each error is shown with what failed to load; an expired session also
        // ends it, and then what else arrived belongs to the old one.
        for error in &failed {
            log_warn!("{error}");
        }
        if let Some(expired) = failed.into_iter().find(Error::is_session_expired) {
            self.log_out();
            self.login.error = Some(expired.to_string());
            return;
        }
        if let Some(link) = link {
            self.open_link(link);
        }
    }

    fn poll_login(&mut self) {
        match self.login.task.poll_take() {
            Some(Ok(client)) => self.on_logged_in(client),
            Some(Err(e)) => {
                self.login.remember = false;
                self.login.error = Some(e.to_string());
            }
            None => {}
        }
    }

    /// Tracks fetched from a card: play or queue them. They aren't shown anywhere,
    /// so a failure is a notice.
    fn poll_quick_play(&mut self, ctx: &egui::Context, failed: &mut Vec<Error>) {
        match self.browse.quick_play.poll_take() {
            Some(Ok((tracks, mode))) => {
                let tracks = playable(tracks);
                if !tracks.is_empty() {
                    self.player.send(mode.command(tracks));
                }
            }
            Some(Err(e)) => {
                // An expired session says so on the login screen instead.
                if !e.is_session_expired() {
                    self.notice.error(ctx, format!("Couldn't load the tracks: {e}"));
                }
                failed.push(e);
            }
            None => {}
        }
    }

    /// Keyboard shortcuts that work anywhere (some only in the full window).
    fn shortcuts(&self, ctx: &egui::Context, acts: &mut Actions) {
        if !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::Space)) {
            acts.push(Action::Player(Cmd::Toggle));
        }
        self.mini_player_shortcut(ctx, acts);
        if self.mini_player.is_none() {
            self.history_input(ctx, acts);
            self.settings_shortcut(ctx, acts);
            self.nav_shortcuts(ctx, acts);
        }
    }

    /// Follow the chosen appearance (and the OS setting live when on System).
    fn sync_appearance(&self, ctx: &egui::Context) {
        let dark = self.prefs.theme.is_dark(ctx);
        if dark != style::palette::is_dark() {
            style::apply(ctx, dark);
        }
    }
}

/// Speaker buttons (volume, play/pause, skip) come back to us over DACP.
fn start_remote_control(player: PlayerHandle) -> Option<dacp::Server> {
    dacp::start(move |remote| {
        player.send(match remote {
            Remote::Volume(v) => Cmd::Volume(v),
            Remote::VolumeStep(d) => Cmd::VolumeStep(d),
            Remote::PlayPause => Cmd::Toggle,
            Remote::Play => Cmd::Resume,
            Remote::Pause => Cmd::Pause,
            Remote::Next => Cmd::Next,
            Remote::Prev => Cmd::Prev,
        })
    })
    .inspect_err(|e| log_warn!("speaker remote control unavailable: {e}"))
    .ok()
}

fn panel_frame(fill: egui::Color32, margin: Margin) -> egui::Frame {
    egui::Frame::new().fill(fill).inner_margin(margin)
}

impl eframe::App for App {
    /// Quitting: a downloaded update installs now, so the next start is the new version.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Once more with the exact position, to resume right there next time.
        if let Some(client) = &self.client {
            self.save_queue_now(client.key());
        }
        self.updates.apply_on_exit();
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        log_repaint_causes(&ctx);
        self.sync_appearance(&ctx);
        self.covers.begin_frame();
        self.poll_loads(&ctx);
        self.updates.poll(&ctx);
        let mut acts = Actions::default();
        if self.client.is_none() {
            self.login_screen(ui, &mut acts);
            self.server_dialog(&ctx, &mut acts);
            self.apply(&ctx, acts);
            return;
        }
        self.shortcuts(&ctx, &mut acts);
        let st = self.player.status();
        self.persist_playback(&st);
        self.save_queue_if_changed(&st);
        if let Some(np) = &mut self.now_playing {
            np.update(&self.covers, &st);
        }
        let p = colors();
        if self.mini_player.is_some() {
            egui::CentralPanel::default().frame(panel_frame(p.bar, Margin::ZERO)).show(ui, |ui| self.mini_player(ui, &st, &mut acts));
            self.apply(&ctx, acts);
            return;
        }
        egui::Panel::bottom("player")
            .exact_size(metrics::PLAYER_BAR_HEIGHT)
            .resizable(false)
            .show_separator_line(false)
            .frame(panel_frame(p.bar, Margin::symmetric(20, 0)))
            .show(ui, |ui| self.player_bar(ui, &st, &mut acts));
        egui::Panel::left("nav")
            .exact_size(metrics::SIDEBAR_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(panel_frame(p.sidebar, Margin { left: 12, right: 12, top: 0, bottom: 10 }))
            .show(ui, |ui| self.sidebar(ui, &mut acts));
        if let Some(panel) = self.side_panel {
            egui::Panel::right("side-panel")
                .exact_size(metrics::QUEUE_WIDTH)
                .resizable(false)
                .show_separator_line(false)
                .frame(panel_frame(p.sidebar, Margin { left: 14, right: 14, top: 0, bottom: 10 }))
                .show(ui, |ui| match panel {
                    SidePanel::Queue => self.queue_panel(ui, &st, &mut acts),
                    SidePanel::Lyrics => self.lyrics_panel(ui, &st, &mut acts),
                });
        }
        let margin = metrics::CONTENT_MARGIN;
        egui::CentralPanel::default()
            .frame(panel_frame(p.bg, Margin { left: margin, right: margin, top: 0, bottom: 0 }))
            .show(ui, |ui| self.content(ui, &st, &mut acts));
        self.playlist_dialog(&ctx, &mut acts);
        self.settings_window(&ctx, &mut acts);
        self.server_dialog(&ctx, &mut acts);
        self.apply(&ctx, acts);
    }
}

/// DUST_LOG=trace: print what requested each frame (to hunt idle redraws).
fn log_repaint_causes(ctx: &egui::Context) {
    log_trace!("frame {}: {:?}", ctx.cumulative_pass_nr(), ctx.repaint_causes());
}

/// An [`App`] for tests, built with [`App::with`]: no window, no player thread, no
/// network. What it sends the player waits to be looked at, its background jobs
/// are held (never run) and its queues are saved to a folder of its own.
#[cfg(test)]
pub(super) mod testing {
    use super::*;
    use crate::model::{DeezerSong, Origin};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::Receiver;

    pub struct TestApp {
        pub app: App,
        pub ctx: egui::Context,
        pub jobs: Jobs,
        commands: Receiver<Cmd>,
        /// Where the queues are saved; removed with the TestApp.
        dir: PathBuf,
    }

    impl TestApp {
        /// An app with these preferences (`key=value` lines, as in the settings file).
        pub fn new(settings: &str) -> Self {
            static COUNT: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!("dust-test-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
            let ctx = egui::Context::default();
            let (player, commands) = PlayerHandle::detached();
            let jobs = Jobs::held();
            let app = App::with(&ctx, Prefs::parse(settings), player, jobs.clone(), QueueFiles::at(dir.clone()));
            TestApp { app, ctx, jobs, commands, dir }
        }

        /// The listener does something.
        pub fn update(&mut self, action: Action) {
            self.app.update(&self.ctx, action);
        }

        /// What was sent to the player since the last look.
        pub fn sent(&self) -> Vec<Cmd> {
            self.commands.try_iter().collect()
        }

        /// The app's saved queues.
        pub fn queues(&self) -> QueueFiles {
            QueueFiles::at(self.dir.clone())
        }
    }

    impl Drop for TestApp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    pub fn deezer() -> Deezer {
        Deezer::offline("Dee Zer")
    }

    pub fn navidrome() -> crate::subsonic::Subsonic {
        crate::subsonic::Subsonic::offline("nas:4533", "me")
    }

    pub fn track(id: u64, available: bool) -> Track {
        Track {
            id: id.into(),
            title: format!("Song {id}"),
            artist: String::new(),
            artists: Vec::new(),
            album: String::new(),
            album_id: String::new(),
            duration: 180,
            art: None,
            available,
            available_from: String::new(),
            origin: Origin::Deezer(DeezerSong::default()),
        }
    }

    /// The numbers of tracks made by [`track`].
    pub fn ids(tracks: &[Track]) -> Vec<u64> {
        tracks.iter().map(|t| t.id.as_str().parse().expect("a test track")).collect()
    }

    /// The player showing a queue of these tracks.
    pub fn queue(ids: &[u64], index: usize) -> Status {
        Status { queue: std::sync::Arc::new(ids.iter().map(|&id| track(id, true)).collect()), index, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{TestApp, deezer, track};
    use super::*;
    use crate::model::Section;
    use crate::ui::load::Load;

    /// Logged in to Deezer, on the home page.
    fn logged_in() -> TestApp {
        let mut t = TestApp::new("");
        t.app.deezer_ready(deezer());
        t
    }

    fn page(t: &TestApp) -> (View, &str) {
        (t.app.browse.view.clone(), t.app.browse.search.as_str())
    }

    #[test]
    fn back_and_forward_bring_back_the_page_and_its_search() {
        let mut t = logged_in();
        t.update(Action::Open(View::Loved));
        t.app.browse.search = "daft punk".into();
        t.update(Action::Open(View::Search));
        assert_eq!(t.app.browse.searched, "daft punk");
        // Typed, but not searched for.
        t.app.browse.search = "justice".into();
        t.update(Action::Open(View::Playlists));
        t.update(Action::Back);
        assert_eq!(page(&t), (View::Search, "daft punk"));
        assert!(t.app.browse.search_load.is_loading(), "the search runs again");
        t.update(Action::Back);
        assert_eq!(page(&t).0, View::Loved);
        t.update(Action::Forward);
        assert_eq!(page(&t), (View::Search, "daft punk"));
        t.update(Action::Forward);
        assert_eq!(page(&t).0, View::Playlists);
        assert!(!t.app.browse.history.can_go_forward());
    }

    #[test]
    fn cached_pages_load_once_and_track_lists_each_time() {
        let mut t = logged_in();
        assert!(t.app.library.home.is_loading(), "home loads with the source");
        t.app.library.home = Load::Ready(vec![Section { title: "Mixes".into(), items: Vec::new() }]);
        let jobs = t.jobs.waiting();
        t.update(Action::Open(View::Loved));
        assert!(t.app.browse.tracks_load.is_loading());
        t.update(Action::Open(View::Home));
        assert_eq!(t.jobs.waiting(), jobs + 1, "home was there already");
        t.update(Action::Back);
        assert_eq!(t.jobs.waiting(), jobs + 2, "a track list loads each time it's shown");
        t.update(Action::ReloadHome);
        assert!(t.app.library.home.is_loading(), "refresh loads home again");
    }

    #[test]
    fn the_sidebar_search_opens_empty() {
        let mut t = logged_in();
        t.app.browse.search = "daft".into();
        t.update(Action::Open(View::Search));
        t.app.browse.tracks = vec![track(1, true)];
        t.update(Action::OpenSearch);
        let browse = &t.app.browse;
        assert!(browse.tracks.is_empty() && browse.searched.is_empty() && !browse.loading_tracks());
        t.update(Action::Back);
        assert_eq!(t.app.browse.searched, "daft", "the search before is a page of its own");
    }

    #[test]
    fn the_app_starts_with_nothing_running() {
        let t = TestApp::new("shuffle=true\nrepeat=all\n");
        assert!(matches!(t.sent()[..], [Cmd::Shuffle(true), Cmd::Repeat(Repeat::All)]), "the player gets the saved modes");
        assert_eq!(t.jobs.waiting(), 0);
        assert!(t.app.client.is_none() && t.app.discovery.is_none() && t.app.now_playing.is_none());
        assert!(t.queues().load("").is_none());
    }

    #[test]
    fn player_actions_go_to_the_player() {
        let mut t = TestApp::new("");
        t.sent();
        t.update(Action::Player(Cmd::Next));
        t.update(Action::Repeat(Repeat::One));
        assert!(matches!(t.sent()[..], [Cmd::Next, Cmd::Repeat(Repeat::One)]));
        assert_eq!(t.app.prefs.repeat, Repeat::One, "remembered (in memory: the test's settings aren't a file)");
    }
}
