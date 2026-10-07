//! Track lists: search results, Loved tracks and opened collections. A header
//! (artwork, title, Play/Shuffle) scrolls with a virtualised track table.

use crate::deezer::{ArtistRef, Likeable, Track, image_url};
use crate::player::{Cmd, State, Status};
use crate::ui::app::App;
use crate::ui::covers::Covers;
use crate::ui::format::{cover_url, mmss, total_duration};
use crate::ui::state::{ArtistView, Coll, PlayMode, Source, View};
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::views::home::{CARD_ROW_HEIGHT, CardPicks, card_row};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{self, Align2, Color32, CornerRadius, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

/// Search page: title and result count, then card rows, then the table captions.
const SEARCH_TOP: f32 = 64.0;
const SEARCH_TRACKS_TITLE: f32 = 50.0;
const CAPTIONS: f32 = 32.0;
const COLLECTION_HEADER: f32 = 268.0;
/// Below this width the album column is hidden.
const ALBUM_COLUMN_MIN_WIDTH: f32 = 640.0;

/// Collection header content.
#[derive(Default)]
struct Header {
    kind: &'static str,
    title: String,
    subtitle: String,
    art: Option<String>,
    /// Placeholder tile when there is no artwork.
    tile: Option<(Icon, Color32)>,
    round: bool,
}

/// Actions picked in the table this frame.
#[derive(Default)]
pub(super) struct Picked {
    /// Play from this row (shuffled or not).
    pub play: Option<(usize, bool)>,
    pub queue: Option<(usize, PlayMode)>,
    /// The playing row's number was clicked: pause/resume.
    pub toggle: bool,
    pub artist: Option<ArtistRef>,
    /// An album name was clicked.
    pub album: Option<Coll>,
    /// A heart was clicked (track row or page header).
    pub like: Option<Likeable>,
    pub cards: CardPicks,
}

/// X positions of the table columns.
pub(super) struct Columns {
    index: f32,
    heart: f32,
    art: f32,
    title: f32,
    album: Option<f32>,
    time: f32,
}

impl Columns {
    pub(super) fn new(left: f32, width: f32) -> Self {
        Columns {
            index: left + 8.0,
            heart: left + width - 78.0,
            art: left + 48.0,
            title: left + 104.0,
            album: (width > ALBUM_COLUMN_MIN_WIDTH).then_some(left + width * 0.58),
            time: left + width - 16.0,
        }
    }
}

impl App {
    pub(in crate::ui) fn collection_page(&mut self, ui: &mut Ui, st: &Status) {
        let (playing_id, playing) = (st.track.as_ref().map(|t| t.id), st.state == State::Playing);
        let header_height = if self.view == View::Search { self.search_header_height() } else { COLLECTION_HEADER };
        let count = self.tracks.len();
        let mut picked = Picked::default();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            ui.set_height(header_height + count as f32 * metrics::TRACK_ROW + 24.0);
            let origin = ui.max_rect().min;
            let head = Rect::from_min_size(origin, vec2(width, header_height));
            ui.scope_builder(UiBuilder::new().max_rect(head), |ui| {
                if self.view == View::Search {
                    self.search_header(ui, head, st, &mut picked.cards);
                } else {
                    self.collection_header(ui, head, &mut picked);
                }
                if count > 0 {
                    column_captions(ui, head);
                }
            });

            let below_header = Rect::from_min_size(origin + vec2(0.0, header_height + 24.0), vec2(width, 40.0));
            if self.loading_tracks() {
                ui.put(below_header, egui::Spinner::new().size(22.0));
                return;
            }
            if let Some(e) = &self.list_error {
                text_left(ui.painter(), below_header.min, e, ty::TITLE, colors().danger, width);
                return;
            }
            // Only rows in the viewport are laid out and painted.
            let first = ((viewport.top() - header_height) / metrics::TRACK_ROW).floor().max(0.0) as usize;
            let last = (((viewport.bottom() - header_height) / metrics::TRACK_ROW).ceil().max(0.0) as usize).min(count);
            let columns = Columns::new(origin.x, width);
            for i in first..last {
                let rect =
                    Rect::from_min_size(origin + vec2(0.0, header_height + i as f32 * metrics::TRACK_ROW), vec2(width, metrics::TRACK_ROW));
                let liked = self.likes.likes.tracks.contains(&self.tracks[i].id);
                track_row(ui, &mut self.covers, &self.tracks[i], i, rect, &columns, (playing_id, playing), liked, &mut picked);
            }
        });

        let ctx = ui.ctx().clone();
        self.apply_card_picks(&ctx, picked.cards);
        if picked.toggle {
            self.player.send(Cmd::Toggle);
        }
        if let Some(artist) = picked.artist {
            self.open_artist(&ctx, ArtistView::from_ref(&artist));
        }
        if let Some(album) = picked.album {
            self.open_collection(&ctx, album);
        }
        if let Some(what) = picked.like {
            self.toggle_like(&ctx, what);
        }
        if let Some((i, mode)) = picked.queue {
            self.player.send(mode.command(vec![self.tracks[i].clone()]));
        }
        if let Some((i, shuffle)) = picked.play {
            let mut queue = self.tracks.clone();
            if shuffle {
                fastrand::shuffle(&mut queue);
            }
            self.player.send(Cmd::Play(queue, i));
        }
    }

    fn header(&self) -> Header {
        match &self.view {
            View::Loved => {
                Header { kind: "COLLECTION", title: "Tracks".into(), tile: Some((Icon::Heart, colors().tile_loved)), ..Default::default() }
            }
            View::Collection(coll) => Header {
                kind: coll.kind,
                title: coll.title.clone(),
                subtitle: coll.subtitle.clone(),
                art: coll.picture.as_ref().map(|(k, m)| image_url(k, m, metrics::HEADER_PX)),
                tile: None,
                round: coll.round(),
            },
            View::Home | View::Search | View::Playlists | View::Artists | View::Albums | View::Artist(_) => Header::default(),
        }
    }

    fn search_header_height(&self) -> f32 {
        let rows = self.search_sections.len() as f32;
        let tracks_title = if rows > 0.0 { SEARCH_TRACKS_TITLE } else { 0.0 };
        SEARCH_TOP + rows * CARD_ROW_HEIGHT + tracks_title + CAPTIONS
    }

    fn search_header(&mut self, ui: &mut Ui, head: Rect, st: &Status, picked: &mut CardPicks) {
        let p = colors();
        let title = if self.searched.is_empty() { "Search".to_string() } else { format!("“{}”", self.searched) };
        text_left(ui.painter(), head.min + vec2(0.0, 22.0), &title, ty::PAGE_TITLE, p.text, head.width());
        let counts: Vec<String> = std::iter::once((self.tracks.len(), "tracks"))
            .chain(self.search_sections.iter().map(|s| (s.items.len(), s.title.as_str())))
            .filter(|(n, _)| *n > 0)
            .map(|(n, what)| format!("{n} {}", what.to_lowercase()))
            .collect();
        text_left(ui.painter(), head.min + vec2(0.0, 54.0), &counts.join(" · "), ty::BODY, p.dim, head.width());
        if self.search_sections.is_empty() {
            return;
        }
        let rows =
            Rect::from_min_size(head.min + vec2(0.0, SEARCH_TOP), vec2(head.width(), self.search_sections.len() as f32 * CARD_ROW_HEIGHT));
        ui.scope_builder(UiBuilder::new().max_rect(rows), |ui| {
            for (i, section) in self.search_sections.iter().enumerate() {
                card_row(ui, &mut self.covers, ("search-row", i), &section.title, &section.items, st, picked);
            }
        });
        if !self.tracks.is_empty() {
            let y = rows.bottom() + 22.0 + 14.0;
            text_left(ui.painter(), pos2(head.left(), y), "Tracks", ty::SECTION_TITLE, p.text, head.width());
        }
    }

    /// The artist of an album page when every track lists them (not compilations).
    fn album_artist(&self) -> Option<ArtistRef> {
        let View::Collection(coll) = &self.view else { return None };
        let Source::Album(_) = coll.source else { return None };
        let main = self.tracks.first()?.artists.first()?.clone();
        self.tracks.iter().all(|t| t.artists.iter().any(|a| a.id == main.id)).then_some(main)
    }

    fn collection_header(&mut self, ui: &mut Ui, head: Rect, picked: &mut Picked) {
        let p = colors();
        let Header { kind, title, subtitle, art, tile, round } = self.header();
        let art_rect = Rect::from_min_size(head.min, Vec2::splat(metrics::HEADER_ART));
        let corner = if round { (metrics::HEADER_ART / 2.0) as u8 } else { radius::CARD };
        match (art, tile) {
            (Some(url), _) => widgets::cover(ui, &mut self.covers, Some(&url), art_rect, corner),
            (None, Some((icon, color))) => widgets::tile(ui, art_rect, icon, color, radius::CARD),
            _ => widgets::cover(ui, &mut self.covers, None, art_rect, radius::CARD),
        }
        let x = art_rect.right() + 28.0;
        let width = head.right() - x;
        text_left(ui.painter(), pos2(x, head.top() + 40.0), kind, ty::CAPTION.strong(), p.dim, width);
        text_left(ui.painter(), pos2(x, head.top() + 82.0), &title, ty::HERO, p.text, width);
        let count = self.tracks.len();
        let stats = (count > 0).then(|| format!("{count} tracks · {}", total_duration(&self.tracks)));
        let meta_pos = pos2(x, head.top() + 124.0);
        match self.album_artist() {
            // Album by one artist: their name links to the artist page.
            Some(artist) => {
                let (clicked, right) = widgets::artist_links(ui, meta_pos, &[artist], "", ty::ITEM, p.dim, p.text, width);
                picked.artist = clicked;
                if let Some(stats) = stats {
                    text_left(ui.painter(), pos2(right, meta_pos.y), &format!("  ·  {stats}"), ty::ITEM, p.dim, x + width - right);
                }
            }
            None => {
                let meta = match (subtitle.is_empty(), stats) {
                    (true, Some(stats)) => stats,
                    (false, Some(stats)) => format!("{subtitle}  ·  {stats}"),
                    (_, None) => subtitle,
                };
                text_left(ui.painter(), meta_pos, &meta, ty::ITEM, p.dim, width);
            }
        }
        let buttons = Rect::from_min_size(pos2(x, head.top() + 152.0), vec2(width, 44.0));
        ui.scope_builder(UiBuilder::new().max_rect(buttons).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            if count > 0 && widgets::pill(ui, "Play", Some(Icon::Play), true).clicked() {
                picked.play = Some((0, false));
            }
            if count > 1 && widgets::pill(ui, "Shuffle", Some(Icon::Shuffle), false).clicked() {
                picked.play = Some((0, true));
            }
            // Albums can be liked from their header.
            if let View::Collection(coll) = &self.view
                && let Source::Album(id) = &coll.source
            {
                let what = Likeable::Album(id.clone());
                if widgets::heart_button(ui, self.likes.likes.contains(&what), 20.0).clicked() {
                    picked.like = Some(what);
                }
            }
        });
    }
}

