//! Update checks in the background and the sidebar button that installs them.

use crate::settings;
use crate::ui::app::App;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, radius, typography as ty};
use crate::ui::widgets::text_left;
use crate::update::{self, Release, Restart};
use eframe::egui::{self, CornerRadius, Rect, Sense, Ui, Vec2, pos2, vec2};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Let startup traffic (login, home page, covers) go first.
const FIRST_CHECK: Duration = Duration::from_secs(5);

pub enum State {
    UpToDate,
    /// `installable`: dust can replace itself; otherwise the button opens the page.
    Available {
        release: Release,
        installable: bool,
    },
    Installing {
        release: Release,
        progress: Arc<AtomicU32>,
        done: Receiver<Result<Restart, String>>,
    },
    /// Keeps the release so the button can fall back to its page.
    Failed(Release, String),
}

pub struct Updates {
    /// "Check for updates automatically" (setting).
    pub auto: bool,
    pub state: State,
    checking: Option<Receiver<Result<Option<Release>, String>>>,
    /// A check has completed since startup.
    checked: bool,
    next_check: Instant,
    /// Result of the last check, for the account menu.
    pub last_error: Option<String>,
}

impl Updates {
    pub fn new(auto: bool) -> Self {
        update::clean_up();
        Self { auto, state: State::UpToDate, checking: None, checked: false, next_check: Instant::now() + FIRST_CHECK, last_error: None }
    }

    /// One line for the account menu.
    pub fn status(&self) -> String {
        let current = update::CURRENT;
        match &self.state {
            _ if self.checking.is_some() => "Checking…".into(),
            State::Available { release, .. } | State::Failed(release, _) => format!("dust {} is available", release.version),
            State::Installing { release, .. } => format!("Installing dust {}", release.version),
            State::UpToDate if self.last_error.is_some() => "Couldn't reach GitHub".into(),
            State::UpToDate if self.checked => format!("dust {current} is the latest version"),
            State::UpToDate => format!("You have dust {current}"),
        }
    }

    pub fn set_auto(&mut self, auto: bool) {
        self.auto = auto;
        settings::set("auto_update", if auto { "true" } else { "false" });
    }

    /// Start a check now (menu "Check now", or when one is due).
    pub fn check_now(&mut self, ctx: &egui::Context) {
        if self.checking.is_some() || matches!(self.state, State::Installing { .. }) {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(update::check());
            ctx.request_repaint();
        });
        self.checking = Some(rx);
        self.next_check = Instant::now() + INTERVAL;
    }

    /// Call every frame: runs due checks and collects results. Returns a restart
    /// once an install finished; the caller quits after [`update::relaunch`].
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Restart> {
        if self.auto && Instant::now() >= self.next_check {
            self.check_now(ctx);
        }
        if self.auto {
            ctx.request_repaint_after(self.next_check.saturating_duration_since(Instant::now()));
        }
        if let Some(rx) = &self.checking {
            match rx.try_recv() {
                Ok(result) => {
                    self.checking = None;
                    self.checked = true;
                    match result {
                        Ok(Some(release)) => {
                            let installable = update::can_install(&release);
                            log_info!("update: {} available (installs in place: {installable})", release.version);
                            self.last_error = None;
                            self.state = State::Available { release, installable };
                        }
                        Ok(None) => self.last_error = None,
                        Err(e) => {
                            log_debug!("update check failed: {e}");
                            self.last_error = Some(e);
                        }
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.checking = None,
            }
        }
        if let State::Installing { done, .. } = &self.state {
            ctx.request_repaint_after(Duration::from_millis(200));
            match done.try_recv() {
                Ok(Ok(restart)) => return Some(restart),
                Ok(Err(e)) => self.fail(e),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.fail("update stopped unexpectedly".into()),
            }
        }
        None
    }

    fn fail(&mut self, error: String) {
        log_warn!("update failed: {error}");
        if let State::Installing { release, .. } = &self.state {
            self.state = State::Failed(release.clone(), error);
        }
    }

    /// The sidebar button was clicked: install in place, or open the release page.
    pub fn act(&mut self, ctx: &egui::Context) {
        let release = match &self.state {
            State::Available { release, installable: true } => release.clone(),
            State::Available { release: r, installable: false } | State::Failed(r, _) => {
                ctx.open_url(egui::OpenUrl::new_tab(&r.page));
                return;
            }
            State::UpToDate | State::Installing { .. } => return,
        };
        let progress = Arc::new(AtomicU32::new(0));
        let (tx, done) = std::sync::mpsc::channel();
        let (p, ctx2) = (progress.clone(), ctx.clone());
        let pending = release.clone();
        std::thread::spawn(move || {
            let _ = tx.send(update::install(&pending, &p));
            ctx2.request_repaint();
        });
        self.state = State::Installing { release, progress, done };
    }

    /// Download progress 0..=1.
    pub fn progress(&self) -> Option<f32> {
        match &self.state {
            State::Installing { progress, .. } => Some(progress.load(Ordering::Relaxed) as f32 / 1000.0),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- sidebar button

/// Height the button takes in the sidebar footer (0 when there is nothing to show).
pub const BUTTON_HEIGHT: f32 = 36.0;
const BUTTON_GAP: f32 = 6.0;

impl App {
    /// Footer space the update button needs.
    pub(in crate::ui) fn update_button_height(&self) -> f32 {
        if matches!(self.updates.state, State::UpToDate) { 0.0 } else { BUTTON_HEIGHT + BUTTON_GAP }
    }

    pub(in crate::ui) fn update_button(&mut self, ui: &mut Ui) {
        let p = colors();
        let (label, hint, color) = match &self.updates.state {
            State::UpToDate => return,
            State::Available { release, installable: true } => {
                (format!("Update to {}", release.version), "Download, install and restart dust".to_string(), p.accent)
            }
            State::Available { release, installable: false } => {
                (format!("dust {} is out", release.version), "Open the download page".to_string(), p.accent)
            }
            State::Installing { .. } => {
                let percent = (self.updates.progress().unwrap_or(0.0) * 100.0).round();
                (format!("Updating… {percent}%"), "dust restarts when it's done".to_string(), p.accent)
            }
            State::Failed(_, error) => ("Update failed".to_string(), format!("{error}\nClick to download it manually"), p.danger),
        };
        ui.add_space(BUTTON_GAP);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), BUTTON_HEIGHT), Sense::click());
        let fill = if resp.hovered() { p.raised } else { p.surface };
        ui.painter().rect_filled(rect, CornerRadius::same(radius::PANEL), fill);
        if let Some(progress) = self.updates.progress() {
            let done = Rect::from_min_size(rect.min, vec2(rect.width() * progress, rect.height()));
            ui.painter().rect_filled(done, CornerRadius::same(radius::PANEL), color.gamma_multiply(0.18));
        }
        let icon = Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(14.0));
        icons::paint(ui.painter(), icon, if matches!(self.updates.state, State::Failed(..)) { Icon::Close } else { Icon::Refresh }, color);
        text_left(ui.painter(), pos2(rect.left() + 38.0, rect.center().y), &label, ty::ITEM.strong(), color, rect.width() - 46.0);
        if resp.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.updates.act(ui.ctx());
        }
    }
}
