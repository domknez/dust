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

/// `text` as a link: left edge at `pos.x`, centred on `pos.y`, underlined on hover.
/// Returns whether it was clicked and its right edge.
#[allow(clippy::too_many_arguments)]
pub fn text_link(
    ui: &Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    pos: Pos2,
    text: &str,
    style: Text,
    color: Color32,
    hover: Color32,
    max_width: f32,
) -> (bool, f32) {
    let painter = ui.painter();
    let galley = line(painter, text, style, color, max_width);
    let rect = Rect::from_min_size(pos2(pos.x, pos.y - galley.size().y / 2.0), galley.size());
    let resp = ui.interact(rect, ui.id().with(id), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let tint = if resp.hovered() { hover } else { color };
    painter.galley_with_override_text_color(rect.min, galley, tint);
    if resp.hovered() {
        painter.hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(1.0, tint));
    }
    (resp.clicked(), rect.right())
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
        let id = ("artist-link", &artist.id, x as i32, pos.y as i32);
        let (hit, end) = text_link(ui, id, pos2(x, pos.y), &artist.name, style, color, hover, right - x);
        if hit {
            clicked = Some(artist.clone());
        }
        x = end;
    }
    (clicked, x)
}
