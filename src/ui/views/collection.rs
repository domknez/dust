//! Track lists: search results, Loved tracks and opened collections. A header
//! (artwork, title, Play/Shuffle) scrolls with a virtualised track table.

use crate::deezer::{ArtistRef, Item, Likeable, Playlist, Track, image_url};
use crate::player::{Cmd, State, Status};
use crate::ui::app::App;
use crate::ui::covers::Covers;
use crate::ui::format::{cover_url, mmss, release_date, total_clock};
use crate::ui::playlist_edit::{Dialog, Edit};
use crate::ui::state::{ArtistView, Coll, Source, View, playable_from};
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::views::home::{CARD_ROW_HEIGHT, CardPicks, card_row};
use crate::ui::widgets::{self, TrackMenu, text_left};
use eframe::egui::{self, Align2, Color32, CornerRadius, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

/// Search page: title and result count, then card rows, then the table captions.
const SEARCH_TOP: f32 = 64.0;
/// How long "Add to queue" reads "Added ✓".
const QUEUED_FLASH: std::time::Duration = std::time::Duration::from_secs(2);
const SEARCH_TRACKS_TITLE: f32 = 50.0;
const CAPTIONS: f32 = 32.0;
/// The tinted band behind a collection's artwork and title.
const HEADER_BAND: f32 = 236.0;
const COLLECTION_HEADER: f32 = HEADER_BAND + 12.0 + CAPTIONS;
/// "More by" column next to an album's tracks, and the width it needs.
const MORE_WIDTH: f32 = 260.0;
const MORE_MIN_WIDTH: f32 = 860.0;
const MORE_ROW: f32 = 50.0;
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
    /// A choice in a row's right-click menu.
    pub menu: Option<(usize, TrackMenu)>,
    /// The playing row's number was clicked: pause/resume.
    pub toggle: bool,
    pub artist: Option<ArtistRef>,
    /// An album name was clicked.
    pub album: Option<Coll>,
    /// A heart was clicked (track row or page header).
    pub like: Option<Likeable>,
    /// The header's "Add to queue": the whole list.
    pub queue_all: bool,
    /// A choice in the header's ⋯ menu.
    pub header_menu: Option<HeaderMenu>,
    pub cards: CardPicks,
}

/// X positions of the table columns, and the row height.
pub(super) struct Columns {
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
    pub(super) row: f32,
}