/// One row of a track table: number (or equalizer / play icon), artwork, title,
/// linked artists, album, duration. Clicks and menu choices go into `picked`.
#[allow(clippy::too_many_arguments)]
pub(super) fn track_row(
    ui: &mut Ui,
    covers: &mut Covers,
    track: &Track,
    i: usize,
    rect: Rect,
    columns: &Columns,
    (playing_id, playing): (Option<u64>, bool),
    liked: bool,
    picked: &mut Picked,
) {
    let p = colors();
    let resp = ui.interact(rect, ui.id().with(("row", i)), Sense::click());
    let current = playing_id == Some(track.id);
    let hovered = resp.hovered();
    if hovered {
        ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
    }
    let cy = rect.center().y;
    let index = Rect::from_center_size(pos2(columns.index + 14.0, cy), Vec2::splat(14.0));
    if hovered {
        icons::paint(ui.painter(), index, if current && playing { Icon::Pause } else { Icon::Play }, p.text);
    } else if current {
        widgets::equalizer(ui.painter(), index, playing);
    } else {
        ui.painter().text(index.center(), Align2::CENTER_CENTER, (i + 1).to_string(), ty::BODY.font(), p.faint);
    }
    let art = Rect::from_min_size(pos2(columns.art, cy - metrics::TRACK_ART / 2.0), Vec2::splat(metrics::TRACK_ART));
    widgets::cover(ui, covers, cover_url(track, metrics::THUMB_PX).as_deref(), art, radius::THUMB);
    let title_width = columns.album.unwrap_or(columns.time - 60.0) - columns.title - 16.0;
    text_left(ui.painter(), pos2(columns.title, cy - 9.0), &track.title, ty::TITLE, if current { p.accent } else { p.text }, title_width);
    let (artist, _) =
        widgets::artist_links(ui, pos2(columns.title, cy + 10.0), &track.artists, &track.artist, ty::SECONDARY, p.dim, p.text, title_width);
    if artist.is_some() {
        picked.artist = artist;
    }
    let painter = ui.painter();
    if let Some(album) = columns.album {
        let width = columns.time - album - 70.0;
        match Coll::album_of(track) {
            Some(coll) => {
                if widgets::text_link(ui, ("album-link", i), pos2(album, cy), &track.album, ty::BODY, p.dim, p.text, width).0 {
                    picked.album = Some(coll);
                }
            }
            None => {
                text_left(painter, pos2(album, cy), &track.album, ty::BODY, p.dim, width);
            }
        }
    }
    painter.text(pos2(columns.time, cy), Align2::RIGHT_CENTER, mmss(track.duration as f64), ty::BODY.font(), p.dim);
    // Heart: always shown when liked, on hover otherwise.
    if liked || hovered {
        let heart = Rect::from_center_size(pos2(columns.heart, cy), Vec2::splat(28.0));
        let resp = ui.scope_builder(UiBuilder::new().max_rect(heart), |ui| widgets::heart_button(ui, liked, 15.0)).inner;
        if resp.clicked() {
            picked.like = Some(Likeable::Track(track.id));
        }
    }

    // Double-click a row, or click its number, to play from there.
    let clicked_number = resp.clicked() && resp.interact_pointer_pos().is_some_and(|pt| pt.x < columns.art);
    if resp.double_clicked() || clicked_number {
        if current && clicked_number {
            picked.toggle = true;
        } else {
            picked.play = Some((i, false));
        }
    }
    if let Some(mode) = widgets::queue_menu(&resp) {
        picked.queue = Some((i, mode));
    }
}

fn column_captions(ui: &Ui, head: Rect) {
    let y = head.bottom() - 22.0;
    let columns = Columns::new(head.left(), head.width());
    let (font, color) = (ty::OVERLINE.font(), colors().faint);
    let painter = ui.painter();
    painter.text(pos2(columns.index + 14.0, y), Align2::CENTER_CENTER, "#", font.clone(), color);
    painter.text(pos2(columns.title, y), Align2::LEFT_CENTER, "TITLE", font.clone(), color);
    if let Some(album) = columns.album {
        painter.text(pos2(album, y), Align2::LEFT_CENTER, "ALBUM", font.clone(), color);
    }
    painter.text(pos2(columns.time, y), Align2::RIGHT_CENTER, "TIME", font, color);
    painter.hline(head.x_range(), head.bottom() - 4.0, egui::Stroke::new(1.0, colors().line));
}
