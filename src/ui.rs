//! egui front-end. Reactive: nothing is redrawn unless there is input, a background
//! task finishes, or a track is playing (twice a second for the progress bar).

use crate::deezer::{Deezer, Playlist, Quality, Track};
use crate::output::airplay::Discovery;
use crate::player::{Cmd, Output, PlayerHandle, State};
use eframe::egui::{self, Align2, Color32, FontId, Key, RichText, Sense, vec2};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

const ACCENT: Color32 = Color32::from_rgb(0xa2, 0x38, 0xff);
const KEYRING_SERVICE: &str = "dust";
const KEYRING_USER: &str = "arl";

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

#[derive(Clone, PartialEq)]
enum View {
    Search,
    Flow,
    Loved,
    Playlist(u64, String),
}

pub struct App {
    player: PlayerHandle,
    client: Option<Deezer>,
    arl: String,
    login: Task<Deezer>,
    login_error: Option<String>,
    view: View,
    search: String,
    tracks: Vec<Track>,
    tracks_task: Task<Vec<Track>>,
    list_error: Option<String>,
    playlists: Vec<Playlist>,
    playlists_task: Task<Vec<Playlist>>,
    seek_drag: Option<f64>,
    volume: f32,
    quality: Quality,
    discovery: Option<Discovery>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        let mut visuals = egui::Visuals::dark();
        visuals.selection.bg_fill = ACCENT;
        visuals.hyperlink_color = ACCENT;
        ctx.set_visuals(visuals);

