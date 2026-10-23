//! Track tables: one row per track with its number (or the equalizer / a play
//! button), artwork, title, linked artists, album and duration, a heart and a
//! right-click menu. Only the rows asked for are drawn, so a long list stays cheap.
//! Rows report what was clicked as [`Action`]s.

use super::{TrackMenu, artist_links, cover, equalizer, heart_at, queue_menu, text_left, text_link, track_menu};
use crate::model::{Likeable, Playlist, Track, TrackId};
use crate::player::Cmd;
use crate::ui::action::{Action, Actions};
use crate::ui::covers::Covers;
use crate::ui::format::{mmss, release_date};
use crate::ui::state::Coll;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use eframe::egui::{self, Align2, CornerRadius, Pos2, Rect, Sense, Ui, Vec2, pos2, vec2};
use std::ops::Range;

/// Below this width the album column is hidden.
const ALBUM_COLUMN_MIN_WIDTH: f32 = 640.0;

/// X positions of the table columns, and the row height.
pub struct Columns {
    index: f32,
    /// Artwork left edge and size.
    art: Option<(f32, f32)>,
    title: f32,
    /// Artists under the title (two-line rows) rather than in their own column.
    artists_below: bool,
    artist: Option<f32>,
    album: Option<f32>,
    heart: f32,
    time: f32,
    pub row: f32,
}

impl Columns {
    /// Lists mixing albums (playlists, Loved, search): artwork, title over artists,
    /// album.
    pub fn list(left: f32, width: f32) -> Self {
        Columns {
            index: left + 8.0,
            art: Some((left + 48.0, metrics::TRACK_ART)),
            title: left + 104.0,
            artists_below: true,
            artist: None,
            album: (width > ALBUM_COLUMN_MIN_WIDTH).then_some(left + width * 0.58),
            heart: left + width - 78.0,
            time: left + width - 16.0,
            row: metrics::TRACK_ROW,
        }
    }

    /// One album: number, title and artists in compact rows, no artwork.
    pub fn album(left: f32, width: f32) -> Self {
        Columns {
            index: left + 8.0,
            art: None,
            title: left + 52.0,
            artists_below: false,
            artist: (width > 520.0).then_some(left + width * 0.6),
            album: None,
            heart: left + width - 78.0,
            time: left + width - 16.0,
            row: metrics::ALBUM_ROW,
        }
    }

    /// An artist's popular tracks: rank, small artwork and title.
    pub fn popular(left: f32, width: f32) -> Self {
        Columns {
            index: left + 8.0,
            art: Some((left + 40.0, metrics::LIST_ART)),
            title: left + 40.0 + metrics::LIST_ART + 14.0,
            artists_below: false,
            artist: None,
            album: None,
            heart: left + width - 78.0,
            time: left + width - 16.0,
            row: metrics::POPULAR_ROW,
        }
    }
}

/// What a row's right-click menu offers beyond queueing.
pub struct RowMenu<'a> {
    /// The account's playlists; the own ones are offered for "Add to playlist".
    pub playlists: &'a [Playlist],
    /// Showing one of the own playlists: rows can be removed from it.
    pub removable: bool,
}

/// A track list as a table.
pub struct Table<'a> {
    /// The whole list: playing from a row plays it from there.
    pub tracks: &'a [Track],
    pub columns: &'a Columns,
    /// The playing track's id, and whether it's playing (not paused).
    pub now: (Option<&'a TrackId>, bool),
    pub liked: &'a dyn Fn(&TrackId) -> bool,
    pub menu: RowMenu<'a>,
}

/// The rows in `range` of a table whose first row is at `top`, each `width` wide.
pub fn track_rows(ui: &mut Ui, covers: &mut Covers, table: &Table, range: Range<usize>, top: Pos2, width: f32, acts: &mut Actions) {
    let row = table.columns.row;
    for i in range {
        let rect = Rect::from_min_size(top + vec2(0.0, i as f32 * row), vec2(width, row));
        track_row(ui, covers, table, i, rect, acts);
    }
}

