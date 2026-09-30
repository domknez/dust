//! egui front-end. Reactive: nothing is redrawn unless there is input, a background
//! task finishes, or a track is playing (twice a second for the progress bar).

mod covers;
mod icons;
mod theme;
mod widgets;

use crate::deezer::{Deezer, Item, Playlist, Quality, Section, Track, image_url};
use crate::output::airplay::Discovery;
use crate::output::dacp::{self, Remote};
use crate::player::{Cmd, Output, PlayerHandle, State, Status};
use covers::Covers;
use eframe::egui::{self, Align2, Color32, CornerRadius, Key, Margin, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
use icons::Icon;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;
use theme::*;
use widgets::text_left;

const KEYRING_SERVICE: &str = "dust";
const KEYRING_USER: &str = "arl";
/// Room for the macOS traffic lights over the full-size content view.
const TOP_INSET: f32 = if cfg!(target_os = "macos") { 34.0 } else { 14.0 };
const ROW_H: f32 = 56.0;
const THUMB: u32 = 80;

type Task<T> = Option<Receiver<Result<T, String>>>;

fn spawn<T: Send + 'static>(ctx: &egui::Context, f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Task<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(f());
        ctx.request_repaint();
    });
    Some(rx)
}

/// Some(result) once the task finished.
fn poll<T>(task: &mut Task<T>) -> Option<Result<T, String>> {
    let r = match task.as_ref()?.try_recv() {
        Ok(r) => r,
        Err(TryRecvError::Empty) => return None,
        Err(TryRecvError::Disconnected) => Err("task failed".into()),
    };
    *task = None;
    Some(r)
}

fn keyring() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()
}

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn total_duration(tracks: &[Track]) -> String {
    let secs: u64 = tracks.iter().map(|t| t.duration as u64).sum();
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    if h > 0 { format!("{h} hr {m} min") } else { format!("{m} min") }
}

fn cover_url(t: &Track, size: u32) -> Option<String> {
    (!t.cover.is_empty()).then(|| image_url("cover", &t.cover, size))
}

fn playlist_url(p: &Playlist, size: u32) -> Option<String> {
    p.picture.as_ref().map(|(kind, md5)| image_url(kind, md5, size))
}

/// What to do with fetched tracks.
#[derive(Clone, Copy, PartialEq, Debug)]
enum PlayMode {
    Now,
    Next,
    Queue,
}

impl PlayMode {
    fn cmd(self, tracks: Vec<Track>) -> Cmd {
        match self {
            PlayMode::Now => Cmd::Play(tracks, 0),
            PlayMode::Next => Cmd::PlayNext(tracks),
            PlayMode::Queue => Cmd::Enqueue(tracks),
        }
    }
}

/// Right-click menu offering Play next / Add to queue.
fn queue_menu(resp: &egui::Response) -> Option<PlayMode> {
    let mut pick = None;
    resp.context_menu(|ui| {
        // Menu rows fill the available width; cap it or the menu spans the window.
        ui.set_min_width(190.0);
        ui.set_max_width(190.0);
        if widgets::menu_row(ui, Some(Icon::Play), "Play next", None, false, c().text).clicked() {
            pick = Some(PlayMode::Next);
            ui.close();
        }
        if widgets::menu_row(ui, Some(Icon::Queue), "Add to queue", None, false, c().text).clicked() {
            pick = Some(PlayMode::Queue);
            ui.close();
        }
    });
    pick
}

/// Where a collection's tracks come from.
#[derive(Clone, PartialEq, Debug)]
enum Source {
    Playlist(u64),
    Album(String),
    Artist(String),
    Mix(String),
}

/// A track collection opened from the sidebar or the home page.
#[derive(Clone, PartialEq, Debug)]
struct Coll {
    source: Source,
    kind: &'static str,
    title: String,
    subtitle: String,
    picture: Option<(String, String)>,
}

impl Coll {
    fn from_playlist(p: &Playlist) -> Self {
        Coll { source: Source::Playlist(p.id), kind: "PLAYLIST", title: p.title.clone(), subtitle: String::new(), picture: p.picture.clone() }
    }

    /// None for item types we don't open (flows play directly; channels, shows, ...).
    fn from_item(it: &Item) -> Option<Self> {
        let (source, kind) = match it.kind.as_str() {
            "playlist" => (Source::Playlist(it.id.parse().ok()?), "PLAYLIST"),
            "album" => (Source::Album(it.id.clone()), "ALBUM"),
            "artist" => (Source::Artist(it.id.clone()), "ARTIST · TOP TRACKS"),
            "smarttracklist" => (Source::Mix(it.id.clone()), "MIX"),
            _ => return None,
        };
        Some(Coll { source, kind, title: it.title.clone(), subtitle: it.subtitle.clone(), picture: it.picture.clone() })
    }

    fn round(&self) -> bool {
        matches!(self.source, Source::Artist(_))
    }
}

fn fetch(client: &Deezer, source: &Source) -> Result<Vec<Track>, String> {
    match source {
        Source::Playlist(id) => client.playlist(*id),
        Source::Album(id) => client.album(id),
        Source::Artist(id) => client.artist_top(id),
        Source::Mix(id) => client.mix(id),
    }
}

#[derive(Clone, PartialEq)]
enum View {
    Home,
    Search,
    Loved,
    Playlists,
    Collection(Box<Coll>),
}

pub struct App {
    player: PlayerHandle,
    client: Option<Deezer>,
    arl: String,
    login: Task<Deezer>,
    /// Store the ARL in the keychain once the pending login succeeds.
    remember: bool,
    show_manual: bool,
    login_error: Option<String>,
    view: View,
    search: String,
    searched: String,
    tracks: Vec<Track>,
    tracks_task: Task<Vec<Track>>,
    list_error: Option<String>,
    playlists: Vec<Playlist>,
    playlists_task: Task<Vec<Playlist>>,
    home: Vec<Section>,
    home_task: Task<Vec<Section>>,
    /// Tracks fetched to play straight from a home card.
    quick_play: Task<(Vec<Track>, PlayMode)>,
    show_queue: bool,
    /// Queue row being dragged (index into the queue).
    queue_drag: Option<usize>,
    seek_drag: Option<f32>,
    volume_drag: Option<f32>,
    unmuted_volume: f32,
    quality: Quality,
    discovery: Option<Discovery>,
    covers: Covers,
    theme_mode: theme::Mode,
    report_listens: bool,
    logo: egui::TextureHandle,
    /// Last volume written to settings.
    saved_volume: f32,
    /// Speaker (by name) to reselect once discovery finds it.
    restore_output: Option<String>,
    _dacp: Option<dacp::Server>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        let saved = crate::settings::load();
        let theme_mode = saved.get("theme").and_then(|k| theme::Mode::from_key(k)).unwrap_or(theme::Mode::System);
        let quality = saved.get("quality").and_then(|k| Quality::from_key(k)).unwrap_or(Quality::Mp3_320);
        let report_listens = saved.get("report_listens").is_none_or(|v| v != "false");
        theme::install_fonts(ctx);
        theme::apply(ctx, theme_mode.resolve(ctx));

