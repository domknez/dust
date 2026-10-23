//! Music sources in the UI: which are logged in and which one is in use, starting
//! the stored logins, switching between sources (each keeps its own queue) and
//! logging out of one. [`MENU`] lists how the account menu offers each kind of
//! source; what's particular to one (Deezer's login screen, the "Connect to
//! Navidrome" dialog here) is reached from its entry.

use super::action::{Action, Actions};
use super::app::App;
use super::browse::Browse;
use super::library::Library;
use super::load::Load;
use super::state::View;
use super::style::icons::Icon;
use super::style::{colors, radius, typography as ty};
use super::widgets;
use crate::credentials;
use crate::deezer::{self, Deezer};
use crate::player::Cmd;
use crate::source::{Features, KINDS, Shared, SourceId};
use crate::subsonic::{self, Server, Subsonic};
use eframe::egui::{self, Key, RichText, vec2};
use std::collections::HashMap;
use std::sync::Arc;

/// How the account menu shows a kind of source.
pub(super) struct SourceUi {
    pub id: SourceId,
    pub icon: Icon,
    /// While it isn't logged in: the row's note and what clicking it does; None
    /// leaves the row out (no way to log in from the menu in this build).
    pub connect: fn(&App) -> Option<(&'static str, Action)>,
}

/// The account menu's sources, in [`KINDS`] order. A new source adds its row here.
pub(super) const MENU: &[SourceUi] = &[
    SourceUi { id: deezer::ID, icon: Icon::Streaming, connect: connect_deezer },
    SourceUi { id: subsonic::ID, icon: Icon::Server, connect: connect_server },
];

/// Deezer's login window, where this build has it.
#[cfg(feature = "login-window")]
fn connect_deezer(_app: &App) -> Option<(&'static str, Action)> {
    Some(("Log in…", Action::LogInWithBrowser))
}

#[cfg(not(feature = "login-window"))]
fn connect_deezer(_app: &App) -> Option<(&'static str, Action)> {
    None
}

/// The "Connect to Navidrome" dialog.
fn connect_server(app: &App) -> Option<(&'static str, Action)> {
    Some((if app.sources.connecting() { "Connecting…" } else { "Connect a server…" }, Action::OpenServerForm))
}

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

/// The logged-in sources, and a server connection on its way.
pub struct Sources {
    logged_in: HashMap<SourceId, Shared>,
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
        Sources { logged_in: HashMap::new(), connecting: Load::Idle, connecting_new: None, form: None, settled: false }
    }

    pub fn connecting(&self) -> bool {
        self.connecting.is_loading()
    }

    /// The logged-in source of this kind.
    pub fn get(&self, id: SourceId) -> Option<&Shared> {
        self.logged_in.get(&id)
    }

    /// The first logged-in source, in [`KINDS`] order.
    fn first(&self) -> Option<Shared> {
        KINDS.iter().find_map(|id| self.get(*id).cloned())
    }
}

impl App {
    /// At start: log in with the stored logins.
    pub(super) fn start_sources(&mut self) {
        if let Some(arl) = credentials::load(deezer::ID) {
            self.login.task.start(&self.jobs, move || Deezer::login(&arl));
        }
        if let Some(server) = credentials::load(subsonic::ID).and_then(|text| Server::from_json(&text)) {
            self.sources.connecting.start(&self.jobs, move || Subsonic::connect(&server));
        }
    }

    /// A source logged in: use it if nothing is in use yet, or if it's the one used
    /// last time and that hasn't been settled.
    pub(super) fn source_ready(&mut self, source: Shared) {
        self.sources.logged_in.insert(source.id(), source.clone());
        let preferred = source.id() == self.prefs.source;
        let same = self.client.as_ref().is_some_and(|c| c.id() == source.id());
        if self.client.is_none() || same || (preferred && !self.sources.settled) {
            self.activate(source, preferred);
        }
    }

