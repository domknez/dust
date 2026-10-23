//! Music sources: Deezer and a Navidrome / Subsonic server. Which one is active,
//! connecting a server, switching between them (each keeps its own queue) and
//! logging out of one.

use super::action::{Action, Actions};
use super::app::App;
use super::browse::Browse;
use super::library::Library;
use super::likes;
use super::load::Load;
use super::state::View;
use super::style::icons::Icon;
use super::style::{colors, radius, typography as ty};
use super::widgets;
use crate::credentials;
use crate::deezer::Deezer;
use crate::player::Cmd;
use crate::provider::Provider;
use crate::settings::SourcePref;
use crate::subsonic::{Server, Subsonic};
use eframe::egui::{self, Key, RichText, vec2};

/// The "Connect to Navidrome" form.
#[derive(Default)]
pub struct ServerForm {
    pub url: String,
    pub user: String,
    pub password: String,
    pub error: Option<String>,
    pub show_password: bool,
    /// The address field got the cursor when the form opened.
    focused: bool,
}

/// The login screen: the Deezer login on its way, the ARL field, and why the last
/// attempt (or the session) ended.
#[derive(Default)]
pub struct Login {
    pub task: Load<Deezer>,
    /// The ARL typed in, and whether its field is open.
    pub arl: String,
    pub show_arl: bool,
    /// Store the session in the keychain once the pending login succeeds.
    pub remember: bool,
    pub error: Option<String>,
}

/// Both sources' logins.
pub struct Sources {
    pub deezer: Option<Deezer>,
    pub server: Option<Subsonic>,
    /// Connecting to a server: the attempt, and the login to save if it works (one
    /// typed into the form; a stored one is already saved).
    pub(super) connecting: Load<Subsonic>,
    pub(super) connecting_new: Option<Server>,
    pub form: Option<ServerForm>,
    /// The preferred source ([`crate::settings::Prefs::source`], or one the
    /// listener picked) is in use, so a login finishing later doesn't switch away
    /// from it. Until then another source may stand in, without becoming the
    /// preferred one.
    settled: bool,
}

impl Sources {
    pub fn new() -> Self {
        Sources { deezer: None, server: None, connecting: Load::Idle, connecting_new: None, form: None, settled: false }
    }

    pub fn connecting(&self) -> bool {
        self.connecting.is_loading()
    }
}

fn setting(provider: &Provider) -> SourcePref {
    match provider {
        Provider::Deezer(_) => SourcePref::Deezer,
        Provider::Subsonic(_) => SourcePref::Navidrome,
    }
}

impl App {
    /// At start: connect the stored server too (Deezer's session is App::new's).
    pub(super) fn start_sources(&mut self) {
        if let Some(server) = credentials::load_server() {
            self.sources.connecting.start(&self.jobs, move || Subsonic::connect(&server));
        }
    }

    /// Deezer logged in.
    pub(super) fn deezer_ready(&mut self, client: Deezer) {
        self.sources.deezer = Some(client.clone());
        self.offer(Provider::Deezer(client));
    }

    /// Use a source that just became available if nothing is in use yet, or if it's
    /// the one used last time and that hasn't been settled.
    fn offer(&mut self, provider: Provider) {
        let preferred = setting(&provider) == self.prefs.source;
        let same = self.client.as_ref().is_some_and(|c| c.key() == provider.key());
        if self.client.is_none() || same || (preferred && !self.sources.settled) {
            self.activate(provider, preferred);
        }
    }

