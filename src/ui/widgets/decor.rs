//! Decorative touches: the wordmark, the playing-track equalizer and fades.

use crate::ui::style::{colors, radius, typography as ty};
use eframe::egui::{self, Align2, Color32, CornerRadius, Painter, Pos2, Rect, pos2, vec2};
use std::time::Duration;

/// `dust_` in `style`'s mono, the underscore in lime like a cursor, placed at `at`
/// by `align`.
pub fn wordmark(painter: &Painter, at: Pos2, align: Align2, style: ty::Text) {
    let p = colors();
    let word = painter.layout_no_wrap("dust".into(), style.font(), p.text);
    let cursor = painter.layout_no_wrap("_".into(), style.font(), p.accent);
    let word_width = word.size().x;
    let rect = align.anchor_size(at, vec2(word_width + cursor.size().x, word.size().y));
    painter.galley(rect.min, word, p.text);
    painter.galley(rect.min + vec2(word_width, 0.0), cursor, p.accent);
}

/// Three little bars marking the current track; they dance while playing.
pub fn equalizer(p: &Painter, r: Rect, animated: bool) {
    let t = if animated { p.ctx().input(|i| i.time) as f32 } else { 0.0 };
    for (k, phase) in [0.0f32, 1.7, 3.1].into_iter().enumerate() {
        let height = if animated { 0.35 + 0.65 * (0.5 + 0.5 * (t * 7.0 + phase).sin()) } else { 0.5 };
        let x = r.left() + k as f32 * r.width() * 0.38;
        let bar = Rect::from_min_max(pos2(x, r.bottom() - r.height() * height), pos2(x + r.width() * 0.24, r.bottom()));
        p.rect_filled(bar, CornerRadius::same(radius::BAR), colors().accent);
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
