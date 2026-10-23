//! The [`App`]: UI state, its lifecycle (login, background loads, settings) and the
//! top-level panel layout. Screens are drawn by the modules in `views/`.

use super::action::{Action, Actions};
use super::browse::Browse;
use super::covers::Covers;
use super::history::Visit;
use super::library::Library;
use super::notice::Notice;
use super::now_playing::NowPlaying;
use super::sources::{Login, Sources};
use super::state::{ArtistView, Coll, PlayMode, Source, View, playable, playable_from};
use super::style::{self, Appearance, colors, metrics};
use super::updates::Updates;
use crate::credentials;
use crate::deezer::{self, Deezer, Link, Quality, Track};
use crate::model::Error;
use crate::output::airplay::Discovery;
use crate::output::airplay::dacp::{self, Remote};
use crate::player::{Cmd, Output, PlayerHandle, Repeat, Status, saved};
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

/// Field visibility is `pub(super)`: the views in `ui::views` read and update state
/// directly; nothing outside `ui` does.
pub struct App {
    pub(super) player: PlayerHandle,
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

    // Services
    pub(super) discovery: Option<Discovery>,
    pub(super) covers: Covers,
    pub(super) logo: egui::TextureHandle,
    _remote_control: Option<dacp::Server>,
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
    pub(super) queue_saved: Option<(usize, usize, Option<String>)>,
    /// A restore was sent and the player hasn't shown the queue yet.
    pub(super) restore_pending: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        let prefs = Prefs::load();
        let volume = prefs.volume;
        style::install_fonts(ctx);
        style::apply(ctx, prefs.theme.is_dark(ctx));