    /// Server connections finishing.
    pub(super) fn poll_sources(&mut self) {
        let Some(result) = self.sources.connecting.poll_take() else { return };
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
                    self.switch_source(Provider::Subsonic(server));
                } else {
                    self.offer(Provider::Subsonic(server));
                }
            }
            Err(e) => {
                log_warn!("music server: {e}");
                match self.sources.form.as_mut() {
                    Some(form) => form.error = Some(e.to_string()),
                    None if self.client.is_none() && !self.login.task.is_loading() => {
                        self.login.error = Some(format!("Couldn't reach your Navidrome server: {e}"));
                    }
                    None => {}
                }
            }
        }
    }

    /// Make `provider` the source in use; `chosen`: the listener's choice (or the
    /// one used last time), remembered for next time. Another source than before
    /// stops playback; its queue is saved and the new source's comes back.
    pub(super) fn activate(&mut self, provider: Provider, chosen: bool) {
        let old = self.client.as_ref().map(Provider::key);
        let new = provider.key();
        if chosen {
            self.sources.settled = true;
            self.prefs.update(|p| p.source = setting(&provider));
        }
        self.covers.set_resolver(std::sync::Arc::new(provider.clone()));
        self.player.send(Cmd::Client(provider.clone()));
        self.player.send(Cmd::Quality(self.prefs.quality));
        self.player.send(Cmd::ReportListens(self.prefs.report_listens));
        if old != Some(new) {
            if let Some(old) = old {
                self.save_queue_now(old);
                log_info!("switching to {}", provider.service());
            }
            self.clear_content();
            self.restore_queue(&provider);
        }
        let c = provider.clone();
        self.library.playlists_load.start(&self.jobs, move || c.playlists());
        let c = provider.clone();
        self.library.likes_load.start(&self.jobs, move || likes::load(&c));
        self.client = Some(provider);
        self.open_view(View::Home);
    }

    /// Bring back the source's queue from last time, paused where it was.
    fn restore_queue(&mut self, provider: &Provider) {
        self.restore_pending = false;
        if let Some(s) = self.queues.load(provider.key()) {
            log_info!("restoring a queue of {} tracks", s.tracks.len());
            self.player.send(Cmd::Restore { tracks: s.tracks, index: s.index, position: s.position, flow: s.flow });
            self.restore_pending = true;
        }
        self.queue_saved = Some((0, usize::MAX, None));
    }

    /// Switch from the account menu.
    pub(super) fn switch_source(&mut self, provider: Provider) {
        self.activate(provider, true);
    }

    /// Log out of the source in use and forget its login; the other one takes over
    /// if it's logged in, otherwise the login screen shows.
    pub(super) fn log_out(&mut self) {
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
            Some(provider) => self.switch_source(provider),
            None => {
                if let Some(old) = self.client.as_ref().map(Provider::key) {
                    self.save_queue_now(old);
                }
                self.client = None;
                self.clear_content();
            }
        }
    }

    /// Forget what the previous source had and showed. Its loads in flight go too,
    /// so none of their results can land on the next source (or the login screen).
    fn clear_content(&mut self) {
        self.library = Library::default();
        self.browse = Browse::default();
        self.lyrics = Default::default();
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
        self.sources.form = Some(ServerForm::default());
    }

    /// Connect to a server typed into the form; it's saved and switched to once
    /// that works.
    pub(super) fn connect_server(&mut self, server: Server) {
        self.sources.connecting_new = Some(server.clone());
        self.sources.connecting.start(&self.jobs, move || Subsonic::connect(&server));
    }

    /// The "Connect to Navidrome" dialog, while open (call once per frame).
    pub(super) fn server_dialog(&mut self, ctx: &egui::Context, acts: &mut Actions) {
        let Some(mut form) = self.sources.form.take() else { return };
        let p = colors();
        let busy = self.sources.connecting();
        let ready = !busy && !form.url.trim().is_empty() && !form.user.trim().is_empty();
        let mut connect = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("server-dialog")).frame(widgets::popover_frame(ctx)).show(ctx, |ui| {
            ui.set_width(DIALOG_WIDTH);
            egui::Frame::new().inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("MUSIC SOURCE").font(ty::OVERLINE.font()).color(p.accent));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let hint = "Close (Esc)";
                        close |= ui
                            .add_enabled_ui(!busy, |ui| widgets::icon_button(ui, Icon::Close, 14.0, p.dim).on_hover_text(hint))
                            .inner
                            .clicked();
                    });
                });
                ui.add_space(12.0);
                ui.label(RichText::new("Connect to Navidrome").font(ty::SECTION_TITLE.font()).color(p.text));
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Play your own library from Navidrome or any Subsonic-compatible server.")
                        .font(ty::SECONDARY.font())
                        .color(p.dim),
                );
                ui.add_space(20.0);

                let mut changed = false;
                ui.add_enabled_ui(!busy, |ui| {
                    field_label(ui, "SERVER ADDRESS", None);
                    let (url, _) = widgets::text_field(ui, &mut form.url, "192.168.1.10:4533 or https://music.example.com", false, None);
                    if !form.focused {
                        url.request_focus();
                        form.focused = true;
                    }
                    changed |= url.changed();
                    ui.add_space(14.0);
                    field_label(ui, "USER NAME", None);
                    let (user, _) = widgets::text_field(ui, &mut form.user, "Your Navidrome user", false, None);
                    changed |= user.changed();
                    ui.add_space(14.0);
                    field_label(ui, "PASSWORD", Some("kept in your keychain"));
                    let action = if form.show_password { "Hide" } else { "Show" };
                    let (password, toggle) = widgets::text_field(ui, &mut form.password, "Password", !form.show_password, Some(action));
                    changed |= password.changed();
                    if toggle {
                        form.show_password = !form.show_password;
                    }
                });
                // Typing again clears the last attempt's error.
                if changed {
                    form.error = None;
                }
                if let Some(e) = &form.error {
                    ui.add_space(14.0);
                    error_box(ui, e);
                }
                ui.add_space(22.0);
                // Buttons on their own row, right-aligned.
                ui.allocate_ui_with_layout(vec2(ui.available_width(), 40.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let label = if busy { "Connecting…" } else { "Connect" };
                    connect = ui.add_enabled_ui(ready, |ui| widgets::pill(ui, label, None, true)).inner.clicked()
                        || (ready && ui.input(|i| i.key_pressed(Key::Enter)));
                    if busy {
                        ui.add(egui::Spinner::new().size(16.0).color(p.dim));
                    } else {
                        close |= widgets::pill(ui, "Cancel", None, false).clicked();
                    }
                });
            });
        });
        close |= modal.should_close() && !busy;
        if connect {
            form.error = None;
            let server = Server { url: form.url.trim().into(), user: form.user.trim().into(), password: form.password.clone() };
            acts.push(Action::ConnectServer(server));
        }
        if !close {
            self.sources.form = Some(form);
        }
    }
}

