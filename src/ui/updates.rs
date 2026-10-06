//! Updates in the background: check, download and stage a new release quietly,
//! then offer "Restart to update" in the sidebar. A staged update that was never
//! applied installs when dust quits.

use crate::settings;
use crate::ui::app::App;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, radius, typography as ty};
use crate::ui::widgets::text_left;
use crate::update::{self, Release, Restart, Staged};
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
    /// Fetching and unpacking in the background; nothing in the sidebar yet.
    Downloading {
        release: Release,
        progress: Arc<AtomicU32>,
        done: Receiver<Result<Staged, String>>,
    },
    /// Staged next to this copy: one click swaps it in and restarts.
    Ready(Release, Staged),
    /// dust can't replace itself here (dev build, read-only folder): offer the page.
    Manual(Release),
    /// Swapping in failed; the button falls back to the release page.
    Failed(Release, String),
}

pub struct Updates {
    /// "Check automatically" (setting).
    pub auto: bool,
    pub state: State,
    checking: Option<Receiver<Result<Option<Release>, String>>>,
    /// A check has completed since startup.
    checked: bool,
    next_check: Instant,
    /// Why the last check or download failed, for the account menu.
    last_error: Option<String>,
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
            State::Downloading { release, progress, .. } => {
                format!("Downloading dust {}… {}%", release.version, progress.load(Ordering::Relaxed) / 10)
            }
            State::Ready(release, _) => format!("dust {} is ready to install", release.version),
            State::Manual(release) | State::Failed(release, _) => format!("dust {} is available", release.version),
            State::UpToDate => match &self.last_error {
                Some(_) => "Couldn't check for updates".into(),
                None if self.checked => format!("dust {current} is the latest version"),
                None => format!("You have dust {current}"),
            },
        }
    }

    pub fn set_auto(&mut self, auto: bool) {
        self.auto = auto;
        settings::set("auto_update", if auto { "true" } else { "false" });
    }

    /// Start a check now (menu "Check now", or when one is due).
    pub fn check_now(&mut self, ctx: &egui::Context) {
        let busy = matches!(self.state, State::Downloading { .. } | State::Ready(..));
        if self.checking.is_some() || busy {
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

    /// Call every frame: runs due checks and collects background results.
    pub fn poll(&mut self, ctx: &egui::Context) {
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
                        Ok(Some(release)) => self.found(ctx, release),
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
        if let State::Downloading { done, release, .. } = &self.state {
            ctx.request_repaint_after(Duration::from_millis(500));
            match done.try_recv() {
                Ok(Ok(staged)) => {
                    log_info!("update: {} staged, waiting for a restart", staged.version);
                    self.state = State::Ready(release.clone(), staged);
                }
                // Quietly try again at the next check.
                Ok(Err(e)) => {
                    log_warn!("update download failed: {e}");
                    self.last_error = Some(e);
                    self.state = State::UpToDate;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.state = State::UpToDate,
            }
        }
    }

    /// A newer release exists: download it in the background, or offer its page.
    fn found(&mut self, ctx: &egui::Context, release: Release) {
        self.last_error = None;
        if !update::can_install(&release) {
            log_info!("update: {} available (install manually)", release.version);
            self.state = State::Manual(release);
            return;
        }
        log_info!("update: {} available, downloading", release.version);
        let progress = Arc::new(AtomicU32::new(0));
        let (tx, done) = std::sync::mpsc::channel();
        let (p, ctx, pending) = (progress.clone(), ctx.clone(), release.clone());
        std::thread::spawn(move || {
            let _ = tx.send(update::stage(&pending, &p));
            ctx.request_repaint();
        });
        self.state = State::Downloading { release, progress, done };
    }

    /// The sidebar button was clicked. Returns a restart when the update was
    /// swapped in; the caller relaunches and quits.
    fn act(&mut self, ctx: &egui::Context) -> Option<Restart> {
        match std::mem::replace(&mut self.state, State::UpToDate) {
            State::Ready(release, staged) => match update::apply(&staged) {
                Ok(restart) => return Some(restart),
                Err(e) => {
                    log_warn!("update failed: {e}");
                    self.state = State::Failed(release, e);
                }
            },
            State::Manual(release) | State::Failed(release, _) => {
                ctx.open_url(egui::OpenUrl::new_tab(&release.page));
                self.state = State::Manual(release);
            }
            other => self.state = other,
        }
        None
    }

    /// dust is quitting: install a staged update so the next start is the new version.
    pub fn apply_on_exit(&mut self) {
        if let State::Ready(_, staged) = &self.state {
            match update::apply(staged) {
                Ok(_) => log_info!("update: installed on quit"),
                Err(e) => log_warn!("update on quit failed: {e}"),
            }
        }
    }
}

// ---------------------------------------------------------------- sidebar button

const BUTTON_HEIGHT: f32 = 36.0;
const BUTTON_GAP: f32 = 6.0;

impl App {
    /// Footer space the update button needs (none while there is nothing to offer).
    pub(in crate::ui) fn update_button_height(&self) -> f32 {
        match self.updates.state {
            State::Ready(..) | State::Manual(_) | State::Failed(..) => BUTTON_HEIGHT + BUTTON_GAP,
            State::UpToDate | State::Downloading { .. } => 0.0,
        }
    }

    pub(in crate::ui) fn update_button(&mut self, ui: &mut Ui) {
        let p = colors();
        let (icon, label, hint, color) = match &self.updates.state {
            State::UpToDate | State::Downloading { .. } => return,
            State::Ready(release, _) => (
                Icon::Refresh,
                format!("Restart to update to {}", release.version),
                "Installs the update and reopens dust".to_string(),
                p.accent,
            ),
            State::Manual(release) => {
                (Icon::Refresh, format!("dust {} is out", release.version), "Open the download page".to_string(), p.accent)
            }
            State::Failed(_, error) => {
                (Icon::Close, "Update failed".to_string(), format!("{error}\nClick to download it manually"), p.danger)
            }
        };
        ui.add_space(BUTTON_GAP);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), BUTTON_HEIGHT), Sense::click());
        let fill = if resp.hovered() { p.raised } else { p.surface };
        ui.painter().rect_filled(rect, CornerRadius::same(radius::PANEL), fill);
        let icon_rect = Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), Vec2::splat(14.0));
        icons::paint(ui.painter(), icon_rect, icon, color);
        text_left(ui.painter(), pos2(rect.left() + 38.0, rect.center().y), &label, ty::ITEM.strong(), color, rect.width() - 46.0);
        if resp.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
            && let Some(restart) = self.updates.act(ui.ctx())
        {
            update::relaunch(&restart);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
