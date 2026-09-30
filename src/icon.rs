//! App icon, decoded from assets/dust-icon-pack at build time (see build.rs).

use eframe::egui;

#[cfg(target_os = "macos")]
const WINDOW_ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-256-macos.rgba"));
#[cfg(not(target_os = "macos"))]
const WINDOW_ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-256.rgba"));
const LOGO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon-256.rgba"));

/// Window / Dock / taskbar icon. On macOS it carries Apple's icon-grid margin so it
/// matches other apps in the Dock and Cmd-Tab.
pub fn window_icon() -> egui::IconData {
    egui::IconData { rgba: WINDOW_ICON.to_vec(), width: 256, height: 256 }
}

/// Large logo for the login screen.
pub fn logo_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let image = egui::ColorImage::from_rgba_unmultiplied([256, 256], LOGO);
    ctx.load_texture("dust-logo", image, egui::TextureOptions::LINEAR)
}
