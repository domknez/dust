//! Painting single lines of text, truncated with "…".

use crate::ui::style::typography::Text;
use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{Color32, Galley, Painter, Pos2, Rect, pos2};
use std::sync::Arc;

/// Single-line galley cut with "…" at `max_width`.
pub fn line(painter: &Painter, text: &str, style: Text, color: Color32, max_width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_string(), style.font(), color);
    job.wrap = TextWrapping { max_width: max_width.max(1.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    painter.layout_job(job)
}

/// Paint single-line text with its left edge at `pos.x`, vertically centred on `pos.y`.
/// Returns the painted rect.
pub fn text_left(painter: &Painter, pos: Pos2, text: &str, style: Text, color: Color32, max_width: f32) -> Rect {
    let galley = line(painter, text, style, color, max_width);
    let top = pos2(pos.x, pos.y - galley.size().y / 2.0);
    let rect = Rect::from_min_size(top, galley.size());
    painter.galley(top, galley, color);
    rect
}
