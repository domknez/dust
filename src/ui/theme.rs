//! Colours, type and egui style: near-black surfaces, white type, one cyan accent.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x0b);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
pub const BAR: Color32 = Color32::from_rgb(0x0f, 0x0f, 0x11);
pub const SURFACE: Color32 = Color32::from_rgb(0x1b, 0x1b, 0x1f);
pub const HOVER: Color32 = Color32::from_rgb(0x17, 0x17, 0x1a);
pub const LINE: Color32 = Color32::from_rgb(0x26, 0x26, 0x2b);
pub const TEXT: Color32 = Color32::from_rgb(0xf5, 0xf5, 0xf7);
pub const DIM: Color32 = Color32::from_rgb(0xa1, 0xa1, 0xaa);
pub const FAINT: Color32 = Color32::from_rgb(0x6b, 0x6b, 0x74);
pub const ACCENT: Color32 = Color32::from_rgb(0x33, 0xe1, 0xd6);
pub const DANGER: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, semibold())
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("Inter".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Inter-Regular.ttf"))));
    fonts
        .font_data
        .insert("Inter-SemiBold".into(), Arc::new(FontData::from_static(include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"))));
    // Keep egui's defaults behind Inter as fallbacks (emoji, CJK-less symbols).
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.insert(FontFamily::Proportional, [vec!["Inter".to_string()], fallbacks.clone()].concat());
    fonts.families.insert(semibold(), [vec!["Inter-SemiBold".to_string()], fallbacks].concat());
    ctx.set_fonts(fonts);

    let mut v = Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = SURFACE;
    v.faint_bg_color = HOVER;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.popup_shadow = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(160) };
    let r = CornerRadius::same(6);
    for (w, bg) in [
        (&mut v.widgets.noninteractive, BG),
        (&mut v.widgets.inactive, SURFACE),
        (&mut v.widgets.hovered, Color32::from_rgb(0x26, 0x26, 0x2b)),
        (&mut v.widgets.active, Color32::from_rgb(0x30, 0x30, 0x36)),
        (&mut v.widgets.open, SURFACE),
    ] {
        w.bg_fill = bg;
        w.weak_bg_fill = bg;
        w.bg_stroke = Stroke::NONE;
        w.corner_radius = r;
        w.fg_stroke.color = TEXT;
    }
    v.widgets.noninteractive.fg_stroke.color = DIM;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    ctx.set_visuals(v);

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