        let volume = saved.get("volume").and_then(|v| v.parse::<f32>().ok()).map_or(0.5, |v| v.clamp(0.0, 1.0));
        let restore_output = saved.get("output").filter(|o| !o.is_empty()).cloned();
        let player = PlayerHandle::spawn(ctx.clone(), volume);
        let repaint = ctx.clone();
        let discovery = Discovery::start(move || repaint.request_repaint())
            .inspect_err(|e| eprintln!("dust: AirPlay discovery unavailable: {e}"))
            .ok();
        // Speaker buttons (volume, play/pause, skip) come back to us over DACP.
        let remote = player.clone();
        let dacp_server = dacp::start(move |r| {
            remote.send(match r {
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
        .ok();
        let mut app = Self {
            player,
            client: None,
            arl: String::new(),
            login: None,
            remember: false,
            show_manual: false,
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
            discovery,
            covers: Covers::new(ctx),
            theme_mode,
            report_listens,
            logo: crate::icon::logo_texture(ctx),
            saved_volume: volume,
            restore_output,
            _dacp: dacp_server,
        };
        if let Some(arl) = keyring().and_then(|k| k.get_password().ok()) {
            app.login = spawn(ctx, move || Deezer::login(&arl));
        }
        app
    }

    fn start_login(&mut self, ctx: &egui::Context, arl: String) {
        self.login_error = None;
        self.remember = true;
        self.login = spawn(ctx, move || Deezer::login(&arl));
    }

    #[cfg(feature = "login-window")]
    fn start_browser_login(&mut self, ctx: &egui::Context) {
        self.login_error = None;
        self.remember = true;
        self.login = spawn(ctx, || Deezer::login(&crate::login::obtain_arl()?));
    }

    fn open_view(&mut self, ctx: &egui::Context, view: View) {
        let Some(client) = self.client.clone() else { return };
        self.view = view.clone();
        self.list_error = None;
        if view == View::Home {
            // Cached; the refresh button clears it to force a reload.
            if self.home.is_empty() && self.home_task.is_none() {
                self.home_task = spawn(ctx, move || client.home());
            }
            return;
        }
        self.tracks.clear();
        self.tracks_task = match view {
            View::Home | View::Playlists => None,
            View::Search if self.search.trim().is_empty() => None,
            View::Search => {
                let q = self.search.trim().to_string();
                self.searched = q.clone();
                spawn(ctx, move || client.search(&q))
            }
            View::Loved => spawn(ctx, move || client.loved()),
            View::Collection(c) => spawn(ctx, move || fetch(&client, &c.source)),
        };
    }

    /// Fetch a collection and start playing it without opening it.
    fn play_source(&mut self, ctx: &egui::Context, source: Source, mode: PlayMode) {
        let Some(client) = self.client.clone() else { return };
        self.quick_play = spawn(ctx, move || fetch(&client, &source).map(|t| (t, mode)));
    }

    fn poll_tasks(&mut self, ctx: &egui::Context) {
        match poll(&mut self.login) {
            Some(Ok(client)) => {
                if std::mem::take(&mut self.remember)
                    && let Some(k) = keyring()
                {
                    let _ = k.set_password(client.arl());
                }
                self.arl.clear();
                self.player.send(Cmd::Client(client.clone()));
                self.player.send(Cmd::Quality(self.quality));
                self.player.send(Cmd::ReportListens(self.report_listens));
                let c = client.clone();
                self.playlists_task = spawn(ctx, move || c.playlists());
                self.client = Some(client);
                self.open_view(ctx, View::Home);
            }
            Some(Err(e)) => {
                self.remember = false;
                self.login_error = Some(e);
            }
            None => {}
        }
        match poll(&mut self.tracks_task) {
            Some(Ok(t)) => self.tracks = t,
            Some(Err(e)) => self.list_error = Some(e),
            None => {}
        }
        if let Some(Ok(p)) = poll(&mut self.playlists_task) {
            self.playlists = p;
        }
        match poll(&mut self.home_task) {
            Some(Ok(h)) => self.home = h,
            Some(Err(e)) => self.list_error = Some(e),
            None => {}
        }
        if let Some(Ok((tracks, mode))) = poll(&mut self.quick_play)
            && !tracks.is_empty()
        {
            self.player.send(mode.cmd(tracks));
        }
    }

    fn logout(&mut self) {
        if let Some(k) = keyring() {
            let _ = k.delete_credential();
        }
        self.client = None;
        self.tracks.clear();
        self.playlists.clear();
    }

    // ------------------------------------------------------------ login

    fn login_screen(&mut self, ui: &mut Ui) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(c().sidebar)).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.3).max(40.0));
                ui.add(egui::Image::from_texture((self.logo.id(), Vec2::splat(112.0))));
                ui.add_space(10.0);
                let (r, _) = ui.allocate_exact_size(vec2(200.0, 56.0), Sense::hover());
                let g = ui.painter().layout_no_wrap("dust".into(), bold(48.0), c().text);
                ui.painter().galley(pos2(r.center().x - g.size().x / 2.0, r.center().y - g.size().y / 2.0), g, c().text);
                ui.label(egui::RichText::new("Your music, nothing else.").color(c().dim).size(15.0));
                ui.add_space(36.0);
                if self.login.is_some() {
                    ui.spinner();
                    return;
                }
                #[cfg(feature = "login-window")]
                {
                    if widgets::pill(ui, "Log in with Deezer", None, true).clicked() {
                        self.start_browser_login(ui.ctx());
                    }
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Deezer Premium required").color(c().faint).size(12.0));
                    ui.add_space(18.0);
                    if ui.link(egui::RichText::new("Paste ARL cookie instead").size(13.0)).clicked() {
                        self.show_manual = !self.show_manual;
                    }
                }
                #[cfg(not(feature = "login-window"))]
                {
                    self.show_manual = true;
                }
                if self.show_manual {
                    ui.add_space(10.0);
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.arl).password(true).hint_text("arl").desired_width(340.0).margin(Margin::symmetric(12, 8)),
                    );
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !self.arl.trim().is_empty() {
                        let arl = self.arl.trim().to_string();
                        self.start_login(ui.ctx(), arl);
                    }
                    ui.label(egui::RichText::new("deezer.com → DevTools → Application → Cookies → arl").color(c().faint).size(12.0));
                }
                if let Some(e) = &self.login_error {
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(e).color(c().danger));
                }
            });
        });
    }

    // ------------------------------------------------------------ sidebar

    fn sidebar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(TOP_INSET);
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
        text_left(ui.painter(), pos2(row.left() + 8.0, row.center().y), "dust", bold(26.0), c().text, 160.0);
        ui.add_space(18.0);

        let mut go = None;
        if widgets::nav_item(ui, Icon::Home, "Home", self.view == View::Home).clicked() {
            go = Some(View::Home);
        }
        if widgets::nav_item(ui, Icon::Search, "Search", self.view == View::Search).clicked() {
            self.view = View::Search;
            self.tracks.clear();
            self.searched.clear();
        }
        section(ui, "YOUR COLLECTION");
        if widgets::nav_item(ui, Icon::Heart, "Tracks", self.view == View::Loved).clicked() {
            go = Some(View::Loved);
        }
        if widgets::nav_item(ui, Icon::Grid, "Playlists", self.view == View::Playlists).clicked() {
            go = Some(View::Playlists);
        }
        section(ui, "PLAYLISTS");

        let footer = 56.0;
        egui::ScrollArea::vertical().max_height((ui.available_height() - footer).max(0.0)).auto_shrink([false, false]).show(ui, |ui| {
            if self.playlists_task.is_some() {
                ui.spinner();
            }
            for p in &self.playlists {
                let selected = matches!(&self.view, View::Collection(c) if c.source == Source::Playlist(p.id));
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
                if selected || resp.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), if selected { c().surface } else { c().hover });
                }
                let img = Rect::from_min_size(rect.min + vec2(6.0, 6.0), Vec2::splat(36.0));
                widgets::cover(ui, &mut self.covers, playlist_url(p, 80).as_deref(), img, 4);
                let x = img.right() + 12.0;
                let w = rect.right() - x - 6.0;
                text_left(ui.painter(), pos2(x, rect.center().y - 8.0), &p.title, regular(13.5), if selected { c().text } else { c().dim }, w);
                text_left(ui.painter(), pos2(x, rect.center().y + 9.0), &format!("{} tracks", p.count), regular(11.5), c().faint, w);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    go = Some(View::Collection(Box::new(Coll::from_playlist(p))));
                }
            }
        });
        if let Some(v) = go {
            self.open_view(&ctx, v);
        }

        // Account / settings menu.
        ui.add_space(6.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(6), c().hover);
        }
        let name = self.client.as_ref().map(|c| c.name().to_string()).unwrap_or_default();
        let avatar = pos2(rect.left() + 22.0, rect.center().y);
        ui.painter().circle_filled(avatar, 15.0, c().accent);
        let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
        ui.painter().text(avatar, Align2::CENTER_CENTER, initial, bold(14.0), c().bg);
        text_left(ui.painter(), pos2(rect.left() + 46.0, rect.center().y - 7.0), &name, bold(13.0), c().text, rect.width() - 56.0);
        text_left(ui.painter(), pos2(rect.left() + 46.0, rect.center().y + 9.0), self.quality.label(), regular(11.5), c().faint, rect.width() - 56.0);
        let menu_frame = widgets::popover_frame(ui);
        egui::Popup::menu(&resp).width(272.0).gap(8.0).frame(menu_frame).show(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            // Account header.
            let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
            let avatar = pos2(head.left() + 22.0, head.center().y);
            ui.painter().circle_filled(avatar, 17.0, c().accent);
            let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            ui.painter().text(avatar, Align2::CENTER_CENTER, initial, bold(15.0), c().bg);
            text_left(ui.painter(), pos2(head.left() + 48.0, head.center().y - 8.0), &name, bold(14.0), c().text, head.width() - 56.0);
            text_left(ui.painter(), pos2(head.left() + 48.0, head.center().y + 10.0), "Deezer account", regular(12.0), c().faint, head.width() - 56.0);
            widgets::divider(ui);

            widgets::caption(ui, "APPEARANCE");
            let modes = theme::Mode::ALL.map(|m| {
                let icon = match m {
                    theme::Mode::System => Icon::Computer,
                    theme::Mode::Dark => Icon::Moon,
                    theme::Mode::Light => Icon::Sun,
                };
                (icon, m.label())
            });
            let current = theme::Mode::ALL.iter().position(|m| *m == self.theme_mode).unwrap_or(0);
            if let Some(i) = widgets::segmented(ui, &modes, current) {
                self.theme_mode = theme::Mode::ALL[i];
                crate::settings::set("theme", self.theme_mode.key());
            }
            ui.add_space(8.0);

            widgets::caption(ui, "STREAMING QUALITY");
            for q in Quality::ALL {
                let sub = match q {
                    Quality::Mp3_128 => "Saves data",
                    Quality::Mp3_320 => "High quality",
                    Quality::Flac => "Lossless · needs a HiFi plan",
                };
                if widgets::menu_row(ui, None, q.label(), Some(sub), self.quality == q, c().text).clicked() {
                    self.quality = q;
                    self.player.send(Cmd::Quality(q));
                    crate::settings::set("quality", q.key());
                }
            }
            ui.add_space(8.0);
            widgets::caption(ui, "LISTENING");
            let sub = "History, Flow and Last.fm scrobbling via Deezer";
            if widgets::menu_row(ui, None, "Share listening with Deezer", Some(sub), self.report_listens, c().text).clicked() {
                self.report_listens = !self.report_listens;
                self.player.send(Cmd::ReportListens(self.report_listens));
                crate::settings::set("report_listens", if self.report_listens { "true" } else { "false" });
            }
            widgets::divider(ui);
            if widgets::menu_row(ui, Some(Icon::LogOut), "Log out", None, false, c().danger).clicked() {
                self.logout();
                ui.close();
            }
        });
    }

    // ------------------------------------------------------------ main area

    fn main_area(&mut self, ui: &mut Ui, st: &Status) {
        let ctx = ui.ctx().clone();
        ui.add_space(TOP_INSET);
        // Search field + transient error.
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(360.0f32.min(ui.available_width()), 38.0), Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(19), c().surface);
            icons::paint(ui.painter(), Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(15.0)), Icon::Search, c().dim);
            let inner = Rect::from_min_max(pos2(rect.left() + 38.0, rect.top() + 9.0), pos2(rect.right() - 14.0, rect.bottom() - 7.0));
            let edit = ui.put(
                inner,
                egui::TextEdit::singleline(&mut self.search).hint_text("Search tracks, artists, albums").frame(egui::Frame::new()).text_color(c().text),
            );
            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                self.open_view(&ctx, View::Search);
            }
            if let Some(e) = &st.error {
                ui.add_space(12.0);
                ui.label(egui::RichText::new(e).color(c().danger).size(12.5));
            }
        });
        ui.add_space(20.0);

        if self.view == View::Playlists {
            self.playlist_grid(ui);
            return;
        }
        if self.view == View::Home {
            self.home_view(ui, st);
            return;
        }
        let playing_id = st.track.as_ref().map(|t| t.id);
        let playing = st.state == State::Playing;
        self.track_view(ui, playing_id, playing);
    }

    fn header_info(&self) -> Header {
        match &self.view {
            View::Loved => Header { kind: "COLLECTION", title: "Tracks".into(), tile: Some((Icon::Heart, c().tile_loved)), ..Default::default() },
            View::Collection(coll) => Header {
                kind: coll.kind,
                title: coll.title.clone(),
                subtitle: coll.subtitle.clone(),
                art: coll.picture.as_ref().map(|(k, m)| image_url(k, m, 400)),
                tile: None,
                round: coll.round(),
            },
            View::Home | View::Search | View::Playlists => Header::default(),
        }
    }

    fn track_view(&mut self, ui: &mut Ui, playing_id: Option<u64>, playing: bool) {
        let search = self.view == View::Search;
        let header_h = if search { 96.0 } else { 268.0 };
        let n = self.tracks.len();
        let mut play: Option<(usize, bool)> = None;
        let mut queue_action: Option<(usize, PlayMode)> = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            ui.set_height(header_h + n as f32 * ROW_H + 24.0);
            let origin = ui.max_rect().min;

            // ---- header
            let head = Rect::from_min_size(origin, vec2(width, header_h));
            ui.scope_builder(UiBuilder::new().max_rect(head), |ui| {
                if search {
                    let title = if self.searched.is_empty() { "Search".to_string() } else { format!("“{}”", self.searched) };
                    text_left(ui.painter(), head.min + vec2(0.0, 22.0), &title, bold(30.0), c().text, width);
                    if !self.tracks.is_empty() {
                        text_left(ui.painter(), head.min + vec2(0.0, 54.0), &format!("{n} tracks"), regular(13.0), c().dim, width);
                    }
                } else {
                    let Header { kind, title, subtitle, art, tile, round } = self.header_info();
                    let art_rect = Rect::from_min_size(head.min, Vec2::splat(200.0));
                    match (art, tile) {
                        (Some(url), _) => widgets::cover(ui, &mut self.covers, Some(&url), art_rect, if round { 100 } else { 8 }),
                        (None, Some((icon, color))) => widgets::tile(ui, art_rect, icon, color, 8),
                        _ => widgets::cover(ui, &mut self.covers, None, art_rect, 8),
                    }
                    let x = art_rect.right() + 28.0;
                    let tw = width - (x - head.left());
                    text_left(ui.painter(), pos2(x, head.top() + 40.0), kind, bold(11.5), c().dim, tw);
                    text_left(ui.painter(), pos2(x, head.top() + 82.0), &title, bold(40.0), c().text, tw);
                    let mut meta = if n > 0 { format!("{n} tracks · {}", total_duration(&self.tracks)) } else { String::new() };
                    if !subtitle.is_empty() {
                        meta = if meta.is_empty() { subtitle } else { format!("{subtitle}  ·  {meta}") };
                    }
                    text_left(ui.painter(), pos2(x, head.top() + 124.0), &meta, regular(13.5), c().dim, tw);
                    let buttons = Rect::from_min_size(pos2(x, head.top() + 152.0), vec2(tw, 44.0));
                    ui.scope_builder(UiBuilder::new().max_rect(buttons).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                        if n > 0 && widgets::pill(ui, "Play", Some(Icon::Play), true).clicked() {
                            play = Some((0, false));
                        }
                        if n > 1 && widgets::pill(ui, "Shuffle", Some(Icon::Shuffle), false).clicked() {
                            play = Some((0, true));
                        }
                    });
                }
                // Column captions.
                if n > 0 {
                    let y = head.bottom() - 22.0;
                    let cols = columns(head.left(), width);
                    let p = ui.painter();
                    p.text(pos2(cols.index + 14.0, y), Align2::CENTER_CENTER, "#", bold(11.0), c().faint);
                    p.text(pos2(cols.title, y), Align2::LEFT_CENTER, "TITLE", bold(11.0), c().faint);
                    if let Some(a) = cols.album {
                        p.text(pos2(a, y), Align2::LEFT_CENTER, "ALBUM", bold(11.0), c().faint);
                    }
                    p.text(pos2(cols.time, y), Align2::RIGHT_CENTER, "TIME", bold(11.0), c().faint);
                    p.hline(head.x_range(), head.bottom() - 4.0, egui::Stroke::new(1.0, c().line));
                }
            });

            if self.tracks_task.is_some() {
                let r = Rect::from_min_size(origin + vec2(0.0, header_h + 24.0), vec2(width, 40.0));
                ui.put(r, egui::Spinner::new().size(22.0));
                return;
            }
            if let Some(e) = &self.list_error {
                text_left(ui.painter(), origin + vec2(0.0, header_h + 24.0), e, regular(14.0), c().danger, width);
                return;
            }

            // ---- rows (virtualised)
            let first = ((viewport.top() - header_h) / ROW_H).floor().max(0.0) as usize;
            let last = (((viewport.bottom() - header_h) / ROW_H).ceil().max(0.0) as usize).min(n);
            let cols = columns(origin.x, width);
            for i in first..last {
                let rect = Rect::from_min_size(origin + vec2(0.0, header_h + i as f32 * ROW_H), vec2(width, ROW_H));
                let resp = ui.interact(rect, ui.id().with(("row", i)), Sense::click());
                let t = &self.tracks[i];
                let current = playing_id == Some(t.id);
                let hovered = resp.hovered();
                let p = ui.painter();
                if hovered {
                    p.rect_filled(rect, CornerRadius::same(6), c().hover);
                }
                let cy = rect.center().y;
                let idx = Rect::from_center_size(pos2(cols.index + 14.0, cy), Vec2::splat(14.0));
                if hovered {
                    icons::paint(p, idx, if current && playing { Icon::Pause } else { Icon::Play }, c().text);
                } else if current {
                    equalizer(p, idx, playing);
                } else {
                    p.text(idx.center(), Align2::CENTER_CENTER, (i + 1).to_string(), regular(13.0), c().faint);
                }
                let art = Rect::from_min_size(pos2(cols.art, cy - 20.0), Vec2::splat(40.0));
                widgets::cover(ui, &mut self.covers, cover_url(t, THUMB).as_deref(), art, 4);
                let p = ui.painter();
                let tw = cols.album.unwrap_or(cols.time - 60.0) - cols.title - 16.0;
                text_left(p, pos2(cols.title, cy - 9.0), &t.title, regular(14.0), if current { c().accent } else { c().text }, tw);
                text_left(p, pos2(cols.title, cy + 10.0), &t.artist, regular(12.5), c().dim, tw);
                if let Some(a) = cols.album {
                    text_left(p, pos2(a, cy), &t.album, regular(13.0), c().dim, cols.time - a - 70.0);
                }
                p.text(pos2(cols.time, cy), Align2::RIGHT_CENTER, mmss(t.duration as f64), regular(13.0), c().dim);

                let clicked_index = resp.clicked() && resp.interact_pointer_pos().is_some_and(|pt| pt.x < cols.art);
                if resp.double_clicked() || clicked_index {
                    if current && clicked_index {
                        self.player.send(Cmd::Toggle);
                    } else {
                        play = Some((i, false));
                    }
                }
                if let Some(mode) = queue_menu(&resp) {
                    queue_action = Some((i, mode));
                }
            }
        });
        if let Some((i, mode)) = queue_action {
            self.player.send(mode.cmd(vec![self.tracks[i].clone()]));
        }

        if let Some((i, shuffle)) = play {
            let mut queue = self.tracks.clone();
            if shuffle {
                fastrand::shuffle(&mut queue);
            }
            self.player.send(Cmd::Play(queue, i));
        }
    }

    fn home_view(&mut self, ui: &mut Ui, st: &Status) {
        let ctx = ui.ctx().clone();
        let mut open: Option<Coll> = None;
        let mut play: Option<(Source, PlayMode)> = None;
        let mut flow: Option<Option<String>> = None;
        let mut reload = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(96.0, 44.0), Sense::hover());
                text_left(ui.painter(), pos2(r.left(), r.center().y), "Home", bold(30.0), c().text, r.width());
                if widgets::icon_button(ui, Icon::Refresh, 16.0, c().dim).on_hover_text("Refresh recommendations").clicked() {
                    reload = true;
                }
            });
            if self.home.is_empty() {
                if self.home_task.is_some() {
                    ui.add_space(24.0);
                    ui.add(egui::Spinner::new().size(22.0));
                } else if let Some(e) = &self.list_error {
                    ui.label(egui::RichText::new(e).color(c().danger));
                }
                return;
            }
            // Flow first, as on Deezer.
            let mut order: Vec<usize> = (0..self.home.len()).collect();
            order.sort_by_key(|&i| !self.home[i].items.iter().any(|it| it.kind == "flow"));
            for si in order {
                let sec = &self.home[si];
                let is_flow = sec.items.iter().all(|it| it.kind == "flow");
                let items: Vec<&Item> = sec.items.iter().filter(|it| it.kind == "flow" || Coll::from_item(it).is_some()).collect();
                if items.is_empty() {
                    continue;
                }
                ui.add_space(22.0);
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                text_left(ui.painter(), pos2(r.left(), r.center().y), &sec.title, bold(19.0), c().text, r.width());
                ui.add_space(8.0);
                egui::ScrollArea::horizontal()
                    .id_salt(("home-row", si))
                    .auto_shrink([false, true])
                    // Bar visibility animation on these rows never settled and kept
                    // the UI redrawing while idle; trackpad/shift-scroll still work.
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = if is_flow { 10.0 } else { 18.0 };
                        for it in items {
                            if is_flow {
                                if let Some(f) = flow_card(ui, &mut self.covers, it, st.flow.as_deref() == Some(it.id.as_str())) {
                                    flow = Some(f);
                                }
                            } else if let Some(coll) = Coll::from_item(it) {
                                match card(ui, &mut self.covers, &coll) {
                                    CardClick::Open => open = Some(coll),
                                    CardClick::Play(mode) => play = Some((coll.source.clone(), mode)),
                                    CardClick::None => {}
                                }
                            }
                        }
                    });
                });
            }
            ui.add_space(24.0);
        });
        if reload {
            self.home.clear();
            self.open_view(&ctx, View::Home);
        }
        if let Some(f) = flow {
            self.player.send(Cmd::PlayFlow(f));
        }
        if let Some((src, mode)) = play {
            self.play_source(&ctx, src, mode);
        }
        if let Some(coll) = open {
            self.open_view(&ctx, View::Collection(Box::new(coll)));
        }
    }

    fn playlist_grid(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        text_left(ui.painter(), ui.cursor().min + vec2(0.0, 18.0), "Playlists", bold(30.0), c().text, 400.0);
        ui.add_space(52.0);
        let mut open = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let card = 176.0;
            let gap = 22.0;
            let width = ui.available_width();
            let per_row = (((width + gap) / (card + gap)).floor() as usize).max(1);
            for row in self.playlists.chunks(per_row) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for p in row {
                        let (rect, resp) = ui.allocate_exact_size(vec2(card, card + 54.0), Sense::click());
                        let art = Rect::from_min_size(rect.min, Vec2::splat(card));
                        widgets::cover(ui, &mut self.covers, playlist_url(p, 352).as_deref(), art, 8);
                        if resp.hovered() {
                            ui.painter().rect_filled(art, CornerRadius::same(8), c().veil);
                            let knob = pos2(art.right() - 30.0, art.bottom() - 30.0);
                            ui.painter().circle_filled(knob, 20.0, c().text);
                            icons::paint(ui.painter(), Rect::from_center_size(knob, Vec2::splat(16.0)), Icon::Play, c().bg);
                        }
                        text_left(ui.painter(), pos2(rect.left(), art.bottom() + 16.0), &p.title, bold(14.0), c().text, card);
                        text_left(ui.painter(), pos2(rect.left(), art.bottom() + 36.0), &format!("{} tracks", p.count), regular(12.5), c().dim, card);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            open = Some(p.id);
                        }
                    }
                });
                ui.add_space(18.0);
            }
        });
        if let Some(id) = open
            && let Some(p) = self.playlists.iter().find(|p| p.id == id)
        {
            let view = View::Collection(Box::new(Coll::from_playlist(p)));
            self.open_view(&ctx, view);
        }
    }

    // ------------------------------------------------------------ queue

    fn queue_panel(&mut self, ui: &mut Ui, st: &Status) {
        const QROW: f32 = 52.0;
        ui.add_space(TOP_INSET);
        let queue = st.queue.clone();
        let current = st.index;
        let has_current = st.track.is_some() && current < queue.len();
        let upcoming = if queue.is_empty() { 0 } else { queue.len().saturating_sub(current + 1) };

        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width() - 90.0, 36.0), Sense::hover());
            text_left(ui.painter(), pos2(r.left() + 4.0, r.center().y), "Queue", bold(20.0), c().text, r.width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, Icon::Close, 14.0, c().dim).on_hover_text("Close").clicked() {
                    self.show_queue = false;
                }
                if upcoming > 0 && ui.link(egui::RichText::new("Clear").size(13.0)).clicked() {
                    self.player.send(Cmd::ClearUpcoming);
                }
            });
        });
        if st.flow.is_some() {
            text_left(ui.painter(), ui.cursor().min + vec2(4.0, 8.0), "Flow · keeps going", regular(12.0), c().accent, 300.0);
            ui.add_space(18.0);
        }

        if !has_current && upcoming == 0 {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Your queue is empty").color(c().dim).size(14.0));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Right-click a track or card → Add to queue").color(c().faint).size(12.0));
            });
            return;
        }

        let mut jump = None;
        let mut remove = None;
        if has_current {
            widgets::caption(ui, "NOW PLAYING");
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), QROW), Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(6), c().hover);
            queue_row(ui, &mut self.covers, rect, &queue[current], true, true);
            ui.add_space(10.0);
        }
        if upcoming == 0 {
            return;
        }
        widgets::caption(ui, &format!("NEXT UP · {upcoming}"));
        let first_up = current + 1;
        let mut drop_at: Option<usize> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, QROW, upcoming, |ui, range| {
            for k in range {
                let i = first_up + k;
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), QROW), Sense::click_and_drag());
                let dragging = self.queue_drag == Some(i);
                if resp.hovered() || dragging {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), c().hover);
                }
                let show_remove = resp.hovered() && self.queue_drag.is_none();
                queue_row(ui, &mut self.covers, rect, &queue[i], false, !show_remove);
                // Remove button replaces the duration on hover.
                let x = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(24.0));
                let over_x = resp.hover_pos().is_some_and(|p| x.contains(p));
                if show_remove {
                    ui.painter().rect_filled(x, CornerRadius::same(12), if over_x { c().raised } else { c().hover });
                    icons::paint(ui.painter(), Rect::from_center_size(x.center(), Vec2::splat(10.0)), Icon::Close, c().dim);
                }
                if resp.drag_started() {
                    self.queue_drag = Some(i);
                }
                if resp.clicked() {
                    if over_x {
                        remove = Some(i);
                    } else {
                        jump = Some(i);
                    }
                }
                // Drop target: the row under the pointer while dragging.
                if self.queue_drag.is_some()
                    && let Some(p) = ui.ctx().pointer_interact_pos()
                    && rect.contains(p)
                {
                    drop_at = Some(i);
                    let y = if self.queue_drag.is_some_and(|d| d < i) { rect.bottom() } else { rect.top() };
                    ui.painter().hline(rect.x_range(), y, egui::Stroke::new(2.0, c().accent));
                }
            }
        });
        if let Some(from) = self.queue_drag {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            if ui.input(|i| i.pointer.any_released()) {
                if let Some(to) = drop_at
                    && to != from
                {
                    self.player.send(Cmd::Move(from, to));
                }
                self.queue_drag = None;
            }
        }
        if let Some(i) = remove {
            self.player.send(Cmd::Remove(i));
        } else if let Some(i) = jump {
            self.player.send(Cmd::JumpTo(i));
        }
    }

    // ------------------------------------------------------------ player bar

    fn player_bar(&mut self, ui: &mut Ui, st: &Status) {
        let full = ui.max_rect();
        ui.painter().hline(full.x_range(), full.top(), egui::Stroke::new(1.0, c().line));
        let duration = st.track.as_ref().map_or(0.0, |t| t.duration as f64);
        let side = (full.width() * 0.3).min(360.0);
        let left = Rect::from_min_max(full.min, pos2(full.left() + side, full.bottom()));
        let right = Rect::from_min_max(pos2(full.right() - side, full.top()), full.max);
        let center = Rect::from_min_max(pos2(left.right() + 16.0, full.top()), pos2(right.left() - 16.0, full.bottom()));

        // Now playing.
        if let Some(t) = &st.track {
            let art = Rect::from_min_size(pos2(left.left(), left.center().y - 28.0), Vec2::splat(56.0));
            widgets::cover(ui, &mut self.covers, cover_url(t, 112).as_deref(), art, 4);
            let x = art.right() + 14.0;
            let w = left.right() - x;
            text_left(ui.painter(), pos2(x, left.center().y - 9.0), &t.title, bold(14.0), c().text, w);
            text_left(ui.painter(), pos2(x, left.center().y + 11.0), &t.artist, regular(12.5), c().dim, w);
        }
        if st.state == State::Loading {
            ui.put(Rect::from_center_size(pos2(left.left() + 28.0, left.center().y), Vec2::splat(56.0)), egui::Spinner::new().size(20.0));
        }

        // Transport + progress.
        ui.scope_builder(UiBuilder::new().max_rect(center), |ui| {
            let controls = Rect::from_center_size(pos2(center.center().x, center.top() + 30.0), vec2(160.0, 40.0));
            ui.scope_builder(UiBuilder::new().max_rect(controls).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if widgets::icon_button(ui, Icon::Prev, 16.0, c().dim).clicked() {
                    self.player.send(Cmd::Prev);
                }
                if widgets::play_circle(ui, st.state == State::Playing, 38.0).clicked() {
                    self.player.send(Cmd::Toggle);
                }
                if widgets::icon_button(ui, Icon::Next, 16.0, c().dim).clicked() {
                    self.player.send(Cmd::Next);
                }
            });
            let bar_w = center.width().min(560.0);
            let row = Rect::from_center_size(pos2(center.center().x, center.top() + 64.0), vec2(bar_w, 16.0));
            let pos = self.seek_drag.map(|f| f as f64 * duration).unwrap_or(st.position).min(duration);
            let p = ui.painter();
            p.text(pos2(row.left(), row.center().y), Align2::RIGHT_CENTER, mmss(pos), regular(11.5), c().dim);
            p.text(pos2(row.right(), row.center().y), Align2::LEFT_CENTER, mmss(duration), regular(11.5), c().dim);
            let slider = Rect::from_min_max(pos2(row.left() + 10.0, row.top()), pos2(row.right() - 10.0, row.bottom()));
            ui.scope_builder(UiBuilder::new().max_rect(slider), |ui| {
                let mut frac = if duration > 0.0 { (pos / duration) as f32 } else { 0.0 };
                let resp = widgets::thin_slider(ui, &mut frac, slider.width(), duration > 0.0);
                if resp.dragged() {
                    self.seek_drag = Some(frac);
                }
                if resp.drag_stopped() || (resp.clicked() && !resp.dragged()) {
                    self.player.send(Cmd::Seek(frac as f64 * duration));
                    self.seek_drag = None;
                }
            });
        });

        // Output + volume.
        ui.scope_builder(UiBuilder::new().max_rect(right).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let mut vol = self.volume_drag.unwrap_or(st.volume);
            let resp = widgets::thin_slider(ui, &mut vol, 104.0, true);
            if resp.changed() {
                self.volume_drag = Some(vol);
                self.player.send(Cmd::Volume(vol));
            }
            if resp.drag_stopped() || (!resp.dragged() && self.volume_drag.is_some() && !resp.hovered()) {
                self.volume_drag = None;
            }
            let icon = if vol <= 0.001 { Icon::Mute } else { Icon::Volume };
            if widgets::icon_button(ui, icon, 18.0, c().dim).on_hover_text("Mute").clicked() {
                if vol > 0.001 {
                    self.unmuted_volume = vol;
                    self.player.send(Cmd::Volume(0.0));
                } else {
                    self.player.send(Cmd::Volume(self.unmuted_volume.max(0.1)));
                }
            }
            ui.add_space(10.0);
            self.output_button(ui, &st.output);
            let color = if self.show_queue { c().accent } else { c().dim };
            if widgets::icon_button(ui, Icon::Queue, 18.0, color).on_hover_text("Queue").clicked() {
                self.show_queue = !self.show_queue;
            }
        });

        if st.state == State::Playing {
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        }
    }

    /// Save volume once it settles (not on every slider step) and reselect the
    /// remembered speaker as soon as it shows up on the network.
    fn persist_playback_settings(&mut self, st: &Status) {
        if self.volume_drag.is_none() && (st.volume - self.saved_volume).abs() > 0.001 {
            self.saved_volume = st.volume;
            crate::settings::set("volume", &format!("{:.3}", st.volume));
        }
        if let Some(name) = &self.restore_output
            && let Some(d) = self.discovery.as_ref().and_then(|disc| disc.devices().into_iter().find(|d| &d.name == name))
        {
            if d.supported && !d.password {
                self.player.send(Cmd::Output(Output::AirPlay(d)));
            }
            self.restore_output = None;
        }
    }

    fn output_button(&mut self, ui: &mut Ui, current: &str) {
        let remote = current != Output::Local.name();
        let label = if remote { current } else { "" };
        let g = ui.painter().layout_no_wrap(label.to_string(), regular(12.5), c().accent);
        let w = 34.0 + if remote { g.size().x.min(140.0) + 8.0 } else { 0.0 };
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(17), c().hover);
        }
        let color = if remote { c().accent } else if resp.hovered() { c().text } else { c().dim };
        icons::paint(ui.painter(), Rect::from_center_size(pos2(rect.left() + 17.0, rect.center().y), Vec2::splat(18.0)), Icon::AirPlay, color);
        if remote {
            text_left(ui.painter(), pos2(rect.left() + 32.0, rect.center().y), label, regular(12.5), c().accent, 140.0);
        }
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(260.0);
            ui.add_space(4.0);
            ui.label(egui::RichText::new("PLAY ON").color(c().faint).size(11.0));
            ui.add_space(2.0);
            let mut pick = None;
            if widgets::nav_item(ui, Icon::Computer, Output::Local.name(), !remote).clicked() {
                pick = Some(Output::Local);
            }
            let devices = self.discovery.as_ref().map(|d| d.devices()).unwrap_or_default();
            for d in devices.into_iter().filter(|d| d.supported && !d.password) {
                if widgets::nav_item(ui, Icon::AirPlay, &d.name, current == d.name).clicked() {
                    pick = Some(Output::AirPlay(d));
                }
            }
            if self.discovery.as_ref().is_some_and(|d| d.devices().is_empty()) {
                ui.label(egui::RichText::new("Looking for AirPlay speakers…").color(c().faint).size(12.0));
            }
            if let Some(o) = pick {
                let remembered = match &o {
                    Output::Local => String::new(),
                    Output::AirPlay(d) => d.name.clone(),
                };
                crate::settings::set("output", &remembered);
                self.restore_output = None;
                self.player.send(Cmd::Output(o));
                ui.close();
            }
        });
    }
}

