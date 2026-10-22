//! Screens and panels, each an `impl App` block drawing one part of the window.

mod account_menu;
mod artist;
mod artists;
mod collection;
mod content;
mod home;
mod login;
mod lyrics;
mod mini_player;
mod output_picker;
mod player_bar;
mod playlists;
mod queue;
mod settings;
mod sidebar;
mod sleep_timer;

pub use lyrics::LyricsState;
pub use sleep_timer::SleepChoice;
