//! Colours, type and egui style. Two soft palettes — a charcoal dark and a grey
//! light — deliberately avoiding pure black and pure white, which are harsh on the eyes.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};
use std::sync::Arc;
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
    accent: rgb(0x4f, 0xd6, 0xcc),
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
    accent: rgb(0x10, 0x8c, 0x85),
    danger: rgb(0xc4, 0x40, 0x40),
    raised: rgb(0xcf, 0xcf, 0xd6),
    pressed: rgb(0xc4, 0xc4, 0xcc),
    tile_loved: rgb(0xc0, 0x55, 0x80),
    veil: Color32::from_black_alpha(40),
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

/// Active palette.
pub fn c() -> &'static Palette {
    if IS_DARK.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

pub fn is_dark() -> bool {
    IS_DARK.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    System,
    Dark,
    Light,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::System, Mode::Dark, Mode::Light];

    pub fn label(self) -> &'static str {
        match self {
            Mode::System => "System",
            Mode::Dark => "Dark",
            Mode::Light => "Light",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Mode::System => "system",
            Mode::Dark => "dark",
            Mode::Light => "light",
        }
    }

    pub fn from_key(k: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.key() == k)
    }

    /// Whether this mode currently means dark.
    pub fn resolve(self, ctx: &egui::Context) -> bool {
        match self {
            Mode::Dark => true,
            Mode::Light => false,
            Mode::System => ctx.system_theme() != Some(egui::Theme::Light),
        }
    }
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, semibold())
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("Inter".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Inter-Regular.ttf"))));
    fonts
        .font_data
        .insert("Inter-SemiBold".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"))));
    // Keep egui's defaults behind Inter as fallbacks for symbols Inter lacks.
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.insert(FontFamily::Proportional, [vec!["Inter".to_string()], fallbacks.clone()].concat());
    fonts.families.insert(semibold(), [vec!["Inter-SemiBold".to_string()], fallbacks].concat());
    ctx.set_fonts(fonts);
}

/// Switch palettes and restyle egui's built-in widgets to match.
pub fn apply(ctx: &egui::Context, dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
    let p = c();
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.hover;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.popup_shadow = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(if dark { 110 } else { 45 }) };
    let r = CornerRadius::same(6);
    for (w, bg) in [
        (&mut v.widgets.noninteractive, p.bg),
        (&mut v.widgets.inactive, p.surface),
        (&mut v.widgets.hovered, p.raised),
        (&mut v.widgets.active, p.pressed),
        (&mut v.widgets.open, p.surface),
    ] {
        w.bg_fill = bg;
        w.weak_bg_fill = bg;
        w.bg_stroke = Stroke::NONE;
        w.corner_radius = r;
        w.fg_stroke.color = p.text;
    }
    v.widgets.noninteractive.fg_stroke.color = p.dim;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    let theme = if dark { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.set_theme(theme);
    ctx.set_visuals_of(theme, v);

    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Heading, bold(26.0)),
            (TextStyle::Body, regular(14.0)),
            (TextStyle::Button, regular(14.0)),
            (TextStyle::Small, regular(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.scroll = egui::style::ScrollStyle::floating();
        s.spacing.scroll.bar_width = 6.0;
        s.visuals.override_text_color = None;
    });
}