/// Collection header content.
#[derive(Default)]
struct Header {
    kind: &'static str,
    title: String,
    subtitle: String,
    art: Option<String>,
    /// Placeholder tile when there is no artwork.
    tile: Option<(Icon, Color32)>,
    round: bool,
}

/// One track in the queue panel.
fn queue_row(ui: &Ui, covers: &mut Covers, rect: Rect, t: &Track, current: bool, show_time: bool) {
    let art = Rect::from_min_size(pos2(rect.left() + 6.0, rect.center().y - 18.0), Vec2::splat(36.0));
    widgets::cover(ui, covers, cover_url(t, THUMB).as_deref(), art, 4);
    let x = art.right() + 12.0;
    let w = rect.right() - x - 52.0;
    let p = ui.painter();
    text_left(p, pos2(x, rect.center().y - 8.0), &t.title, regular(13.5), if current { c().accent } else { c().text }, w);
    text_left(p, pos2(x, rect.center().y + 10.0), &t.artist, regular(12.0), c().dim, w);
    if show_time {
        p.text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, mmss(t.duration as f64), regular(12.0), c().faint);
    }
}

enum CardClick {
    None,
    Open,
    Play(PlayMode),
}

/// Home card: artwork (round for artists), title, subtitle; a play button on hover.
fn card(ui: &mut Ui, covers: &mut Covers, coll: &Coll) -> CardClick {
    let w = 168.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, w + 50.0), Sense::click());
    let art = Rect::from_min_size(rect.min, Vec2::splat(w));
    let radius = if coll.round() { 84 } else { 8 };
    let url = coll.picture.as_ref().map(|(k, m)| image_url(k, m, 336));
    widgets::cover(ui, covers, url.as_deref(), art, radius);
    let mut click = CardClick::None;
    if resp.hovered() {
        ui.painter().rect_filled(art, CornerRadius::same(radius), c().veil);
        let knob = pos2(art.right() - 28.0, art.bottom() - 28.0);
        let over_knob = resp.hover_pos().is_some_and(|p| p.distance(knob) < 22.0);
        ui.painter().circle_filled(knob, if over_knob { 22.0 } else { 20.0 }, c().text);
        icons::paint(ui.painter(), Rect::from_center_size(knob, Vec2::splat(15.0)), Icon::Play, c().bg);
        if resp.clicked() {
            click = if over_knob { CardClick::Play(PlayMode::Now) } else { CardClick::Open };
        }
    }
    if let Some(mode) = queue_menu(&resp) {
        click = CardClick::Play(mode);
    }
    text_left(ui.painter(), pos2(rect.left(), art.bottom() + 15.0), &coll.title, bold(13.5), c().text, w);
    text_left(ui.painter(), pos2(rect.left(), art.bottom() + 34.0), &coll.subtitle, regular(12.0), c().dim, w);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    click
}

