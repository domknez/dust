//! Reusable custom-painted widgets. Views compose these; they know nothing about
//! app state beyond what they're given.

mod art;
mod cards;
mod controls;
mod decor;
mod menu;
mod nav;
mod text;

pub use art::{cover, tile};
pub use cards::{CardClick, card, flow_card, play_knob, queue_row};
pub use controls::{icon_button, pill, play_circle, segmented, thin_slider};
pub use decor::{dust_particles, equalizer, fade_to};
pub use menu::{caption, divider, menu_row, popover_frame, queue_menu};
pub use nav::{nav_item, section};
pub use text::{artist_links, text_left, text_link};
