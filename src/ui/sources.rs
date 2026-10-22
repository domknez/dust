//! Music sources: Deezer and a Navidrome / Subsonic server. Which one is active,
//! connecting a server, switching between them (each keeps its own queue) and
//! logging out of one.

use super::app::App;
use super::likes::{self, LikeState};
use super::state::View;
use super::style::{colors, typography as ty};
use super::tasks::{self, Task};
use super::widgets;
use crate::credentials;
use crate::deezer::Deezer;
use crate::player::{Cmd, saved};
use crate::provider::Provider;
use crate::settings;
use crate::subsonic::{Server, Subsonic};
use eframe::egui::{self, Key, RichText};

/// Settings values of the sources ([`Provider::key`] is empty for Deezer).
const DEEZER: &str = "deezer";
const NAVIDROME: &str = "navidrome";

/// The "Connect to Navidrome" form.
#[derive(Default)]
pub struct ServerForm {
    pub url: String,
    pub user: String,
    pub password: String,
    pub error: Option<String>,
}

/// Both sources' logins and which one the listener used last.
pub struct Sources {
    pub deezer: Option<Deezer>,
    pub server: Option<Subsonic>,
    /// Connecting to a server: the attempt, and the login to save if it works (one
    /// typed into the form; a stored one is already saved).
    pub(super) connecting: Task<Subsonic>,
    pub(super) connecting_new: Option<Server>,
    pub form: Option<ServerForm>,
    /// The source in use last time ("deezer" or "navidrome").
    preferred: String,
    /// The preferred source (or one the listener picked) is in use, so a login
    /// finishing later doesn't switch away from it. Until then another source may
    /// stand in, without becoming the preferred one.
    settled: bool,
}

impl Sources {
    pub fn new() -> Self {
        let preferred = settings::load().get("source").cloned().unwrap_or_else(|| DEEZER.into());
        Sources { deezer: None, server: None, connecting: None, connecting_new: None, form: None, preferred, settled: false }
    }

    pub fn connecting(&self) -> bool {
        self.connecting.is_some()
    }
}

fn setting(provider: &Provider) -> &'static str {
    match provider {
        Provider::Deezer(_) => DEEZER,
        Provider::Subsonic(_) => NAVIDROME,
    }
}

impl App {
    /// At start: connect the stored server too (Deezer's session is App::new's).
    pub(super) fn start_sources(&mut self, ctx: &egui::Context) {
        if let Some(server) = credentials::load_server() {
            self.sources.connecting = tasks::spawn(ctx, move || Subsonic::connect(&server));
        }
    }

    /// Deezer logged in.
    pub(super) fn deezer_ready(&mut self, ctx: &egui::Context, client: Deezer) {
        self.sources.deezer = Some(client.clone());
        self.offer(ctx, Provider::Deezer(client));
    }

    /// Use a source that just became available if nothing is in use yet, or if it's
    /// the one used last time and that hasn't been settled.
    fn offer(&mut self, ctx: &egui::Context, provider: Provider) {
        let preferred = setting(&provider) == self.sources.preferred;
        let same = self.client.as_ref().is_some_and(|c| c.key() == provider.key());
        if self.client.is_none() || same || (preferred && !self.sources.settled) {
            self.activate(ctx, provider, preferred);
        }
    }

    /// Server connections finishing.
    pub(super) fn poll_sources(&mut self, ctx: &egui::Context) {
        let Some(result) = tasks::poll(&mut self.sources.connecting) else { return };
        let typed = self.sources.connecting_new.take();
        match result {
            Ok(server) => {
                if let Some(login) = &typed {
                    credentials::save_server(login);
                    self.sources.form = None;
                }
                self.sources.server = Some(server.clone());
                if typed.is_some() {
                    // Just added: switch to it.
                    self.switch_source(ctx, Provider::Subsonic(server));
                } else {
                    self.offer(ctx, Provider::Subsonic(server));
                }
            }
            Err(e) => {
                log_warn!("music server: {e}");
                match self.sources.form.as_mut() {
                    Some(form) => form.error = Some(e.to_string()),
                    None if self.client.is_none() && self.login.is_none() => {
                        self.login_error = Some(format!("Couldn't reach your Navidrome server: {e}"));
                    }
                    None => {}
                }
            }
        }
    }

    /// Make `provider` the source in use; `chosen`: the listener's choice (or the
    /// one used last time), remembered for next time. Another source than before
    /// stops playback; its queue is saved and the new source's comes back.
    pub(super) fn activate(&mut self, ctx: &egui::Context, provider: Provider, chosen: bool) {
        let old = self.client.as_ref().map(Provider::key);
        let new = provider.key();
        if chosen {
            self.sources.settled = true;
            self.sources.preferred = setting(&provider).into();
            settings::set("source", setting(&provider));
        }
        if let Provider::Subsonic(server) = &provider {
            server.activate();
        }
        self.player.send(Cmd::Client(provider.clone()));
        self.player.send(Cmd::Quality(self.quality));
        self.player.send(Cmd::ReportListens(self.report_listens));
        if old != Some(new) {
            if let Some(old) = old {
                self.save_queue_now(old);
                log_info!("switching to {}", provider.service());
            }
            self.clear_content();
            self.restore_queue(&provider);
        }
        let c = provider.clone();
        self.playlists_task = tasks::spawn(ctx, move || c.playlists());
        let c = provider.clone();
        self.likes_task = tasks::spawn(ctx, move || likes::load(&c));
        self.client = Some(provider);
        self.open_view(ctx, View::Home);
    }