/// Round Flow mood tile; returns the flow config to play when clicked.
fn flow_card(ui: &mut Ui, covers: &mut Covers, it: &Item, active: bool) -> Option<Option<String>> {
    let d = 116.0;
    // Leave room around the artwork for the selection ring so scroll areas don't clip it.
    let pad = 6.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(d + 2.0 * pad, d + 2.0 * pad + 30.0), Sense::click());
    let art = Rect::from_min_size(rect.min + Vec2::splat(pad), Vec2::splat(d));
    let url = it.picture.as_ref().map(|(k, m)| image_url(k, m, 240));
    widgets::cover(ui, covers, url.as_deref(), art, 58);
    if active || resp.hovered() {
        let color = if active { c().accent } else { c().text.gamma_multiply(0.6) };
        ui.painter().circle_stroke(art.center(), d / 2.0 + 3.0, egui::Stroke::new(2.5, color));
    }
    let g = widgets::line(ui.painter(), &it.title, if active { bold(13.0) } else { regular(13.0) }, if active { c().accent } else { c().text }, d + 10.0);
    let pos = pos2(art.center().x - g.size().x / 2.0, art.bottom() + pad + 8.0);
    ui.painter().galley(pos, g, c().text);
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    resp.clicked().then(|| (it.id != "default").then(|| it.id.clone()))
}

