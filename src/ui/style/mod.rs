//! Everything visual that isn't layout logic: colours, type, dimensions, icons,
//! and the egui style they produce.
//!
//! - [`palette`]: dark/light colours and the appearance preference
//! - [`typography`]: the type scale (text roles)
//! - [`metrics`]: panel sizes, row heights, artwork sizes, corner radii
//! - [`icons`]: vector icons

pub mod icons;
pub mod metrics;
pub mod palette;
pub mod typography;

pub use metrics::radius;
pub use palette::{Appearance, colors};

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};
use std::sync::Arc;

/// Register Inter (regular and semibold) ahead of egui's fallback fonts.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("Inter".into(), Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/Inter-Regular.ttf"))));
    fonts
        .font_data
        .insert("Inter-SemiBold".into(), Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"))));
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.insert(FontFamily::Proportional, [vec!["Inter".to_string()], fallbacks.clone()].concat());
    fonts.families.insert(typography::semibold(), [vec!["Inter-SemiBold".to_string()], fallbacks].concat());
    ctx.set_fonts(fonts);
}

/// Switch palettes and restyle egui's built-in widgets to match.
pub fn apply(ctx: &egui::Context, dark: bool) {
    palette::set_dark(dark);
    let theme = if dark { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.set_theme(theme);
    ctx.set_visuals_of(theme, visuals(dark));
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Heading, typography::PAGE_TITLE.font()),
            (TextStyle::Body, typography::TITLE.font()),
            (TextStyle::Button, typography::TITLE.font()),
            (TextStyle::Small, typography::SMALL.font()),
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

fn visuals(dark: bool) -> Visuals {
    let p = colors();
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = CornerRadius::same(radius::PANEL);
    v.menu_corner_radius = CornerRadius::same(radius::PANEL);
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.hover;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.popup_shadow = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(if dark { 110 } else { 45 }) };
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
        w.corner_radius = CornerRadius::same(radius::ROW);
        w.fg_stroke.color = p.text;
    }
    v.widgets.noninteractive.fg_stroke.color = p.dim;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    v
}