    /// Server connections finishing.
    pub(super) fn poll_sources(&mut self) {
        let Some(result) = self.sources.connecting.poll_take() else { return };
        let typed = self.sources.connecting_new.take();
        match result {
            Ok(server) => {
                let server: Shared = Arc::new(server);
                if let Some(login) = &typed {
                    credentials::save(subsonic::ID, &login.to_json());
                    self.sources.form = None;
                    // Just added: switch to it.
                    self.sources.logged_in.insert(subsonic::ID, server.clone());
                    self.switch_source(server);
                } else {
                    self.source_ready(server);
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

    /// Make `source` the one in use; `chosen`: the listener's choice (or the one
    /// used last time), remembered for next time. Another source than before stops
    /// playback; its queue is saved and the new source's comes back.
    pub(super) fn activate(&mut self, source: Shared, chosen: bool) {
        let old = self.client.as_ref().map(|c| c.id());
        let new = source.id();
        if chosen {
            self.sources.settled = true;
            self.prefs.update(|p| p.source = new);
        }
        self.covers.set_resolver(Arc::new(source.clone()));
        self.player.send(Cmd::Client(source.clone()));
        self.player.send(Cmd::Quality(self.prefs.quality));
        self.player.send(Cmd::ReportListens(self.prefs.report_listens));
        if old != Some(new) {
            if let Some(old) = old {
                self.save_queue_now(old);
                log_info!("switching to {}", new.name);
            }
            self.clear_content();
            self.restore_queue(new);
        }
        let c = source.clone();
        self.library.playlists_load.start(&self.jobs, move || c.playlists());
        let c = source.clone();
        self.library.likes_load.start(&self.jobs, move || c.likes());
        self.client = Some(source);
        self.open_view(View::Home);
    }

    /// Bring back the source's queue from last time, paused where it was.
    fn restore_queue(&mut self, source: SourceId) {
        self.restore_pending = false;
        if let Some(s) = self.queues.load(source) {
            log_info!("restoring a queue of {} tracks", s.tracks.len());
            self.player.send(Cmd::Restore { tracks: s.tracks, index: s.index, position: s.position, flow: s.flow });
            self.restore_pending = true;
        }
        self.queue_saved = Some((0, usize::MAX, None));
    }

    /// Switch from the account menu.
    pub(super) fn switch_source(&mut self, source: Shared) {
        self.activate(source, true);
    }

    /// Log out of the source in use and forget its login; another logged-in one
    /// takes over, otherwise the login screen shows.
    pub(super) fn log_out(&mut self) {
        let Some(id) = self.client.as_ref().map(|c| c.id()) else { return };
        credentials::clear(id);
        self.sources.logged_in.remove(&id);
        match self.sources.first() {
            Some(other) => self.switch_source(other),
            None => {
                self.save_queue_now(id);
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
    pub(super) fn features(&self) -> Features {
        self.client.as_ref().map_or(Features::ALL, |c| c.features())
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
        let modal = egui::Modal::new(egui::Id::unique("server-dialog")).frame(widgets::popover_frame(ctx)).show(ctx, |ui| {
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
        tx.send(Ok(crate::subsonic::Subsonic::offline("nas:4533", "me"))).unwrap();
        t.app.poll_sources();
    }

    fn in_use(t: &TestApp) -> Option<&'static str> {
        t.app.client.as_ref().map(|c| c.id().name)
    }

    /// The tracks of the saved queue of `source`.
    fn saved(t: &TestApp, source: SourceId) -> Option<Vec<u64>> {
        t.queues().load(source).map(|s| ids(&s.tracks))
    }

    /// The tracks of the queue restore sent to the player, if one was.
    fn restored(t: &TestApp) -> Option<Vec<u64>> {
        t.sent().into_iter().find_map(|cmd| match cmd {
            Cmd::Restore { tracks, .. } => Some(ids(&tracks)),
            _ => None,
        })
    }

    #[test]
    fn every_source_has_a_menu_row() {
        let menu: Vec<SourceId> = MENU.iter().map(|row| row.id).collect();
        assert_eq!(menu, KINDS);
    }

    #[test]
    fn the_source_used_last_time_takes_over_when_it_connects_later() {
        let mut t = TestApp::new("source=navidrome\n");
        t.app.source_ready(deezer());
        assert_eq!(in_use(&t), Some("Deezer"), "stands in while the server connects");
        assert_eq!(t.app.prefs.source, subsonic::ID, "without becoming the one used next time");
        server_connects(&mut t);
        assert_eq!(in_use(&t), Some("Navidrome"));
        // Deezer logging in again later doesn't switch away.
        t.app.source_ready(deezer());
        assert_eq!(in_use(&t), Some("Navidrome"));
    }

    #[test]
    fn a_stand_in_gives_way_once() {
        let mut t = TestApp::new("source=deezer\n");
        server_connects(&mut t);
        assert_eq!(in_use(&t), Some("Navidrome"));
        t.app.source_ready(deezer());
        assert_eq!(in_use(&t), Some("Deezer"));
        server_connects(&mut t);
        assert_eq!(in_use(&t), Some("Deezer"), "the server reconnecting doesn't take over");
    }

    #[test]
    fn a_source_picked_by_hand_sticks() {
        let mut t = TestApp::new("source=deezer\n");
        t.app.source_ready(deezer());
        server_connects(&mut t);
        assert_eq!(in_use(&t), Some("Deezer"));
        let server = t.app.sources.get(subsonic::ID).cloned().expect("connected");
        t.update(Action::SwitchSource(server));
        assert_eq!((in_use(&t), t.app.prefs.source), (Some("Navidrome"), subsonic::ID));
        t.app.source_ready(deezer());
        assert_eq!(in_use(&t), Some("Navidrome"), "Deezer logging in again doesn't undo the pick");
    }

    #[test]
    fn logging_out_hands_over_to_the_other_source() {
        let mut t = TestApp::new("source=deezer\n");
        t.app.source_ready(deezer());
        server_connects(&mut t);
        t.update(Action::LogOut);
        assert_eq!(in_use(&t), Some("Navidrome"));
        assert!(t.app.sources.get(deezer::ID).is_none() && t.app.sources.get(subsonic::ID).is_some());
        t.update(Action::LogOut);
        assert_eq!(in_use(&t), None, "the login screen");
        assert!(t.app.sources.first().is_none());
    }

    #[test]
    fn covers_come_from_the_source_in_use() {
        let mut t = TestApp::new("source=deezer\n");
        let art = crate::model::Art::new("al-5F81DCwcfjVByziJAH9maf_0").unwrap();
        assert_eq!(t.app.covers.url(&art, 120), None, "no source yet");
        t.app.source_ready(deezer());
        assert!(t.app.covers.url(&art, 120).unwrap().starts_with("https://cdn-images.dzcdn.net/images/al-5F81DCwcfjVByziJAH9maf_0/"));
        t.update(Action::SwitchSource(navidrome()));
        assert!(t.app.covers.url(&art, 120).unwrap().starts_with("http://nas:4533/rest/getCoverArt?"));
    }

    #[test]
    fn nothing_is_saved_before_the_queue_was_offered_back() {
        let mut t = TestApp::new("");
        t.queues().save(&queue(&[1, 2], 1), deezer::ID);
        // The player's queue before any login: empty, then whatever it is.
        t.app.save_queue_if_changed(&Status::default());
        t.app.save_queue_if_changed(&queue(&[9], 0));
        assert_eq!(saved(&t, deezer::ID), Some(vec![1, 2]));
    }

    #[test]
    fn the_saved_queue_comes_back_and_is_kept_up_to_date() {
        let mut t = TestApp::new("");
        t.queues().save(&queue(&[1, 2], 1), deezer::ID);
        t.app.source_ready(deezer());
        assert_eq!(restored(&t), Some(vec![1, 2]));
        // Until the player shows it, its empty queue isn't saved over it.
        t.app.save_queue_if_changed(&Status::default());
        assert_eq!(saved(&t, deezer::ID), Some(vec![1, 2]));
        t.app.save_queue_if_changed(&queue(&[1, 2], 1));
        t.app.save_queue_if_changed(&queue(&[1, 2, 3], 2));
        assert_eq!(saved(&t, deezer::ID), Some(vec![1, 2, 3]));
        assert_eq!(t.queues().load(deezer::ID).map(|s| s.index), Some(2));
    }

    #[test]
    fn each_source_keeps_its_own_queue() {
        let mut t = TestApp::new("");
        t.queues().save(&queue(&[7], 0), subsonic::ID);
        t.app.source_ready(deezer());
        t.app.player.set_status(|st| *st = queue(&[1, 2], 0));
        t.sent();
        t.update(Action::SwitchSource(navidrome()));
        assert_eq!(saved(&t, deezer::ID), Some(vec![1, 2]), "Deezer's queue is saved on the way out");
        assert_eq!(restored(&t), Some(vec![7]), "and the server's comes back");
    }

    #[test]
    fn switching_before_a_restore_arrives_keeps_the_saved_queue() {
        let mut t = TestApp::new("");
        t.queues().save(&queue(&[1, 2], 1), deezer::ID);
        t.app.source_ready(deezer());
        // The player hasn't shown the restored queue yet: it's still empty.
        t.update(Action::SwitchSource(navidrome()));
        assert_eq!(saved(&t, deezer::ID), Some(vec![1, 2]));
    }
}