        let player = PlayerHandle::spawn(ctx.clone(), volume);
        player.send(Cmd::Shuffle(prefs.shuffle));
        player.send(Cmd::Repeat(prefs.repeat));
        let repaint = ctx.clone();
        let discovery =
            Discovery::start(move || repaint.request_repaint()).inspect_err(|e| log_warn!("AirPlay discovery unavailable: {e}")).ok();
        let mut app = Self {
            _remote_control: start_remote_control(player.clone()),
            now_playing: NowPlaying::start(cc, player.clone()),
            updates: Updates::new(prefs.auto_update),
            notice: Notice::default(),
            queued_flash: None,
            queue_saved: None,
            restore_pending: false,
            player,
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
            unmuted_volume: volume,
            mini_player: None,
            restore_output: prefs.output.clone(),
            prefs,
            discovery,
            covers: Covers::new(ctx),
            logo: crate::icon::logo_texture(ctx),
        };
        if let Some(arl) = credentials::load() {
            app.login.task.start(ctx, move || Deezer::login(&arl));
        }
        app.start_sources(ctx);
        app
    }

    // ------------------------------------------------------------ session

    pub(super) fn log_in_with(&mut self, ctx: &egui::Context, arl: String) {
        self.login.error = None;
        self.login.remember = true;
        self.login.task.start(ctx, move || Deezer::login(&arl));
    }

    #[cfg(feature = "login-window")]
    pub(super) fn log_in_with_browser(&mut self, ctx: &egui::Context) {
        self.login.error = None;
        self.login.remember = true;
        self.login.task.start(ctx, || Deezer::login(&crate::login::obtain_arl().map_err(Error::Other)?));
    }

    fn on_logged_in(&mut self, ctx: &egui::Context, client: Deezer) {
        if std::mem::take(&mut self.login.remember) {
            credentials::save(client.arl());
        }
        self.login.arl.clear();
        self.deezer_ready(ctx, client);
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
    pub(super) fn open_view(&mut self, ctx: &egui::Context, view: View) {
        let query = if view == View::Search { self.browse.search.trim().to_string() } else { String::new() };
        let current = self.current_visit();
        self.browse.history.leave(current, &Visit { view: view.clone(), query });
        self.show_view(ctx, view);
    }

    /// Switch the main area to `view`, loading its content in the background.
    pub(super) fn show_view(&mut self, ctx: &egui::Context, view: View) {
        let Some(client) = self.client.clone() else { return };
        self.browse.view = view.clone();
        let library = &mut self.library;
        match &view {
            // Cached lists: loaded the first time they're shown (or after a reset).
            View::Home if library.home.wants_load() => library.home.start(ctx, move || client.home()),
            View::Albums if library.favorite_albums.wants_load() => library.favorite_albums.start(ctx, move || client.favorite_albums()),
            View::Podcasts if library.favorite_shows.wants_load() => library.favorite_shows.start(ctx, move || client.favorite_shows()),
            View::Artists if library.followed.wants_load() => library.followed.start(ctx, move || client.favorite_artists()),
            View::Home | View::Albums | View::Podcasts | View::Artists => {}
            View::Artist(artist) => {
                let id = artist.id.clone();
                self.browse.artist_page.start(ctx, move || client.artist(&id));
            }
            View::Playlists | View::Search | View::Loved | View::Collection(_) => self.load_list(ctx, client, view),
        }
    }

    /// A page with a track list: empty it and load the new one.
    fn load_list(&mut self, ctx: &egui::Context, client: Provider, view: View) {
        let browse = &mut self.browse;
        browse.clear_list();
        match view {
            View::Home | View::Playlists | View::Artists | View::Albums | View::Podcasts | View::Artist(_) => {}
            View::Search if browse.search.trim().is_empty() => {}
            // A pasted Deezer link opens what it points at instead of searching.
            View::Search if client.features().links && deezer::is_link(&browse.search) => {
                let link = browse.search.trim().to_string();
                browse.searched.clear();
                browse.link_load.start(ctx, move || client.resolve_link(&link));
            }
            View::Search => {
                let query = browse.search.trim().to_string();
                browse.searched = query.clone();
                browse.search_load.start(ctx, move || client.search(&query));
            }
            View::Loved => browse.tracks_load.start(ctx, move || client.loved().map(|t| (t, None))),
            View::Collection(coll) => {
                browse.tracks_load.start(ctx, move || match &coll.source {
                    Source::Album(id) => client.album_page(id).map(|(t, info)| (t, Some(info))),
                    source => source.fetch(&client).map(|t| (t, None)),
                });
            }
        }
    }

    fn open_link(&mut self, ctx: &egui::Context, link: Link) {
        self.browse.search.clear();
        match link {
            Link::Album { id, title, artist, picture } => {
                let picture = picture.map(|md5| ("cover".to_string(), md5));
                self.open_collection(ctx, Coll { source: Source::Album(id), kind: "ALBUM", title, subtitle: artist, picture });
            }
            Link::Artist { id } => self.open_artist(ctx, ArtistView { id, name: String::new(), picture: None }),
            Link::Playlist { id, title, picture } => {
                self.open_collection(ctx, Coll { source: Source::Playlist(id), kind: "PLAYLIST", title, subtitle: String::new(), picture });
            }
        }
    }

    /// Open a collection; artists get their full page instead of a track list.
    pub(super) fn open_collection(&mut self, ctx: &egui::Context, coll: Coll) {
        if let Source::Artist(id) = &coll.source {
            let picture = coll.picture.as_ref().map(|(_, md5)| md5.clone());
            return self.open_artist(ctx, ArtistView { id: id.clone(), name: coll.title.clone(), picture });
        }
        self.open_view(ctx, View::Collection(Box::new(coll)));
    }

    pub(super) fn open_artist(&mut self, ctx: &egui::Context, artist: ArtistView) {
        self.browse.discography_tab = 0;
        self.open_view(ctx, View::Artist(Box::new(artist)));
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
    pub(super) fn play_source(&mut self, ctx: &egui::Context, source: Source, mode: PlayMode) {
        let Some(client) = self.client.clone() else { return };
        self.browse.quick_play.start(ctx, move || source.fetch(&client).map(|t| (t, mode)));
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
            saved::save(&self.player.status(), key);
        }
    }

    /// Keep `queue.json` in step with the queue (after the restore was offered).
    fn save_queue_if_changed(&mut self, st: &Status) {
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
                saved::save(st, client.key());
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
        self.poll_login(ctx);
        self.poll_sources(ctx);
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
            self.log_out(ctx);
            self.login.error = Some(expired.to_string());
            return;
        }
        if let Some(link) = link {
            self.open_link(ctx, link);
        }
    }

    fn poll_login(&mut self, ctx: &egui::Context) {
        match self.login.task.poll_take() {
            Some(Ok(client)) => self.on_logged_in(ctx, client),
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
            np.update(&st);
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
