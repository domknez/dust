//! The [`App`]: UI state, its lifecycle (login, background loads, settings) and the
//! top-level panel layout. Screens are drawn by the modules in `views/`.

use super::covers::Covers;
use super::state::{Coll, PlayMode, Source, View};
use super::style::{self, Appearance, colors, metrics};
use super::tasks::{self, Task};
use crate::credentials;
use crate::deezer::{Deezer, Playlist, Quality, Section, Track};
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
    pub(super) list_error: Option<String>,
    pub(super) playlists: Vec<Playlist>,
    pub(super) playlists_task: Task<Vec<Playlist>>,
    pub(super) home: Vec<Section>,
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
        style::install_fonts(ctx);
        style::apply(ctx, appearance.is_dark(ctx));

        let player = PlayerHandle::spawn(ctx.clone(), volume);
        let repaint = ctx.clone();
        let discovery = Discovery::start(move || repaint.request_repaint())
            .inspect_err(|e| eprintln!("dust: AirPlay discovery unavailable: {e}"))
            .ok();
        let mut app = Self {
            _remote_control: start_remote_control(player.clone()),
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
            list_error: None,
            playlists: Vec::new(),
            playlists_task: None,
            home: Vec::new(),
            home_task: None,
            quick_play: None,
            show_queue: false,
            queue_drag: None,
            seek_drag: None,
            volume_drag: None,
            unmuted_volume: volume,
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
        self.login = tasks::spawn(ctx, || Deezer::login(&crate::login::obtain_arl()?));
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
        self.tracks.clear();
        self.tracks_task = match view {
            View::Home | View::Playlists => None,
            View::Search if self.search.trim().is_empty() => None,
            View::Search => {
                let query = self.search.trim().to_string();
                self.searched = query.clone();
                tasks::spawn(ctx, move || client.search(&query))
            }
            View::Loved => tasks::spawn(ctx, move || client.loved()),
            View::Collection(coll) => tasks::spawn(ctx, move || coll.source.fetch(&client)),
        };
    }

    pub(super) fn open_collection(&mut self, ctx: &egui::Context, coll: Coll) {
        self.open_view(ctx, View::Collection(Box::new(coll)));
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
                self.login_error = Some(e);
            }
            None => {}
        }
        match tasks::poll(&mut self.tracks_task) {
            Some(Ok(t)) => self.tracks = t,
            Some(Err(e)) => self.list_error = Some(e),
            None => {}
        }
        if let Some(Ok(p)) = tasks::poll(&mut self.playlists_task) {
            self.playlists = p;
        }
        match tasks::poll(&mut self.home_task) {
            Some(Ok(h)) => self.home = h,
            Some(Err(e)) => self.list_error = Some(e),
            None => {}
        }
        if let Some(Ok((tracks, mode))) = tasks::poll(&mut self.quick_play)
            && !tracks.is_empty()
        {
            self.player.send(mode.command(tracks));
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
    .inspect_err(|e| eprintln!("dust: speaker remote control unavailable: {e}"))
    .ok()
}

fn panel_frame(fill: egui::Color32, margin: Margin) -> egui::Frame {
    egui::Frame::new().fill(fill).inner_margin(margin)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        log_repaint_causes(&ctx);
        self.sync_appearance(&ctx);
        self.covers.begin_frame();
        self.poll_tasks(&ctx);
        if self.client.is_none() {
            self.login_screen(ui);
            return;
        }
        if !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::Space)) {
            self.player.send(Cmd::Toggle);
        }
        let st = self.player.status();
        self.persist_playback(&st);
        let p = colors();
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
    }
}

/// DUST_DEBUG_REPAINT=1: print what requested each frame (to hunt idle redraws).
fn log_repaint_causes(ctx: &egui::Context) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("DUST_DEBUG_REPAINT").is_some()) {
        eprintln!("frame {}: {:?}", ctx.cumulative_pass_nr(), ctx.repaint_causes());
    }
}
