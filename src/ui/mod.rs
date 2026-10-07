//! egui front-end. Reactive: nothing is redrawn unless there is input, a background
//! task finishes, or a track is playing (twice a second for the progress bar).
//!
//! - [`app`]: state, lifecycle and panel layout
//! - [`views`]: one module per screen or panel
//! - [`widgets`]: reusable painted components
//! - [`style`]: colours, type scale, metrics, icons
//! - [`now_playing`]: media keys and the OS "Now Playing" widget
//! - [`updates`]: background update checks and installs
//! - [`state`], [`tasks`], [`format`], [`covers`]: supporting pieces

mod app;
mod covers;
mod format;
mod history;
mod likes;
mod now_playing;
mod state;
mod style;
mod tasks;
mod updates;
mod views;
mod widgets;

use app::App;
use eframe::egui;
use style::metrics;

/// Open the main window and run the app until it closes.
pub fn run() -> eframe::Result {
    let viewport = egui::ViewportBuilder::default()
        .with_title("dust")
        .with_inner_size(metrics::WINDOW_SIZE)
        .with_min_inner_size(metrics::MIN_WINDOW_SIZE)
        .with_icon(std::sync::Arc::new(crate::icon::window_icon()));
    // Content under a transparent title bar, traffic lights floating over the sidebar.
    #[cfg(target_os = "macos")]
    let viewport = viewport.with_fullsize_content_view(true).with_title_shown(false).with_titlebar_shown(false);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("dust", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
