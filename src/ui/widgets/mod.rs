//! Reusable custom-painted widgets. Views compose these; they know nothing about
//! app state beyond what they're given.

mod art;
mod cards;
mod controls;
mod decor;
mod grid;
mod load_state;
mod menu;
mod nav;
mod text;
mod track_table;

pub use art::{cover, tile};
pub use cards::{CardClick, card, channel_card, flow_panel, play_knob, queue_row};
pub use controls::{heart_at, icon_button, pill, play_button, segmented, tag, text_field, thin_slider, wave_slider};
pub use decor::{equalizer, fade_to};
pub use grid::{Grid, GridColumns};
pub use load_state::load_state;
pub use menu::{TrackMenu, add_to_playlist, caption, divider, menu_row, popover_frame, queue_menu, shortcut_row, track_menu};
pub use nav::sidebar_item;
pub use text::{artist_links, text_left, text_link};
pub use track_table::{Columns, RowMenu, Table, column_captions, track_rows};