const DIALOG_WIDTH: f32 = 400.0;

/// Small mono label above a form field, with an optional note at the right end.
fn field_label(ui: &mut egui::Ui, text: &str, note: Option<&str>) {
    let p = colors();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.text(rect.left_center(), egui::Align2::LEFT_CENTER, text, ty::OVERLINE.font(), p.faint);
    if let Some(note) = note {
        painter.text(rect.right_center(), egui::Align2::RIGHT_CENTER, note, ty::OVERLINE.font(), p.faint);
    }
    ui.add_space(6.0);
}

/// Why connecting failed, in a box tinted with the danger colour.
fn error_box(ui: &mut egui::Ui, message: &str) {
    let p = colors();
    egui::Frame::new()
        .fill(p.danger.gamma_multiply(0.12))
        .stroke(egui::Stroke::new(1.0, p.danger.gamma_multiply(0.5)))
        .corner_radius(egui::CornerRadius::same(radius::CONTROL))
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(message).font(ty::SECONDARY.font()).color(p.danger));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::Status;
    use crate::ui::app::testing::{TestApp, deezer, ids, navidrome, queue};
    use std::sync::mpsc::channel;

    /// The stored server's connection finishes.
    fn server_connects(t: &mut TestApp) {
        let (tx, rx) = channel();
        t.app.sources.connecting = Load::Loading(rx);
        tx.send(Ok(navidrome())).unwrap();
        t.app.poll_sources();
    }

    fn in_use(t: &TestApp) -> Option<&'static str> {
        t.app.client.as_ref().map(Provider::service)
    }

    /// The tracks of the saved queue of source `key`.
    fn saved(t: &TestApp, key: &str) -> Option<Vec<u64>> {
        t.queues().load(key).map(|s| ids(&s.tracks))
    }

    /// The tracks of the queue restore sent to the player, if one was.
    fn restored(t: &TestApp) -> Option<Vec<u64>> {
        t.sent().into_iter().find_map(|cmd| match cmd {
            Cmd::Restore { tracks, .. } => Some(ids(&tracks)),
            _ => None,
        })
    }

    #[test]
    fn the_source_used_last_time_takes_over_when_it_connects_later() {
        let mut t = TestApp::new("source=navidrome\n");
        t.app.deezer_ready(deezer());
        assert_eq!(in_use(&t), Some("Deezer"), "stands in while the server connects");
        assert_eq!(t.app.prefs.source, SourcePref::Navidrome, "without becoming the one used next time");
        server_connects(&mut t);
        assert_eq!(in_use(&t), Some("Navidrome"));
        // Deezer logging in again later doesn't switch away.
        t.app.deezer_ready(deezer());
        assert_eq!(in_use(&t), Some("Navidrome"));
    }

    #[test]
    fn a_stand_in_gives_way_once() {
        let mut t = TestApp::new("source=deezer\n");
        server_connects(&mut t);
        assert_eq!(in_use(&t), Some("Navidrome"));
        t.app.deezer_ready(deezer());
        assert_eq!(in_use(&t), Some("Deezer"));
        server_connects(&mut t);
        assert_eq!(in_use(&t), Some("Deezer"), "the server reconnecting doesn't take over");
    }

    #[test]
    fn a_source_picked_by_hand_sticks() {
        let mut t = TestApp::new("source=deezer\n");
        t.app.deezer_ready(deezer());
        server_connects(&mut t);
        assert_eq!(in_use(&t), Some("Deezer"));
        t.update(Action::SwitchSource(Provider::Subsonic(navidrome())));
        assert_eq!((in_use(&t), t.app.prefs.source), (Some("Navidrome"), SourcePref::Navidrome));
        t.app.deezer_ready(deezer());
        assert_eq!(in_use(&t), Some("Navidrome"), "Deezer logging in again doesn't undo the pick");
    }

    #[test]
    fn covers_come_from_the_source_in_use() {
        let mut t = TestApp::new("source=deezer\n");
        let art = crate::model::Art::new("al-5F81DCwcfjVByziJAH9maf_0").unwrap();
        assert_eq!(t.app.covers.url(&art, 120), None, "no source yet");
        t.app.deezer_ready(deezer());
        assert!(t.app.covers.url(&art, 120).unwrap().starts_with("https://cdn-images.dzcdn.net/images/al-5F81DCwcfjVByziJAH9maf_0/"));
        t.update(Action::SwitchSource(Provider::Subsonic(navidrome())));
        assert!(t.app.covers.url(&art, 120).unwrap().starts_with("http://nas:4533/rest/getCoverArt?"));
    }

    #[test]
    fn nothing_is_saved_before_the_queue_was_offered_back() {
        let mut t = TestApp::new("");
        t.queues().save(&queue(&[1, 2], 1), "");
        // The player's queue before any login: empty, then whatever it is.
        t.app.save_queue_if_changed(&Status::default());
        t.app.save_queue_if_changed(&queue(&[9], 0));
        assert_eq!(saved(&t, ""), Some(vec![1, 2]));
    }

    #[test]
    fn the_saved_queue_comes_back_and_is_kept_up_to_date() {
        let mut t = TestApp::new("");
        t.queues().save(&queue(&[1, 2], 1), "");
        t.app.deezer_ready(deezer());
        assert_eq!(restored(&t), Some(vec![1, 2]));
        // Until the player shows it, its empty queue isn't saved over it.
        t.app.save_queue_if_changed(&Status::default());
        assert_eq!(saved(&t, ""), Some(vec![1, 2]));
        t.app.save_queue_if_changed(&queue(&[1, 2], 1));
        t.app.save_queue_if_changed(&queue(&[1, 2, 3], 2));
        assert_eq!(saved(&t, ""), Some(vec![1, 2, 3]));
        assert_eq!(t.queues().load("").map(|s| s.index), Some(2));
    }

    #[test]
    fn each_source_keeps_its_own_queue() {
        let mut t = TestApp::new("");
        t.queues().save(&queue(&[7], 0), "navidrome");
        t.app.deezer_ready(deezer());
        t.app.player.set_status(|st| *st = queue(&[1, 2], 0));
        t.sent();
        t.update(Action::SwitchSource(Provider::Subsonic(navidrome())));
        assert_eq!(saved(&t, ""), Some(vec![1, 2]), "Deezer's queue is saved on the way out");
        assert_eq!(restored(&t), Some(vec![7]), "and the server's comes back");
    }

    #[test]
    fn switching_before_a_restore_arrives_keeps_the_saved_queue() {
        let mut t = TestApp::new("");
        t.queues().save(&queue(&[1, 2], 1), "");
        t.app.deezer_ready(deezer());
        // The player hasn't shown the restored queue yet: it's still empty.
        t.update(Action::SwitchSource(Provider::Subsonic(navidrome())));
        assert_eq!(saved(&t, ""), Some(vec![1, 2]));
    }
}