    /// Bring back the source's queue from last time, paused where it was.
    fn restore_queue(&mut self, provider: &Provider) {
        self.restore_pending = false;
        if let Some(s) = saved::load(provider.key()) {
            log_info!("restoring a queue of {} tracks", s.tracks.len());
            provider.adopt(&s.tracks);
            self.player.send(Cmd::Restore { tracks: s.tracks, index: s.index, position: s.position, flow: s.flow });
            self.restore_pending = true;
        }
        self.queue_saved = Some((0, usize::MAX, None));
    }

    /// Switch from the account menu.
    pub(super) fn switch_source(&mut self, ctx: &egui::Context, provider: Provider) {
        self.activate(ctx, provider, true);
    }

    /// Log out of the source in use and forget its login; the other one takes over
    /// if it's logged in, otherwise the login screen shows.
    pub(super) fn log_out(&mut self, ctx: &egui::Context) {
        match self.client.as_ref() {
            Some(Provider::Deezer(_)) => {
                credentials::clear();
                self.sources.deezer = None;
            }
            Some(Provider::Subsonic(_)) => {
                credentials::clear_server();
                self.sources.server = None;
            }
            None => {}
        }
        let other = self.sources.deezer.clone().map(Provider::Deezer).or_else(|| self.sources.server.clone().map(Provider::Subsonic));
        match other {
            Some(provider) => self.switch_source(ctx, provider),
            None => {
                if let Some(old) = self.client.as_ref().map(Provider::key) {
                    self.save_queue_now(old);
                }
                self.client = None;
                self.clear_content();
            }
        }
    }

    /// Forget what the previous source showed.
    fn clear_content(&mut self) {
        self.tracks.clear();
        self.album_info = None;
        self.search_sections.clear();
        self.playlists.clear();
        self.home.clear();
        self.followed.clear();
        self.favorite_albums.clear();
        self.favorite_shows.clear();
        self.artist_page = None;
        self.likes = LikeState::default();
        self.history = Default::default();
        self.lyrics = Default::default();
        self.tracks_task = None;
        self.search_task = None;
        self.link_task = None;
        self.home_task = None;
        self.artist_task = None;
        self.followed_task = None;
        self.favorite_albums_task = None;
        self.favorite_shows_task = None;
        self.quick_play = None;
        self.list_error = None;
    }

    /// What the source in use offers (everything while none is).
    pub(super) fn features(&self) -> crate::provider::Features {
        self.client.as_ref().map_or(Provider::DEEZER_FEATURES, Provider::features)
    }

    /// The server's address in use, for a Navidrome source.
    pub(super) fn source_host(&self) -> String {
        match &self.client {
            Some(Provider::Subsonic(s)) => s.host().to_string(),
            _ => String::new(),
        }
    }

    pub(super) fn open_server_form(&mut self) {
        self.sources.form = Some(ServerForm { url: "http://".into(), ..Default::default() });
    }

    /// The "Connect to Navidrome" dialog, while open (call once per frame).
    pub(super) fn server_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.sources.form.take() else { return };
        let p = colors();
        let busy = self.sources.connecting();
        let mut connect = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("server-dialog")).frame(widgets::popover_frame(ctx)).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(RichText::new("Connect to Navidrome").font(ty::SECTION_TITLE.font()).color(p.text));
            ui.label(RichText::new("Or any Subsonic-compatible server.").color(p.dim));
            let field = |ui: &mut egui::Ui, value: &mut String, hint: &str, password: bool| {
                ui.add_enabled(!busy, egui::TextEdit::singleline(value).hint_text(hint).password(password).desired_width(f32::INFINITY))
            };
            let url = field(ui, &mut form.url, "Server address, e.g. http://192.168.1.10:4533", false);
            if ui.memory(|m| m.focused().is_none()) {
                url.request_focus();
            }
            field(ui, &mut form.user, "User name", false);
            field(ui, &mut form.password, "Password", true);
            if let Some(e) = &form.error {
                ui.label(RichText::new(e).color(p.danger));
            }
            ui.horizontal(|ui| {
                let ready = !busy && !form.url.trim().is_empty() && !form.user.trim().is_empty();
                connect = ui.add_enabled(ready, egui::Button::new(RichText::new("Connect").strong())).clicked()
                    || (ready && ui.input(|i| i.key_pressed(Key::Enter)));
                close |= ui.add_enabled(!busy, egui::Button::new("Cancel")).clicked();
                if busy {
                    ui.spinner();
                }
            });
            ui.label(RichText::new("The password is kept in your system's keychain.").color(p.faint).font(ty::SMALL.font()));
        });
        close |= modal.should_close() && !busy;
        if connect {
            form.error = None;
            let server = Server { url: form.url.trim().into(), user: form.user.trim().into(), password: form.password.clone() };
            self.sources.connecting_new = Some(server.clone());
            self.sources.connecting = tasks::spawn(ctx, move || Subsonic::connect(&server));
        }
        if !close {
            self.sources.form = Some(form);
        }
    }
}
