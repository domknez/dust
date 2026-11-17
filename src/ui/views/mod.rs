//! Screens and panels, each an `impl App` block drawing one part of the window.

mod account_menu;
mod artist;
mod artists;
mod channel;
mod collection;
mod content;
mod crate_view;
mod home;
mod login;
mod lyrics;
mod mini_player;
mod output_picker;
mod play_history;
mod player_bar;
mod playlists;
mod queue;
mod settings;
mod sidebar;
mod sleep_timer;
mod stats_view;
mod sync_settings;

pub use crate_view::{CrateView, Filter};
pub use lyrics::LyricsState;
pub use settings::{Section, reveal_log};
pub use sleep_timer::SleepChoice;
pub use stats_view::StatsView;
pub use sync_settings::Export;
