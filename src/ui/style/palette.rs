//! Colours: "Graphite". A near-black charcoal dark and a soft warm-grey light, both
//! deliberately avoiding pure black and pure white, with one lime accent.

use eframe::egui::{self, Color32};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Palette {
    pub bg: Color32,
    pub sidebar: Color32,
    pub bar: Color32,
    /// Cards, fields and other raised areas.
    pub surface: Color32,
    pub hover: Color32,
    /// Hairlines between areas.
    pub line: Color32,
    /// Outlines of secondary buttons and fields.
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    /// Accent for text, icons and strokes.
    pub accent: Color32,
    /// Accent as a fill (primary buttons, tags), with `on_accent` on top.
    pub accent_fill: Color32,
    pub on_accent: Color32,
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

/// The lime of the dark theme, also the fill of primary buttons in both.
const LIME: Color32 = rgb(0xc6, 0xf3, 0x6b);
/// Text on lime.
const ON_LIME: Color32 = rgb(0x1a, 0x1f, 0x0a);

pub static DARK: Palette = Palette {
    bg: rgb(0x16, 0x17, 0x18),
    sidebar: rgb(0x13, 0x14, 0x15),
    bar: rgb(0x13, 0x14, 0x15),
    surface: rgb(0x1c, 0x1d, 0x1f),
    hover: rgb(0x1f, 0x20, 0x22),
    line: rgb(0x2a, 0x2c, 0x2f),
    border: rgb(0x34, 0x36, 0x3a),
    text: rgb(0xe6, 0xe6, 0xe3),
    dim: rgb(0x9c, 0x9e, 0xa3),
    faint: rgb(0x75, 0x78, 0x80),
    accent: LIME,
    accent_fill: LIME,
    on_accent: ON_LIME,
    danger: rgb(0xf2, 0x70, 0x4c),
    raised: rgb(0x2a, 0x2c, 0x2f),
    pressed: rgb(0x3a, 0x3c, 0x40),
    tile_loved: rgb(0x5b, 0x4b, 0xc9),
    veil: Color32::from_black_alpha(70),
};

pub static LIGHT: Palette = Palette {
    bg: rgb(0xec, 0xec, 0xe8),
    sidebar: rgb(0xe4, 0xe4, 0xdf),
    bar: rgb(0xe4, 0xe4, 0xdf),
    surface: rgb(0xf3, 0xf3, 0xef),
    hover: rgb(0xe2, 0xe2, 0xdd),
    line: rgb(0xd3, 0xd3, 0xcd),
    border: rgb(0xc4, 0xc4, 0xbd),
    text: rgb(0x1c, 0x1d, 0x1f),
    dim: rgb(0x5e, 0x60, 0x64),
    faint: rgb(0x85, 0x87, 0x8b),
    // Olive for text and icons on the grey (5:1); buttons keep the lime fill.
    accent: rgb(0x4c, 0x6a, 0x0a),
    accent_fill: LIME,
    on_accent: ON_LIME,
    danger: rgb(0xc2, 0x41, 0x2a),
    raised: rgb(0xda, 0xda, 0xd4),
    pressed: rgb(0xcf, 0xcf, 0xc8),
    tile_loved: rgb(0x6f, 0x5f, 0xd6),
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
