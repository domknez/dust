//! Painting single lines of text, truncated with "…", and artist-name links.

use crate::deezer::ArtistRef;
use crate::ui::style::typography::Text;
use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{Color32, CursorIcon, Galley, Painter, Pos2, Rect, Sense, Stroke, Ui, pos2};
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

/// Artist names as links ("A, B"): left edge at `pos.x`, centred on `pos.y`. Each name
/// underlines on hover. Returns the one clicked and the right edge of what was painted.
/// Without linkable artists, paints `fallback` as plain text.
#[allow(clippy::too_many_arguments)]
pub fn artist_links(
    ui: &Ui,
    pos: Pos2,
    artists: &[ArtistRef],
    fallback: &str,
    style: Text,
    color: Color32,
    hover: Color32,
    max_width: f32,
) -> (Option<ArtistRef>, f32) {
    if artists.is_empty() {
        return (None, text_left(ui.painter(), pos, fallback, style, color, max_width).right());
    }
    let painter = ui.painter();
    let right = pos.x + max_width;
    let mut x = pos.x;
    let mut clicked = None;
    for (i, artist) in artists.iter().enumerate() {
        if i > 0 {
            x = text_left(painter, pos2(x, pos.y), ", ", style, color, right - x).right();
        }
        if right - x < 12.0 {
            break;
        }
        let galley = line(painter, &artist.name, style, color, right - x);
        let rect = Rect::from_min_size(pos2(x, pos.y - galley.size().y / 2.0), galley.size());
        let id = ui.id().with(("artist-link", &artist.id, rect.min.x as i32, rect.min.y as i32));
        let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let tint = if resp.hovered() { hover } else { color };
        painter.galley_with_override_text_color(rect.min, galley, tint);
        if resp.hovered() {
            painter.hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(1.0, tint));
        }
        if resp.clicked() {
            clicked = Some(artist.clone());
        }
        x = rect.right();
    }
    (clicked, x)
}