        let volume = 0.8;
        let repaint = ctx.clone();
        let discovery = Discovery::start(move || repaint.request_repaint())
            .inspect_err(|e| eprintln!("dust: AirPlay discovery unavailable: {e}"))
            .ok();
        let mut app = Self {
            player: PlayerHandle::spawn(ctx.clone(), volume),
            client: None,
            arl: String::new(),
            login: None,
            login_error: None,
            view: View::Search,
            search: String::new(),
            tracks: Vec::new(),
            tracks_task: None,
            list_error: None,
            playlists: Vec::new(),
            playlists_task: None,
            seek_drag: None,
            volume,
            quality: Quality::Mp3_320,
            discovery,
        };
        if let Some(arl) = keyring().and_then(|k| k.get_password().ok()) {
            app.start_login(ctx, arl);
        }
        app
    }

    fn start_login(&mut self, ctx: &egui::Context, arl: String) {
        self.login_error = None;
        self.login = spawn(ctx, move || Deezer::login(&arl));
    }

    fn open_view(&mut self, ctx: &egui::Context, view: View) {
        let Some(client) = self.client.clone() else { return };
        self.view = view.clone();
        self.list_error = None;
        self.tracks.clear();
        self.tracks_task = match view {
            View::Search if self.search.trim().is_empty() => None,
            View::Search => {
                let q = self.search.clone();
                spawn(ctx, move || client.search(&q))
            }
            View::Flow => spawn(ctx, move || client.flow()),
            View::Loved => spawn(ctx, move || client.loved()),
            View::Playlist(id, _) => spawn(ctx, move || client.playlist(id)),
        };
    }

    fn poll_tasks(&mut self, ctx: &egui::Context) {
        match poll(&mut self.login) {
            Some(Ok(client)) => {
                if !self.arl.is_empty() {
                    if let Some(k) = keyring() {
                        let _ = k.set_password(self.arl.trim());
                    }
                    self.arl.clear();
                }
                self.player.send(Cmd::Client(client.clone()));
                self.player.send(Cmd::Quality(self.quality));
                let c = client.clone();
                self.playlists_task = spawn(ctx, move || c.playlists());
                self.client = Some(client);
                self.open_view(ctx, View::Flow);
            }
            Some(Err(e)) => self.login_error = Some(e),
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
    }

    fn login_screen(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.25);
                ui.label(RichText::new("dust").size(42.0).strong().color(ACCENT));
                ui.label("lightweight Deezer player");
                ui.add_space(24.0);
                if self.login.is_some() {
                    ui.spinner();
                    return;
                }
                ui.label("Paste your Deezer ARL cookie (Premium account):");
                let edit = ui.add(egui::TextEdit::singleline(&mut self.arl).password(true).desired_width(360.0));
                let go = ui.button("Log in").clicked() || (edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                if go && !self.arl.trim().is_empty() {
                    let arl = self.arl.trim().to_string();
                    self.start_login(ui.ctx(), arl);
                }
                if let Some(e) = &self.login_error {
                    ui.add_space(8.0);
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
                ui.add_space(16.0);
                ui.small("deezer.com → DevTools → Application → Cookies → arl");
            });
        });
    }

    fn nav(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("dust").size(24.0).strong().color(ACCENT));
        if let Some(c) = &self.client {
            ui.small(c.name());
        }
        ui.add_space(8.0);
        let ctx = ui.ctx().clone();
        for (label, view) in [("🔍 Search", View::Search), ("🌊 Flow", View::Flow), ("♥ Loved tracks", View::Loved)] {
            if ui.selectable_label(self.view == view, label).clicked() {
                self.open_view(&ctx, view);
            }
        }
        ui.separator();
        ui.label(RichText::new("Playlists").small().weak());
        let bottom = 70.0;
        egui::ScrollArea::vertical().max_height(ui.available_height() - bottom).show(ui, |ui| {
            if self.playlists_task.is_some() {
                ui.spinner();
            }
            let mut open = None;
            for p in &self.playlists {
                let selected = self.view == View::Playlist(p.id, p.title.clone());
                if ui.selectable_label(selected, &p.title).on_hover_text(format!("{} tracks", p.count)).clicked() {
                    open = Some(View::Playlist(p.id, p.title.clone()));
                }
            }
            if let Some(v) = open {
                self.open_view(&ctx, v);
            }
        });
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(6.0);
            if ui.small_button("Log out").clicked() {
                if let Some(k) = keyring() {
                    let _ = k.delete_credential();
                }
                self.client = None;
                self.tracks.clear();
                self.playlists.clear();
            }
            egui::ComboBox::from_id_salt("quality").selected_text(self.quality.label()).show_ui(ui, |ui| {
                for q in Quality::ALL {
                    if ui.selectable_value(&mut self.quality, q, q.label()).clicked() {
                        self.player.send(Cmd::Quality(q));
                    }
                }
            });
        });
    }

    fn player_bar(&mut self, ui: &mut egui::Ui) {
        let st = self.player.status();
        let duration = st.track.as_ref().map_or(0.0, |t| t.duration as f64);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("⏮").clicked() {
                self.player.send(Cmd::Prev);
            }
            let icon = if st.state == State::Playing { "⏸" } else { "▶" };
            if ui.add(egui::Button::new(RichText::new(icon).size(18.0))).clicked() {
                self.player.send(Cmd::Toggle);
            }
            if ui.button("⏭").clicked() {
                self.player.send(Cmd::Next);
            }
            ui.add_space(8.0);
            match (&st.track, st.state) {
                (_, State::Loading) => {
                    ui.spinner();
                }
                (Some(t), _) => {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&t.title).strong());
                        ui.label(RichText::new(&t.artist).weak());
                    });
                }
                _ => {}
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Slider::new(&mut self.volume, 0.0..=1.0).show_value(false)).changed() {
                    self.player.send(Cmd::Volume(self.volume));
                }
                ui.label("🔊");
                self.output_picker(ui, &st.output);
            });
        });
        ui.horizontal(|ui| {
            let mut pos = self.seek_drag.unwrap_or(st.position).min(duration);
            ui.label(mmss(pos));
            ui.spacing_mut().slider_width = (ui.available_width() - 50.0).max(50.0);
            let resp = ui.add_enabled(duration > 0.0, egui::Slider::new(&mut pos, 0.0..=duration.max(1.0)).show_value(false));
            if resp.dragged() {
                self.seek_drag = Some(pos);
            }
            if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                self.player.send(Cmd::Seek(pos));
                self.seek_drag = None;
            }
            ui.label(mmss(duration));
        });
        if let Some(e) = &st.error {
            ui.colored_label(Color32::LIGHT_RED, e);
        }
        ui.add_space(4.0);
        if st.state == State::Playing {
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        }
    }

    fn output_picker(&mut self, ui: &mut egui::Ui, current: &str) {
        egui::ComboBox::from_id_salt("output").selected_text(format!("🖧 {current}")).show_ui(ui, |ui| {
            if ui.selectable_label(current == Output::Local.name(), Output::Local.name()).clicked() {
                self.player.send(Cmd::Output(Output::Local));
            }
            let devices = self.discovery.as_ref().map(|d| d.devices()).unwrap_or_default();
            if devices.is_empty() {
                ui.label(RichText::new("Searching for AirPlay…").weak());
            }
            for d in devices {
                let label = if d.supported { format!("AirPlay: {}", d.name) } else { format!("AirPlay: {} (unsupported)", d.name) };
                let r = ui.add_enabled(d.supported && !d.password, egui::Button::selectable(current == d.name, label));
                if r.clicked() {
                    self.player.send(Cmd::Output(Output::AirPlay(d)));
                }
            }
        });
    }

    fn track_list(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        match &self.view {
            View::Search => {
                ui.horizontal(|ui| {
                    let edit = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search tracks…").desired_width(f32::INFINITY));
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.open_view(&ctx, View::Search);
                    }
                });
            }
            View::Flow => {
                ui.horizontal(|ui| {
                    ui.heading("Flow");
                    if ui.small_button("↻").on_hover_text("More").clicked() {
                        self.open_view(&ctx, View::Flow);
                    }
                });
            }
            View::Loved => {
                ui.heading("Loved tracks");
            }
            View::Playlist(_, title) => {
                ui.heading(title.as_str());
            }
        }
        ui.add_space(4.0);
        if self.tracks_task.is_some() {
            ui.spinner();
            return;
        }
        if let Some(e) = &self.list_error {
            ui.colored_label(Color32::LIGHT_RED, e);
            return;
        }

        let playing_id = self.player.status().track.map(|t| t.id);
        let row_h = 22.0;
        let font = FontId::proportional(13.5);
        let text = ui.visuals().text_color();
        let weak = ui.visuals().weak_text_color();
        let hover_bg = ui.visuals().widgets.hovered.weak_bg_fill;
        let mut play = None;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, row_h, self.tracks.len(), |ui, range| {
            for i in range {
                let t = &self.tracks[i];
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                let p = ui.painter();
                if resp.hovered() {
                    p.rect_filled(rect, 3.0, hover_bg);
                }
                let current = playing_id == Some(t.id);
                let color = if current { ACCENT } else { text };
                let w = rect.width();
                let cols = [(0.0, 0.42, &t.title, color), (0.43, 0.27, &t.artist, weak), (0.71, 0.22, &t.album, weak)];
                for (x, width, s, c) in cols {
                    let col = egui::Rect::from_min_size(rect.min + vec2(8.0 + x * w, 0.0), vec2(width * w - 8.0, row_h));
                    p.with_clip_rect(col).text(col.left_center(), Align2::LEFT_CENTER, s, font.clone(), c);
                }
                p.text(rect.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, mmss(t.duration as f64), font.clone(), weak);
                if resp.double_clicked() {
                    play = Some(i);
                }
            }
        });
        if let Some(i) = play {
            self.player.send(Cmd::Play(self.tracks.clone(), i));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_tasks(&ctx);
        if self.client.is_none() {
            self.login_screen(ui);
            return;
        }
        if !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::Space)) {
            self.player.send(Cmd::Toggle);
        }
        egui::Panel::bottom("player").show(ui, |ui| self.player_bar(ui));
        egui::Panel::left("nav").exact_size(200.0).show(ui, |ui| self.nav(ui));
        egui::CentralPanel::default().show(ui, |ui| self.track_list(ui));
    }
}