impl Columns {
    /// Lists mixing albums (playlists, Loved, search): artwork, title over artists,
    /// album.
    pub(super) fn list(left: f32, width: f32) -> Self {
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
    pub(super) fn album(left: f32, width: f32) -> Self {
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
    pub(super) fn popular(left: f32, width: f32) -> Self {
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

impl App {
    pub(in crate::ui) fn collection_page(&mut self, ui: &mut Ui, st: &Status) {
        let (playing_id, playing) = (st.track.as_ref().map(|t| t.id), st.state == State::Playing);
        let header_height = if self.browse.view == View::Search { self.search_header_height() } else { COLLECTION_HEADER };
        let count = self.browse.tracks.len();
        let album = self.showing_album();
        let mut picked = Picked::default();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            // An album gets its artist's other releases beside the tracks when there's room.
            let more =
                self.browse.album_info.as_ref().filter(|i| album && !i.more.is_empty() && width >= MORE_MIN_WIDTH).map(|i| i.more.clone());
            let table_width = if more.is_some() { width - MORE_WIDTH - 32.0 } else { width };
            let origin = ui.max_rect().min;
            let columns = if album { Columns::album(origin.x, table_width) } else { Columns::list(origin.x, table_width) };
            let more_height = more.as_ref().map_or(0.0, |m| 32.0 + m.len() as f32 * MORE_ROW);
            ui.set_height(header_height + (count as f32 * columns.row).max(more_height) + 24.0);
            let head = Rect::from_min_size(origin, vec2(width, header_height));
            ui.scope_builder(UiBuilder::new().max_rect(head), |ui| {
                if self.browse.view == View::Search {
                    self.search_header(ui, head, st, &mut picked.cards);
                } else {
                    self.collection_header(ui, head, &mut picked);
                }
                if count > 0 {
                    let table_head = Rect::from_min_size(head.min, vec2(table_width, head.height()));
                    column_captions(ui, table_head, &columns, if self.showing_podcast() { "PUBLISHED" } else { "ALBUM" });
                }
            });
            if let Some(more) = &more {
                let column =
                    Rect::from_min_size(origin + vec2(width - MORE_WIDTH, header_height - CAPTIONS + 6.0), vec2(MORE_WIDTH, more_height));
                let by =
                    self.album_artist().map_or_else(|| "MORE BY THIS ARTIST".to_string(), |a| format!("MORE BY {}", a.name.to_uppercase()));
                if let Some(coll) = more_releases(ui, &mut self.covers, column, &by, more) {
                    picked.album = Some(coll);
                }
            }

            let below_header = Rect::from_min_size(origin + vec2(0.0, header_height + 24.0), vec2(width, 40.0));
            if self.browse.loading_tracks() {
                ui.put(below_header, egui::Spinner::new().size(22.0));
                return;
            }
            if let Some(e) = self.browse.tracks_error() {
                text_left(ui.painter(), below_header.min, e, ty::TITLE, colors().danger, width);
                return;
            }
            // Only rows in the viewport are laid out and painted.
            let row = columns.row;
            let first = ((viewport.top() - header_height) / row).floor().max(0.0) as usize;
            let last = (((viewport.bottom() - header_height) / row).ceil().max(0.0) as usize).min(count);
            let own = self.own_playlists();
            let menu = RowMenu { playlists: &own, removable: self.showing_own_playlist().is_some() };
            for i in first..last {
                let rect = Rect::from_min_size(origin + vec2(0.0, header_height + i as f32 * row), vec2(table_width, row));
                let liked = self.library.likes.likes.tracks.contains(&self.browse.tracks[i].id);
                track_row(
                    ui,
                    &mut self.covers,
                    &self.browse.tracks[i],
                    i,
                    rect,
                    &columns,
                    (playing_id, playing),
                    liked,
                    &menu,
                    &mut picked,
                );
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
        if picked.queue_all && !self.browse.tracks.is_empty() {
            let playable: Vec<Track> = self.browse.tracks.iter().filter(|t| t.available).cloned().collect();
            self.player.send(Cmd::Enqueue(playable));
            self.queued_flash = Some(std::time::Instant::now());
        }
        if let Some((i, choice)) = picked.menu {
            let track = self.browse.tracks[i].clone();
            self.track_menu_choice(&ctx, track, choice);
        }
        match picked.header_menu {
            Some(HeaderMenu::AddAll(TrackMenu::AddTo(playlist))) => {
                self.edit_playlist(&ctx, Edit::Add { playlist, tracks: self.browse.tracks.clone() });
            }
            Some(HeaderMenu::AddAll(_)) => self.new_playlist(self.browse.tracks.iter().filter(|t| t.available).cloned().collect()),
            Some(HeaderMenu::Rename) => {
                if let Some(playlist) = self.showing_own_playlist() {
                    self.library.editing.dialog = Some(Dialog::Rename { title: playlist.title.clone(), playlist });
                }
            }
            Some(HeaderMenu::Delete) => {
                if let Some(playlist) = self.showing_own_playlist() {
                    self.library.editing.dialog = Some(Dialog::Delete { playlist });
                }
            }
            None => {}
        }
        if let Some((i, shuffle)) = picked.play
            && let Some((queue, i)) = playable_from(&self.browse.tracks, i)
        {
            let i = if shuffle {
                self.set_shuffle(true);
                fastrand::usize(..queue.len())
            } else {
                i
            };
            self.player.send(Cmd::Play(queue, i));
        }
    }

    fn header(&self) -> Header {
        match &self.browse.view {
            View::Loved => Header {
                kind: "COLLECTION",
                title: "Loved tracks".into(),
                tile: Some((Icon::Heart, colors().tile_loved)),
                ..Default::default()
            },
            View::Collection(coll) => Header {
                kind: coll.kind,
                title: coll.title.clone(),
                subtitle: coll.subtitle.clone(),
                art: coll.picture.as_ref().map(|(k, m)| image_url(k, m, metrics::HEADER_PX)),
                tile: None,
                round: coll.round(),
            },
            View::Home | View::Search | View::Playlists | View::Artists | View::Albums | View::Podcasts | View::Artist(_) => {
                Header::default()
            }
        }
    }

    fn search_header_height(&self) -> f32 {
        let rows = self.browse.search_sections.len() as f32;
        let tracks_title = if rows > 0.0 { SEARCH_TRACKS_TITLE } else { 0.0 };
        SEARCH_TOP + rows * CARD_ROW_HEIGHT + tracks_title + CAPTIONS
    }

    fn search_header(&mut self, ui: &mut Ui, head: Rect, st: &Status, picked: &mut CardPicks) {
        let p = colors();
        let title = if self.browse.searched.is_empty() { "Search".to_string() } else { format!("“{}”", self.browse.searched) };
        text_left(ui.painter(), head.min + vec2(0.0, 22.0), &title, ty::PAGE_TITLE, p.text, head.width());
        let counts: Vec<String> = std::iter::once((self.browse.tracks.len(), "tracks"))
            .chain(self.browse.search_sections.iter().map(|s| (s.items.len(), s.title.as_str())))
            .filter(|(n, _)| *n > 0)
            .map(|(n, what)| format!("{n} {}", what.to_lowercase()))
            .collect();
        text_left(ui.painter(), head.min + vec2(0.0, 54.0), &counts.join(" · "), ty::BODY, p.dim, head.width());
        if self.browse.search_sections.is_empty() {
            return;
        }
        let rows = Rect::from_min_size(
            head.min + vec2(0.0, SEARCH_TOP),
            vec2(head.width(), self.browse.search_sections.len() as f32 * CARD_ROW_HEIGHT),
        );
        ui.scope_builder(UiBuilder::new().max_rect(rows), |ui| {
            for (i, section) in self.browse.search_sections.iter().enumerate() {
                card_row(ui, &mut self.covers, ("search-row", i), &section.title, &section.items, st, picked);
            }
        });
        if !self.browse.tracks.is_empty() {
            let y = rows.bottom() + 22.0 + 14.0;
            text_left(ui.painter(), pos2(head.left(), y), "TRACKS", ty::OVERLINE, p.dim, head.width());
        }
    }

    /// The artist of an album page when every track lists them (not compilations).
    fn album_artist(&self) -> Option<ArtistRef> {
        let View::Collection(coll) = &self.browse.view else { return None };
        let Source::Album(_) = coll.source else { return None };
        let main = self.browse.tracks.first()?.artists.first()?.clone();
        self.browse.tracks.iter().all(|t| t.artists.iter().any(|a| a.id == main.id)).then_some(main)
    }

    fn collection_header(&mut self, ui: &mut Ui, head: Rect, picked: &mut Picked) {
        let p = colors();
        let Header { kind, title, subtitle, art, tile, round } = self.header();
        header_band(ui.painter(), Rect::from_min_size(head.min, vec2(head.width(), HEADER_BAND)));
        let art_rect = Rect::from_min_size(head.min + vec2(0.0, 18.0), Vec2::splat(metrics::HEADER_ART));
        let corner = if round { (metrics::HEADER_ART / 2.0) as u8 } else { radius::CONTROL };
        match (art, tile) {
            (Some(url), _) => widgets::cover(ui, &mut self.covers, Some(&url), art_rect, corner),
            (None, Some((icon, color))) => widgets::tile(ui, art_rect, icon, color, radius::CONTROL),
            _ => widgets::cover(ui, &mut self.covers, None, art_rect, radius::CONTROL),
        }
        let x = art_rect.right() + 28.0;
        let width = head.right() - x;
        let count = self.browse.tracks.len();
        let podcast = self.showing_podcast();

        // ALBUM  2021 · 11 TRACKS · 51:52
        let tag = widgets::tag(ui.painter(), pos2(x, head.top() + 46.0), kind);
        let year = self.browse.album_info.as_ref().map(|i| i.year.clone()).filter(|y| !y.is_empty());
        let stats =
            (count > 0).then(|| format!("{count} {} · {}", if podcast { "EPISODES" } else { "TRACKS" }, total_clock(&self.browse.tracks)));
        let meta = [year, stats].into_iter().flatten().collect::<Vec<_>>().join(" · ");
        text_left(ui.painter(), pos2(tag.right() + 10.0, tag.center().y), &meta, ty::META, p.dim, x + width - tag.right() - 10.0);
        text_left(ui.painter(), pos2(x, head.top() + 102.0), &title, ty::HERO, p.text, width);

        // The album's artist (linked) and label, or the list's subtitle.
        let line = pos2(x, head.top() + 150.0);
        let label = self.browse.album_info.as_ref().map(|i| i.label.clone()).filter(|l| !l.is_empty());
        match self.album_artist() {
            Some(artist) => {
                let (clicked, right) = widgets::artist_links(ui, line, &[artist], "", ty::ITEM.strong(), p.text, p.text, width);
                picked.artist = clicked;
                if let Some(label) = label {
                    text_left(ui.painter(), pos2(right, line.y), &format!(" · {label}"), ty::ITEM, p.dim, x + width - right);
                }
            }
            None => {
                let text = [Some(subtitle), label].into_iter().flatten().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" · ");
                text_left(ui.painter(), line, &text, ty::ITEM, p.dim, width);
            }
        }

        let buttons = Rect::from_min_size(pos2(x, head.top() + 174.0), vec2(width, 44.0));
        ui.scope_builder(UiBuilder::new().max_rect(buttons).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            if count > 0 && widgets::pill(ui, "Play", Some(Icon::Play), true).clicked() {
                picked.play = Some((0, false));
            }
            if count > 1 && !podcast && widgets::pill(ui, "Shuffle", None, false).clicked() {
                picked.play = Some((0, true));
            }
            if count > 0 {
                // Briefly confirms, since the queue panel may be closed.
                let just_added = self.queued_flash.is_some_and(|t| t.elapsed() < QUEUED_FLASH);
                let (label, icon) = if just_added { ("Added", Some(Icon::Check)) } else { ("Add to queue", None) };
                let hint = format!("Add every {} to the end of the queue", if podcast { "episode" } else { "track" });
                if widgets::pill(ui, label, icon, false).on_hover_text(hint).clicked() {
                    picked.queue_all = true;
                }
                if just_added {
                    ui.ctx().request_repaint_after(QUEUED_FLASH);
                }
            }
            // Albums can be liked from their header.
            if let View::Collection(coll) = &self.browse.view
                && let Source::Album(id) = &coll.source
            {
                let what = Likeable::Album(id.clone());
                if heart_square(ui, self.library.likes.likes.contains(&what)).clicked() {
                    picked.like = Some(what);
                }
            }
            // Episodes can't go into playlists.
            if count > 0 && !podcast {
                let own = self.own_playlists();
                let editable = self.showing_own_playlist().is_some();
                let more = widgets::icon_button(ui, Icon::More, 18.0, colors().dim).on_hover_text("More");
                egui::Popup::menu(&more).width(220.0).show(|ui| {
                    if let Some(choice) = widgets::add_to_playlist(ui, &own) {
                        picked.header_menu = Some(HeaderMenu::AddAll(choice));
                    }
                    if editable {
                        widgets::divider(ui);
                        if widgets::menu_row(ui, None, "Rename…", None, false, colors().text).clicked() {
                            picked.header_menu = Some(HeaderMenu::Rename);
                        }
                        if widgets::menu_row(ui, Some(Icon::Close), "Delete playlist…", None, false, colors().danger).clicked() {
                            picked.header_menu = Some(HeaderMenu::Delete);
                        }
                    }
                    if picked.header_menu.is_some() {
                        ui.close();
                    }
                });
            }
        });
    }
}

/// The surface-to-background band behind a page header, with a hairline under it.
pub(super) fn header_band(painter: &egui::Painter, band: Rect) {
    let p = colors();
    painter.rect_filled(band, CornerRadius::ZERO, p.surface);
    widgets::fade_to(painter, band, p.bg);
    painter.hline(band.x_range(), band.bottom(), egui::Stroke::new(1.0, p.line));
}

/// Square outlined heart button (album headers).
fn heart_square(ui: &mut Ui, liked: bool) -> egui::Response {
    let p = colors();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
    let corner = CornerRadius::same(radius::CONTROL);
    if resp.hovered() {
        ui.painter().rect_filled(rect, corner, p.raised);
    }
    ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    let (icon, color) = if liked { (Icon::Heart, p.accent) } else { (Icon::HeartOutline, if resp.hovered() { p.text } else { p.dim }) };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(15.0)), icon, color);
    let hint = if liked { "Remove from your likes" } else { "Like" };
    resp.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// "MORE BY …": the artist's other releases (cover, title, year). Returns the one
/// clicked.
fn more_releases(ui: &mut Ui, covers: &mut Covers, column: Rect, caption: &str, items: &[Item]) -> Option<Coll> {
    let p = colors();
    text_left(ui.painter(), pos2(column.left(), column.top() + 10.0), caption, ty::OVERLINE, p.dim, column.width());
    let mut open = None;
    for (k, item) in items.iter().enumerate() {
        let rect =
            Rect::from_min_size(pos2(column.left(), column.top() + 28.0 + k as f32 * MORE_ROW), vec2(column.width(), MORE_ROW - 4.0));
        let resp = ui.interact(rect, ui.id().with(("more-by", k)), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
        }
        let art = Rect::from_min_size(pos2(rect.left() + 2.0, rect.center().y - 21.0), Vec2::splat(42.0));
        let url = item.picture.as_ref().map(|(k, m)| image_url(k, m, metrics::THUMB_PX));
        widgets::cover(ui, covers, url.as_deref(), art, radius::THUMB);
        let x = art.right() + 10.0;
        let width = rect.right() - x - 4.0;
        text_left(ui.painter(), pos2(x, rect.center().y - 8.0), &item.title, ty::ITEM, p.text, width);
        text_left(ui.painter(), pos2(x, rect.center().y + 9.0), &item.subtitle, ty::META, p.dim, width);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            open = Coll::from_item(item);
        }
    }
    open
}

/// The page header's ⋯ menu.
pub(super) enum HeaderMenu {
    /// Add every playable track to a playlist (or a new one).
    AddAll(TrackMenu),
    Rename,
    Delete,
}

/// What a row's right-click menu offers beyond queueing.
pub(super) struct RowMenu<'a> {
    /// Own playlists for "Add to playlist".
    pub playlists: &'a [Playlist],
    /// Showing one of the own playlists: rows can be removed from it.
    pub removable: bool,
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
    menu: &RowMenu,
    picked: &mut Picked,
) {
    let p = colors();
    let resp = ui.interact(rect, ui.id().with(("row", i)), Sense::click());
    let current = playing_id == Some(track.id);
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
        widgets::equalizer(ui.painter(), index, playing);
    } else {
        ui.painter().text(index.center(), Align2::CENTER_CENTER, format!("{:02}", i + 1), ty::META.font(), p.faint);
    }
    if let Some((x, size)) = columns.art {
        let art = Rect::from_min_size(pos2(x, cy - size / 2.0), Vec2::splat(size));
        widgets::cover(ui, covers, cover_url(track, metrics::THUMB_PX).as_deref(), art, radius::THUMB);
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
        widgets::artist_links(ui, at, &track.artists, &track.artist, ty::SECONDARY, sub_color, p.text, title_width).0
    } else {
        text_left(ui.painter(), pos2(columns.title, cy), &track.title, ty::TITLE, title_color, title_width);
        columns.artist.and_then(|x| {
            let width = columns.heart - 24.0 - x;
            widgets::artist_links(ui, pos2(x, cy), &track.artists, &track.artist, ty::BODY, sub_color, p.text, width).0
        })
    };
    if artist.is_some() {
        picked.artist = artist;
    }
    let painter = ui.painter();
    if let Some(album) = columns.album {
        let width = columns.time - album - 70.0;
        match Coll::album_of(track) {
            // Episodes show when they came out instead.
            None if let Some(episode) = &track.episode => {
                let date = release_date(&episode.published).unwrap_or_default();
                text_left(painter, pos2(album, cy), &date, ty::BODY, p.dim, width);
            }
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
    painter.text(pos2(columns.time, cy), Align2::RIGHT_CENTER, mmss(track.duration as f64), ty::META.font(), sub_color);
    // Heart: always shown when liked, on hover otherwise (songs only).
    if !track.is_episode() && (liked || hovered) && widgets::heart_at(ui, pos2(columns.heart, cy), ("row", i), liked, 15.0).clicked() {
        picked.like = Some(Likeable::Track(track.id));
    }

    if !available {
        return;
    }
    // Double-click a row, or click its number, to play from there.
    let number_edge = columns.art.map_or(columns.title, |(x, _)| x);
    let clicked_number = resp.clicked() && resp.interact_pointer_pos().is_some_and(|pt| pt.x < number_edge);
    if resp.double_clicked() || clicked_number {
        if current && clicked_number {
            picked.toggle = true;
        } else {
            picked.play = Some((i, false));
        }
    }
    let choice = if track.is_episode() {
        widgets::queue_menu(&resp).map(TrackMenu::Play)
    } else {
        widgets::track_menu(&resp, menu.playlists, menu.removable)
    };
    if let Some(choice) = choice {
        picked.menu = Some((i, choice));
    }
}

/// Hover text for a track that can't be played (yet).
fn unavailable_note(track: &Track) -> String {
    match release_date(&track.available_from) {
        Some(date) => format!("Available from {date}"),
        None => "Not available to stream".into(),
    }
}

impl App {
    /// The own playlist on screen, if that's what is shown.
    pub(in crate::ui) fn showing_own_playlist(&self) -> Option<Playlist> {
        let View::Collection(coll) = &self.browse.view else { return None };
        let Source::Playlist(id) = coll.source else { return None };
        self.library.playlists.iter().find(|p| p.id == id && p.owned).cloned()
    }

    /// Whether an album's tracks are on screen.
    fn showing_album(&self) -> bool {
        matches!(&self.browse.view, View::Collection(coll) if matches!(coll.source, Source::Album(_)))
    }

    /// Whether a podcast's episodes are on screen.
    fn showing_podcast(&self) -> bool {
        matches!(&self.browse.view, View::Collection(coll) if matches!(coll.source, Source::Show(_)))
    }

    /// Act on a track's right-click menu.
    pub(in crate::ui) fn track_menu_choice(&mut self, ctx: &egui::Context, track: Track, choice: TrackMenu) {
        match choice {
            TrackMenu::Play(mode) => self.player.send(mode.command(vec![track])),
            TrackMenu::AddTo(playlist) => self.edit_playlist(ctx, Edit::Add { playlist, tracks: vec![track] }),
            TrackMenu::NewPlaylist => self.new_playlist(vec![track]),
            TrackMenu::Remove => {
                if let Some(playlist) = self.showing_own_playlist() {
                    self.edit_playlist(ctx, Edit::Remove { playlist: playlist.id, tracks: vec![track.id] });
                }
            }
        }
    }
}

fn column_captions(ui: &Ui, head: Rect, columns: &Columns, album_caption: &str) {
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
