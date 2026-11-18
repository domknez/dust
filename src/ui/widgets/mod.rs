//! Reusable custom-painted widgets. Views compose these; they know nothing about
//! app state beyond what they're given.

mod art;
mod cards;
mod controls;
mod decor;
mod empty;
mod grid;
mod load_state;
mod menu;
mod nav;
mod text;
mod track_table;

pub use art::{cover, tile};
pub use cards::{CardClick, FlowPick, card, channel_card, flow_panel, play_knob, queue_row};
pub use controls::{
    chip, heart_at, icon_button, icon_pill, pill, pill_fit, pill_width, play_button, segmented, tag, text_field, thin_slider, wave_slider,
};
pub use decor::{equalizer, fade_to, wordmark};
pub use empty::{empty_panel, empty_state};
pub use grid::{Grid, GridColumns};
pub use load_state::load_state;
pub use menu::{
    CrateMenu, MenuTarget, NowPlayingMenu, NowPlayingOffer, Playlists, TrackMenu, add_to_playlist, caption, crate_menu, divider, menu_row,
    now_playing_menu, offer_crate, playlist_picker, popover_frame, queue_menu, shortcut_row, switch_row, track_menu,
};
pub use nav::sidebar_item;
pub use text::{artist_links, text_left, text_link};
pub use track_table::{Columns, RowMenu, Table, column_captions, track_rows};
