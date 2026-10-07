//! The [`App`]: UI state, its lifecycle (login, background loads, settings) and the
//! top-level panel layout. Screens are drawn by the modules in `views/`.

use super::covers::Covers;
use super::now_playing::NowPlaying;
use super::state::{ArtistView, Coll, PlayMode, Source, View};
use super::style::{self, Appearance, colors, metrics};
use super::tasks::{self, Task};
use super::updates::Updates;
use crate::credentials;
use crate::deezer::{self, ArtistPage, Deezer, Error, Item, Link, Playlist, Quality, SearchResults, Section, Track};
use crate::output::airplay::Discovery;
use crate::output::airplay::dacp::{self, Remote};
use crate::player::{Cmd, Output, PlayerHandle, Status};
use crate::settings;
use eframe::egui::{self, Key, Margin, Ui};

/// Field visibility is `pub(super)`: the views in `ui::views` read and update state
/// directly; nothing outside `ui` does.
pub struct App {
    pub(super) player: PlayerHandle,
    pub(super) client: Option<Deezer>,

    // Login
    pub(super) arl_input: String,
    pub(super) login: Task<Deezer>,
    /// Store the session in the keychain once the pending login succeeds.
    pub(super) remember_login: bool,
    pub(super) show_arl_input: bool,
    pub(super) login_error: Option<String>,

    // Content
    pub(super) view: View,
    pub(super) search: String,
    pub(super) searched: String,
    pub(super) tracks: Vec<Track>,
    pub(super) tracks_task: Task<Vec<Track>>,
    /// Artist, album and playlist cards of the current search.
    pub(super) search_sections: Vec<Section>,
    pub(super) search_task: Task<SearchResults>,
    /// A Deezer link pasted into search, being resolved.
    pub(super) link_task: Task<Link>,
    pub(super) list_error: Option<String>,
    pub(super) playlists: Vec<Playlist>,
    pub(super) playlists_task: Task<Vec<Playlist>>,
    pub(super) home: Vec<Section>,
    /// The artist page being shown (for `View::Artist`), once loaded.
    pub(super) artist_page: Option<ArtistPage>,
    pub(super) artist_task: Task<ArtistPage>,
    /// Followed artists, as cards (for `View::Artists`).
    pub(super) followed: Vec<Item>,
    pub(super) followed_task: Task<Vec<Item>>,
    pub(super) home_task: Task<Vec<Section>>,
    /// Tracks fetched to play straight from a home card.
    pub(super) quick_play: Task<(Vec<Track>, PlayMode)>,

    // Interaction
    pub(super) show_queue: bool,
    /// Queue row being dragged (index into the queue).
    pub(super) queue_drag: Option<usize>,
    pub(super) seek_drag: Option<f32>,
    pub(super) volume_drag: Option<f32>,
    pub(super) unmuted_volume: f32,
    /// In mini player mode: the full window's size, restored on the way back.
    pub(super) mini_player: Option<egui::Vec2>,
    /// An artist name was clicked somewhere this frame; opened after drawing.
    pub(super) pending_artist: Option<crate::deezer::ArtistRef>,

    // Settings
    pub(super) quality: Quality,
    pub(super) appearance: Appearance,
    pub(super) report_listens: bool,
    /// Last volume written to settings.
    saved_volume: f32,
    /// Speaker (by name) to reselect once discovery finds it.
    pub(super) restore_output: Option<String>,

    // Services
    pub(super) discovery: Option<Discovery>,
    pub(super) covers: Covers,
    pub(super) logo: egui::TextureHandle,
    _remote_control: Option<dacp::Server>,
    now_playing: Option<NowPlaying>,
    pub(super) updates: Updates,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        let saved = settings::load();
        let appearance = saved.get("theme").and_then(|k| Appearance::from_key(k)).unwrap_or(Appearance::System);
        let quality = saved.get("quality").and_then(|k| Quality::from_key(k)).unwrap_or(Quality::Mp3_320);
        let report_listens = saved.get("report_listens").is_none_or(|v| v != "false");
        let volume = saved.get("volume").and_then(|v| v.parse::<f32>().ok()).map_or(0.5, |v| v.clamp(0.0, 1.0));
        let restore_output = saved.get("output").filter(|o| !o.is_empty()).cloned();
        let auto_update = saved.get("auto_update").is_none_or(|v| v != "false");
        style::install_fonts(ctx);
        style::apply(ctx, appearance.is_dark(ctx));

