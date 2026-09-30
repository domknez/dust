//! App icon, decoded from assets/dust-icon-pack at build time (see build.rs).

use eframe::egui;

const ICON_256: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-256.rgba"));
const ICON_64: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-64.rgba"));

/// Window / Dock / taskbar icon.
pub fn window_icon() -> egui::IconData {
    egui::IconData { rgba: ICON_256.to_vec(), width: 256, height: 256 }
}

/// In-app logo: `large` for the login screen, otherwise the small variant.
pub fn texture(ctx: &egui::Context, large: bool) -> egui::TextureHandle {
    let (rgba, size, name) = if large { (ICON_256, 256, "dust-logo-256") } else { (ICON_64, 64, "dust-logo-64") };
    let image = egui::ColorImage::from_rgba_unmultiplied([size, size], rgba);
    ctx.load_texture(name, image, egui::TextureOptions::LINEAR)
}