fn section(ui: &mut Ui, label: &str) {
    ui.add_space(18.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    text_left(ui.painter(), pos2(rect.left() + 10.0, rect.center().y), label, bold(11.0), c().faint, rect.width());
    ui.add_space(4.0);
}

/// Three little bars marking the current track.
fn equalizer(p: &egui::Painter, r: Rect, animated: bool) {
    let t = if animated { p.ctx().input(|i| i.time) as f32 } else { 0.0 };
    for (k, phase) in [0.0f32, 1.7, 3.1].into_iter().enumerate() {
        let h = if animated { 0.35 + 0.65 * (0.5 + 0.5 * (t * 7.0 + phase).sin()) } else { 0.5 };
        let x = r.left() + k as f32 * r.width() * 0.38;
        let bar = Rect::from_min_max(pos2(x, r.bottom() - r.height() * h), pos2(x + r.width() * 0.24, r.bottom()));
        p.rect_filled(bar, CornerRadius::same(1), c().accent);
    }
    if animated {
        p.ctx().request_repaint_after(Duration::from_millis(80));
    }
}

struct Columns {
    index: f32,
    art: f32,
    title: f32,
    album: Option<f32>,
    time: f32,
}

fn columns(left: f32, width: f32) -> Columns {
    let title = left + 104.0;
    Columns {
        index: left + 8.0,
        art: left + 48.0,
        title,
        album: (width > 640.0).then_some(left + width * 0.58),
        time: left + width - 16.0,
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        static DEBUG_REPAINT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *DEBUG_REPAINT.get_or_init(|| std::env::var_os("DUST_DEBUG_REPAINT").is_some()) {
            eprintln!("frame {}: {:?}", ctx.cumulative_pass_nr(), ctx.repaint_causes());
        }
        // Follow the chosen mode (and the OS setting live when on System).
        let dark = self.theme_mode.resolve(&ctx);
        if dark != theme::is_dark() {
            theme::apply(&ctx, dark);
        }
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
        self.persist_playback_settings(&st);
        egui::Panel::bottom("player")
            .exact_size(92.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(c().bar).inner_margin(Margin::symmetric(20, 0)))
            .show(ui, |ui| self.player_bar(ui, &st));
        egui::Panel::left("nav")
            .exact_size(248.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(c().sidebar).inner_margin(Margin { left: 12, right: 12, top: 0, bottom: 10 }))
            .show(ui, |ui| self.sidebar(ui));
        if self.show_queue {
            egui::Panel::right("queue")
                .exact_size(340.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(egui::Frame::new().fill(c().sidebar).inner_margin(Margin { left: 14, right: 14, top: 0, bottom: 10 }))
                .show(ui, |ui| self.queue_panel(ui, &st));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(c().bg).inner_margin(Margin { left: 36, right: 36, top: 0, bottom: 0 }))
            .show(ui, |ui| self.main_area(ui, &st));
    }
}