/// One row of a track table: number (or equalizer / play icon), artwork, title,
/// linked artists, album, duration, for track `i` of the table.
fn track_row(ui: &mut Ui, covers: &mut Covers, table: &Table, i: usize, rect: Rect, acts: &mut Actions) {
    let p = colors();
    let (track, columns, menu) = (&table.tracks[i], table.columns, &table.menu);
    let (playing_id, playing) = table.now;
    let liked = (table.liked)(&track.id);
    let resp = ui.interact(rect, ui.scope_id().with(("row", i)), Sense::click());
    let current = playing_id == Some(&track.id);
    // Pointer anywhere in the row, including over its own heart and links: asking
    // `hovered()` would flip to false over the heart and make it flicker.
    let hovered = ui.rect_contains_pointer(rect);
    let available = track.available;
    let resp = if available { resp } else { resp.on_hover_text(unavailable_note(track)) };
    if hovered && available {
        ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
    } else if current {
        ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.surface);
    }
    let cy = rect.center().y;
    let index = Rect::from_center_size(pos2(columns.index + 12.0, cy), Vec2::splat(14.0));
    if hovered && available {
        icons::paint(ui.painter(), index, if current && playing { Icon::Pause } else { Icon::Play }, p.text);
    } else if current {
        equalizer(ui.painter(), index, playing);
    } else {
        ui.painter().text(index.center(), Align2::CENTER_CENTER, format!("{:02}", i + 1), ty::META.font(), p.faint);
    }
    if let Some((x, size)) = columns.art {
        let art = Rect::from_min_size(pos2(x, cy - size / 2.0), Vec2::splat(size));
        cover(ui, covers, track.art.as_ref(), metrics::THUMB_PX, art, radius::THUMB);
        if !available {
            // Faded artwork, like the text.
            ui.painter().rect_filled(art, CornerRadius::same(radius::THUMB), p.bg.gamma_multiply(0.65));
        }
    }
    let next_column = columns.artist.or(columns.album).unwrap_or(columns.heart - 24.0);
    let title_width = next_column - columns.title - 16.0;
    let (title_color, sub_color) = match (available, current) {
        (false, _) => (p.faint, p.faint),
        (true, true) => (p.accent, p.dim),
        (true, false) => (p.text, p.dim),
    };
    let artist = if columns.artists_below {
        text_left(ui.painter(), pos2(columns.title, cy - 9.0), &track.title, ty::TITLE, title_color, title_width);
        let at = pos2(columns.title, cy + 10.0);
        artist_links(ui, at, &track.artists, &track.artist, ty::SECONDARY, sub_color, p.text, title_width).0
    } else {
        text_left(ui.painter(), pos2(columns.title, cy), &track.title, ty::TITLE, title_color, title_width);
        columns.artist.and_then(|x| {
            let width = columns.heart - 24.0 - x;
            artist_links(ui, pos2(x, cy), &track.artists, &track.artist, ty::BODY, sub_color, p.text, width).0
        })
    };
    acts.extend(artist.map(Action::OpenArtist));
    let painter = ui.painter();
    if let Some(album) = columns.album {
        let width = columns.time - album - 70.0;
        match Coll::album_of(track) {
            // Episodes show when they came out instead.
            None if let Some(episode) = track.episode() => {
                let date = release_date(&episode.published).unwrap_or_default();
                text_left(painter, pos2(album, cy), &date, ty::BODY, p.dim, width);
            }
            Some(coll) => {
                if text_link(ui, ("album-link", i), pos2(album, cy), &track.album, ty::BODY, p.dim, p.text, width).0 {
                    acts.push(Action::OpenCollection(coll));
                }
            }
            None => {
                text_left(painter, pos2(album, cy), &track.album, ty::BODY, p.dim, width);
            }
        }
    }
    painter.text(pos2(columns.time, cy), Align2::RIGHT_CENTER, mmss(track.duration as f64), ty::META.font(), sub_color);
    // Heart: always shown when liked, on hover otherwise (songs only).
    if !track.is_episode() && (liked || hovered) && heart_at(ui, pos2(columns.heart, cy), ("row", i), liked, 15.0).clicked() {
        acts.push(Action::ToggleLike(Likeable::Track(track.id.clone())));
    }

    if !available {
        return;
    }
    // Double-click a row, or click its number, to play from there.
    let number_edge = columns.art.map_or(columns.title, |(x, _)| x);
    let clicked_number = resp.clicked() && resp.interact_pointer_pos().is_some_and(|pt| pt.x < number_edge);
    if resp.double_clicked() || clicked_number {
        if current && clicked_number {
            acts.push(Action::Player(Cmd::Toggle));
        } else {
            acts.push(Action::PlayFrom { tracks: table.tracks.to_vec(), index: i, shuffle: false });
        }
    }
    let choice =
        if track.is_episode() { queue_menu(&resp).map(TrackMenu::Play) } else { track_menu(&resp, menu.playlists, menu.removable) };
    acts.extend(choice.map(|choice| Action::TrackMenu(Box::new(track.clone()), choice)));
}

/// Hover text for a track that can't be played (yet).
fn unavailable_note(track: &Track) -> String {
    match release_date(&track.available_from) {
        Some(date) => format!("Available from {date}"),
        None => "Not available to stream".into(),
    }
}

/// Column titles along the bottom of `head`, over a hairline.
pub fn column_captions(ui: &Ui, head: Rect, columns: &Columns, album_caption: &str) {
    let y = head.bottom() - 22.0;
    let (font, color) = (ty::OVERLINE.font(), colors().dim);
    let painter = ui.painter();
    painter.text(pos2(columns.index + 12.0, y), Align2::CENTER_CENTER, "#", font.clone(), color);
    painter.text(pos2(columns.title, y), Align2::LEFT_CENTER, "TITLE", font.clone(), color);
    if let Some(artist) = columns.artist {
        painter.text(pos2(artist, y), Align2::LEFT_CENTER, "ARTIST", font.clone(), color);
    }
    if let Some(album) = columns.album {
        painter.text(pos2(album, y), Align2::LEFT_CENTER, album_caption, font.clone(), color);
    }
    painter.text(pos2(columns.time, y), Align2::RIGHT_CENTER, "TIME", font, color);
    painter.hline(head.x_range(), head.bottom() - 4.0, egui::Stroke::new(1.0, colors().line));
}
