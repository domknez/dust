//! Decorative touches: the playing-track equalizer, wordmark particles, fades.

use crate::ui::style::colors;
use eframe::egui::{self, Color32, CornerRadius, Painter, Pos2, Rect, pos2, vec2};
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

/// A few brand-blue specks drifting up and right, echoing the particles in the icon.
pub fn dust_particles(p: &Painter, origin: Pos2) {
    // (dx, dy, radius, opacity)
    const SPECKS: [(f32, f32, f32, f32); 7] =
        [(0.0, 0.0, 2.4, 1.0), (5.0, -4.0, 1.8, 0.85), (9.0, -1.0, 1.4, 0.7), (11.0, -8.0, 1.6, 0.6), (15.0, -4.5, 1.1, 0.5), (18.0, -10.0, 1.0, 0.4), (21.0, -6.0, 0.8, 0.3)];
    for (dx, dy, r, opacity) in SPECKS {
        p.circle_filled(origin + vec2(dx, dy), r, colors().accent.gamma_multiply(opacity));
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
