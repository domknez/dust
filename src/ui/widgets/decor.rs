//! Decorative touches: the playing-track equalizer and fades.

use crate::ui::style::colors;
use eframe::egui::{self, Color32, CornerRadius, Painter, Rect, pos2};
use std::time::Duration;

/// Three little bars marking the current track; they dance while playing.
pub fn equalizer(p: &Painter, r: Rect, animated: bool) {
    let t = if animated { p.ctx().input(|i| i.time) as f32 } else { 0.0 };
    for (k, phase) in [0.0f32, 1.7, 3.1].into_iter().enumerate() {
        let height = if animated { 0.35 + 0.65 * (0.5 + 0.5 * (t * 7.0 + phase).sin()) } else { 0.5 };
        let x = r.left() + k as f32 * r.width() * 0.38;
        let bar = Rect::from_min_max(pos2(x, r.bottom() - r.height() * height), pos2(x + r.width() * 0.24, r.bottom()));
        p.rect_filled(bar, CornerRadius::same(1), colors().accent);
    }
    if animated {
        p.ctx().request_repaint_after(Duration::from_millis(80));
    }
}

/// Vertical fade from transparent to `color`, drawn over content above the rect's bottom.
pub fn fade_to(p: &Painter, rect: Rect, color: Color32) {
    let clear = color.gamma_multiply(0.0);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), clear);
    mesh.colored_vertex(rect.right_top(), clear);
    mesh.colored_vertex(rect.right_bottom(), color);
    mesh.colored_vertex(rect.left_bottom(), color);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(mesh);
}