        let player = PlayerHandle::spawn(ctx.clone(), volume);
        let repaint = ctx.clone();
        let discovery =
            Discovery::start(move || repaint.request_repaint()).inspect_err(|e| log_warn!("AirPlay discovery unavailable: {e}")).ok();
        let mut app = Self {
            _remote_control: start_remote_control(player.clone()),
            now_playing: NowPlaying::start(cc, player.clone()),
            updates: Updates::new(auto_update),
            player,
            client: None,
            arl_input: String::new(),
            login: None,
            remember_login: false,
            show_arl_input: false,
            login_error: None,
            view: View::Home,
            search: String::new(),
            searched: String::new(),
            tracks: Vec::new(),
            tracks_task: None,
            search_sections: Vec::new(),
            search_task: None,
            link_task: None,
            list_error: None,
            playlists: Vec::new(),
            playlists_task: None,
            home: Vec::new(),
            artist_page: None,
            artist_task: None,
            followed: Vec::new(),
            followed_task: None,
            home_task: None,
            quick_play: None,
            show_queue: false,
            queue_drag: None,
            seek_drag: None,
            volume_drag: None,
            unmuted_volume: volume,
            mini_player: None,
            pending_artist: None,
            quality,
            appearance,
            report_listens,
            saved_volume: volume,
            restore_output,
            discovery,
            covers: Covers::new(ctx),
            logo: crate::icon::logo_texture(ctx),
        };
        if let Some(arl) = credentials::load() {
            app.login = tasks::spawn(ctx, move || Deezer::login(&arl));
        }
        app
    }

    // ------------------------------------------------------------ session

    pub(super) fn log_in_with(&mut self, ctx: &egui::Context, arl: String) {
        self.login_error = None;
        self.remember_login = true;
        self.login = tasks::spawn(ctx, move || Deezer::login(&arl));
    }

    #[cfg(feature = "login-window")]
    pub(super) fn log_in_with_browser(&mut self, ctx: &egui::Context) {
        self.login_error = None;
        self.remember_login = true;
        self.login = tasks::spawn(ctx, || Deezer::login(&crate::login::obtain_arl().map_err(crate::deezer::Error::Other)?));
    }

    pub(super) fn log_out(&mut self) {
        credentials::clear();
        self.client = None;
        self.tracks.clear();
        self.playlists.clear();
        self.home.clear();
    }

    fn on_logged_in(&mut self, ctx: &egui::Context, client: Deezer) {
        if std::mem::take(&mut self.remember_login) {
            credentials::save(client.arl());
        }
        self.arl_input.clear();
        self.player.send(Cmd::Client(client.clone()));
        self.player.send(Cmd::Quality(self.quality));
        self.player.send(Cmd::ReportListens(self.report_listens));
        let c = client.clone();
        self.playlists_task = tasks::spawn(ctx, move || c.playlists());
        self.client = Some(client);
        self.open_view(ctx, View::Home);
    }

    // ------------------------------------------------------------ navigation

    /// Switch the main area to `view`, loading its content in the background.
    pub(super) fn open_view(&mut self, ctx: &egui::Context, view: View) {
        let Some(client) = self.client.clone() else { return };
        self.view = view.clone();
        self.list_error = None;
        if view == View::Home {
            // Cached; the refresh button clears it to force a reload.
            if self.home.is_empty() && self.home_task.is_none() {
                self.home_task = tasks::spawn(ctx, move || client.home());
            }
            return;
        }
        if view == View::Artists {
            if self.followed.is_empty() && self.followed_task.is_none() {
                self.followed_task = tasks::spawn(ctx, move || client.favorite_artists());
            }
            return;
        }
        if let View::Artist(artist) = &view {
            let id = artist.id.clone();
            self.artist_page = None;
            self.artist_task = tasks::spawn(ctx, move || client.artist(&id));
            return;
        }
        self.tracks.clear();
        self.search_sections.clear();
        // Replacing a task drops its receiver, so a stale result never lands.
        self.tracks_task = None;
        self.search_task = None;
        self.link_task = None;
        match view {
            View::Home | View::Playlists | View::Artists | View::Artist(_) => {}
            View::Search if self.search.trim().is_empty() => {}
            // A pasted Deezer link opens what it points at instead of searching.
            View::Search if deezer::is_link(&self.search) => {
                let link = self.search.trim().to_string();
                self.searched.clear();
                self.link_task = tasks::spawn(ctx, move || client.resolve_link(&link));
            }
            View::Search => {
                let query = self.search.trim().to_string();
                self.searched = query.clone();
                self.search_task = tasks::spawn(ctx, move || client.search(&query));
            }
            View::Loved => self.tracks_task = tasks::spawn(ctx, move || client.loved()),
            View::Collection(coll) => self.tracks_task = tasks::spawn(ctx, move || coll.source.fetch(&client)),
        }
    }

    /// A track list is on its way.
    pub(super) fn loading_tracks(&self) -> bool {
        self.tracks_task.is_some() || self.search_task.is_some() || self.link_task.is_some()
    }

    fn open_link(&mut self, ctx: &egui::Context, link: Link) {
        self.search.clear();
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
        self.open_view(ctx, View::Artist(Box::new(artist)));
    }

    /// Fetch a collection and play/queue it without opening it.
    pub(super) fn play_source(&mut self, ctx: &egui::Context, source: Source, mode: PlayMode) {
        let Some(client) = self.client.clone() else { return };
        self.quick_play = tasks::spawn(ctx, move || source.fetch(&client).map(|t| (t, mode)));
    }

    // ------------------------------------------------------------ settings

    pub(super) fn set_quality(&mut self, quality: Quality) {
        self.quality = quality;
        self.player.send(Cmd::Quality(quality));
        settings::set("quality", quality.key());
    }

    pub(super) fn set_appearance(&mut self, appearance: Appearance) {
        self.appearance = appearance;
        settings::set("theme", appearance.key());
    }

    pub(super) fn toggle_report_listens(&mut self) {
        self.report_listens = !self.report_listens;
        self.player.send(Cmd::ReportListens(self.report_listens));
        settings::set("report_listens", if self.report_listens { "true" } else { "false" });
    }

    pub(super) fn select_output(&mut self, output: Output) {
        let remembered = match &output {
            Output::Local => String::new(),
            Output::AirPlay(d) => d.name.clone(),
        };
        settings::set("output", &remembered);
        self.restore_output = None;
        self.player.send(Cmd::Output(output));
    }

    /// Save volume once it settles (not on every slider step) and reselect the
    /// remembered speaker as soon as it shows up on the network.
    fn persist_playback(&mut self, st: &Status) {
        if self.volume_drag.is_none() && (st.volume - self.saved_volume).abs() > 0.001 {
            self.saved_volume = st.volume;
            settings::set("volume", &format!("{:.3}", st.volume));
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

    fn poll_tasks(&mut self, ctx: &egui::Context) {
        match tasks::poll(&mut self.login) {
            Some(Ok(client)) => self.on_logged_in(ctx, client),
            Some(Err(e)) => {
                self.remember_login = false;
                self.login_error = Some(e.to_string());
            }
            None => {}
        }
        match tasks::poll(&mut self.tracks_task) {
            Some(Ok(t)) => self.tracks = t,
            Some(Err(e)) => self.on_load_error(e),
            None => {}
        }
        match tasks::poll(&mut self.link_task) {
            Some(Ok(link)) => self.open_link(ctx, link),
            Some(Err(e)) => self.on_load_error(e),
            None => {}
        }
        match tasks::poll(&mut self.search_task) {
            Some(Ok(results)) => {
                self.tracks = results.tracks;
                self.search_sections = results.sections;
                // The listener's own playlists first.
                if let Some(own) = own_playlists(&self.playlists, &self.searched) {
                    self.search_sections.insert(0, own);
                }
            }
            Some(Err(e)) => self.on_load_error(e),
            None => {}
        }
        match tasks::poll(&mut self.artist_task) {
            Some(Ok(page)) => self.artist_page = Some(page),
            Some(Err(e)) => self.on_load_error(e),
            None => {}
        }
        match tasks::poll(&mut self.followed_task) {
            Some(Ok(artists)) => self.followed = artists,
            Some(Err(e)) => self.on_load_error(e),
            None => {}
        }
        match tasks::poll(&mut self.playlists_task) {
            Some(Ok(p)) => self.playlists = p,
            Some(Err(e)) => self.on_load_error(e),
            None => {}
        }
        match tasks::poll(&mut self.home_task) {
            Some(Ok(h)) => self.home = h,
            Some(Err(e)) => self.on_load_error(e),
            None => {}
        }
        match tasks::poll(&mut self.quick_play) {
            Some(Ok((tracks, mode))) if !tracks.is_empty() => self.player.send(mode.command(tracks)),
            Some(Err(e)) => self.on_load_error(e),
            _ => {}
        }
    }

    /// A background load failed: an expired session sends the listener back to the
    /// login screen with the reason; anything else is shown in place.
    fn on_load_error(&mut self, error: Error) {
        log_warn!("{error}");
        if error.is_session_expired() {
            self.log_out();
            self.login_error = Some(error.to_string());
        } else {
            self.list_error = Some(error.to_string());
        }
    }

    /// Follow the chosen appearance (and the OS setting live when on System).
    fn sync_appearance(&self, ctx: &egui::Context) {
        let dark = self.appearance.is_dark(ctx);
        if dark != style::palette::is_dark() {
            style::apply(ctx, dark);
        }
    }
}

/// The account's playlists whose title contains `query` (case-insensitive), as a
/// search section; `None` when nothing matches.
fn own_playlists(playlists: &[Playlist], query: &str) -> Option<Section> {
    let query = query.trim().to_lowercase();
    let items: Vec<Item> = playlists
        .iter()
        .filter(|p| !query.is_empty() && p.title.to_lowercase().contains(&query))
        .map(|p| Item {
            kind: "playlist".into(),
            id: p.id.to_string(),
            title: p.title.clone(),
            subtitle: format!("{} tracks", p.count),
            picture: p.picture.clone(),
        })
        .collect();
    (!items.is_empty()).then(|| Section { title: "Your playlists".into(), layout: "search".into(), items })
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
        self.updates.apply_on_exit();
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        log_repaint_causes(&ctx);
        self.sync_appearance(&ctx);
        self.covers.begin_frame();
        self.poll_tasks(&ctx);
        self.updates.poll(&ctx);
        if self.client.is_none() {
            self.login_screen(ui);
            return;
        }
        if !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::Space)) {
            self.player.send(Cmd::Toggle);
        }
        self.mini_player_shortcut(&ctx);
        let st = self.player.status();
        self.persist_playback(&st);
        if let Some(np) = &mut self.now_playing {
            np.update(&st);
        }
        let p = colors();
        if self.mini_player.is_some() {
            egui::CentralPanel::default().frame(panel_frame(p.bar, Margin::ZERO)).show(ui, |ui| self.mini_player(ui, &st));
            // An artist link in the mini player opens the full window on their page.
            if let Some(artist) = self.pending_artist.take() {
                self.toggle_mini_player(&ctx);
                self.open_artist(&ctx, ArtistView::from_ref(&artist));
            }
            return;
        }
        egui::Panel::bottom("player")
            .exact_size(metrics::PLAYER_BAR_HEIGHT)
            .resizable(false)
            .show_separator_line(false)
            .frame(panel_frame(p.bar, Margin::symmetric(20, 0)))
            .show(ui, |ui| self.player_bar(ui, &st));
        egui::Panel::left("nav")
            .exact_size(metrics::SIDEBAR_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(panel_frame(p.sidebar, Margin { left: 12, right: 12, top: 0, bottom: 10 }))
            .show(ui, |ui| self.sidebar(ui));
        if self.show_queue {
            egui::Panel::right("queue")
                .exact_size(metrics::QUEUE_WIDTH)
                .resizable(false)
                .show_separator_line(false)
                .frame(panel_frame(p.sidebar, Margin { left: 14, right: 14, top: 0, bottom: 10 }))
                .show(ui, |ui| self.queue_panel(ui, &st));
        }
        let margin = metrics::CONTENT_MARGIN;
        egui::CentralPanel::default()
            .frame(panel_frame(p.bg, Margin { left: margin, right: margin, top: 0, bottom: 0 }))
            .show(ui, |ui| self.content(ui, &st));
        if let Some(artist) = self.pending_artist.take() {
            self.open_artist(&ctx, ArtistView::from_ref(&artist));
        }
    }
}

/// DUST_LOG=trace: print what requested each frame (to hunt idle redraws).
fn log_repaint_causes(ctx: &egui::Context) {
    log_trace!("frame {}: {:?}", ctx.cumulative_pass_nr(), ctx.repaint_causes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_playlists_match_case_insensitively() {
        let p = |id, title: &str| Playlist { id, title: title.into(), count: 3, picture: None };
        let mine = [p(1, "Rock Essentials"), p(2, "Rock & Chill"), p(3, "trip-hop mix")];
        let found = own_playlists(&mine, "  rock ").unwrap();
        assert_eq!(found.title, "Your playlists");
        assert_eq!(found.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["1", "2"]);
        assert_eq!(found.items[0].subtitle, "3 tracks");
        assert!(own_playlists(&mine, "jazz").is_none());
        assert!(own_playlists(&mine, "").is_none());
    }
}
