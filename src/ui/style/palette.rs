//! Colours. Two soft palettes — a charcoal dark and a grey light — deliberately
//! avoiding pure black and pure white, which are harsh on the eyes. The accent is
//! the blue from the app icon.

use eframe::egui::{self, Color32};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Palette {
    pub bg: Color32,
    pub sidebar: Color32,
    pub bar: Color32,
    pub surface: Color32,
    pub hover: Color32,
    pub line: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub danger: Color32,
    /// Hover/pressed fills for secondary buttons.
    pub raised: Color32,
    pub pressed: Color32,
    /// Tile for the Loved collection, which has no artwork.
    pub tile_loved: Color32,
    /// Veil drawn over artwork on hover.
    pub veil: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub static DARK: Palette = Palette {
    bg: rgb(0x1c, 0x1c, 0x20),
    sidebar: rgb(0x17, 0x17, 0x1a),
    bar: rgb(0x21, 0x21, 0x26),
    surface: rgb(0x2b, 0x2b, 0x31),
    hover: rgb(0x25, 0x25, 0x2a),
    line: rgb(0x35, 0x35, 0x3c),
    text: rgb(0xe8, 0xe8, 0xec),
    dim: rgb(0xa4, 0xa4, 0xae),
    faint: rgb(0x77, 0x77, 0x81),
    // Brand blue from the app icon (#3D8BFF), lifted slightly for text on charcoal.
    accent: rgb(0x5a, 0x9c, 0xff),
    danger: rgb(0xf0, 0x7a, 0x7a),
    raised: rgb(0x36, 0x36, 0x3d),
    pressed: rgb(0x40, 0x40, 0x48),
    tile_loved: rgb(0x92, 0x3a, 0x60),
    veil: Color32::from_black_alpha(60),
};

pub static LIGHT: Palette = Palette {
    bg: rgb(0xee, 0xee, 0xf1),
    sidebar: rgb(0xe5, 0xe5, 0xea),
    bar: rgb(0xe8, 0xe8, 0xed),
    surface: rgb(0xdb, 0xdb, 0xe1),
    hover: rgb(0xe2, 0xe2, 0xe7),
    line: rgb(0xd0, 0xd0, 0xd8),
    text: rgb(0x1f, 0x1f, 0x24),
    dim: rgb(0x56, 0x56, 0x60),
    faint: rgb(0x84, 0x84, 0x8e),
    // Deeper brand blue for contrast on the grey background.
    accent: rgb(0x24, 0x5e, 0xbd),
    danger: rgb(0xc4, 0x40, 0x40),
    raised: rgb(0xcf, 0xcf, 0xd6),
    pressed: rgb(0xc4, 0xc4, 0xcc),
    tile_loved: rgb(0xc0, 0x55, 0x80),
    veil: Color32::from_black_alpha(40),
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

/// The active palette.
pub fn colors() -> &'static Palette {
    if is_dark() { &DARK } else { &LIGHT }
}

pub fn is_dark() -> bool {
    IS_DARK.load(Ordering::Relaxed)
}

pub(super) fn set_dark(dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
}

/// The listener's appearance preference.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Appearance {
    System,
    Dark,
    Light,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Dark, Appearance::Light];

    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "System",
            Appearance::Dark => "Dark",
            Appearance::Light => "Light",
        }
    }

    /// Stable key for the settings file.
    pub fn key(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Dark => "dark",
            Appearance::Light => "light",
        }
    }

    pub fn from_key(k: &str) -> Option<Appearance> {
        Appearance::ALL.into_iter().find(|m| m.key() == k)
    }

    /// Whether this preference currently means dark.
    pub fn is_dark(self, ctx: &egui::Context) -> bool {
        match self {
            Appearance::Dark => true,
            Appearance::Light => false,
            Appearance::System => ctx.system_theme() != Some(egui::Theme::Light),
        }
    }
}
