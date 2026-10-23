//! Track lists: search results, Loved tracks and opened collections. A header
//! (artwork, title, Play/Shuffle) scrolls with a virtualised track table.

use crate::model::{Art, ArtistRef, Item, Likeable, Playlist, TrackId};
use crate::player::{State, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::covers::Covers;
use crate::ui::format::total_clock;
use crate::ui::playlist_edit::{Dialog, Edit};
use crate::ui::state::{Coll, CollSource, View, playable};
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::views::home::{CARD_ROW_HEIGHT, card_row};
use crate::ui::widgets::{self, Columns, RowMenu, Table, TrackMenu, column_captions, text_left};
use eframe::egui::{self, Color32, CornerRadius, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

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

/// Collection header content.
#[derive(Default)]
struct Header {
    kind: &'static str,
    title: String,
    subtitle: String,
    art: Option<Art>,
    /// Placeholder tile when there is no artwork.
    tile: Option<(Icon, Color32)>,
    round: bool,
}

impl App {
    pub(in crate::ui) fn collection_page(&mut self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        let (playing_id, playing) = (st.track.as_ref().map(|t| &t.id), st.state == State::Playing);
        let header_height = if self.browse.view == View::Search { self.search_header_height() } else { COLLECTION_HEADER };
        let count = self.browse.tracks.len();
        let album = self.showing_album();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            // An album gets its artist's other releases beside the tracks when there's room.
            let more =
                self.browse.album_info.as_ref().filter(|i| album && !i.more.is_empty() && width >= MORE_MIN_WIDTH).map(|i| i.more.len());
            let table_width = if more.is_some() { width - MORE_WIDTH - 32.0 } else { width };
            let origin = ui.max_rect().min;
            let columns = if album { Columns::album(origin.x, table_width) } else { Columns::list(origin.x, table_width) };
            let more_height = more.map_or(0.0, |count| 32.0 + count as f32 * MORE_ROW);
            ui.set_height(header_height + (count as f32 * columns.row).max(more_height) + 24.0);
            let head = Rect::from_min_size(origin, vec2(width, header_height));
            ui.scope_builder(UiBuilder::new().max_rect(head), |ui| {
                if self.browse.view == View::Search {
                    self.search_header(ui, head, st, acts);
                } else {
                    self.collection_header(ui, head, acts);
                }
                if count > 0 {
                    let table_head = Rect::from_min_size(head.min, vec2(table_width, head.height()));
                    column_captions(ui, table_head, &columns, if self.showing_podcast() { "PUBLISHED" } else { "ALBUM" });
                }
            });
            if more.is_some() {
                let column =
                    Rect::from_min_size(origin + vec2(width - MORE_WIDTH, header_height - CAPTIONS + 6.0), vec2(MORE_WIDTH, more_height));
                let by =
                    self.album_artist().map_or_else(|| "MORE BY THIS ARTIST".to_string(), |a| format!("MORE BY {}", a.name.to_uppercase()));
                let items = self.browse.album_info.as_ref().map_or(&[][..], |i| &i.more[..]);
                acts.extend(more_releases(ui, &mut self.covers, column, &by, items).map(Action::OpenCollection));
            }

            let (loading, error) = (self.browse.loading_tracks(), self.browse.tracks_error());
            if loading || error.is_some() {
                let below_header = Rect::from_min_size(origin + vec2(0.0, header_height + 24.0), vec2(width, 40.0));
                ui.scope_builder(UiBuilder::new().max_rect(below_header), |ui| widgets::load_state(ui, loading, error, None));
                return;
            }
            // Only rows in the viewport are laid out and painted.
            let row = columns.row;
            let first = ((viewport.top() - header_height) / row).floor().max(0.0) as usize;
            let last = (((viewport.bottom() - header_height) / row).ceil().max(0.0) as usize).min(count);
            let removable = self.showing_own_playlist().is_some();
            let liked = |id: &TrackId| self.library.likes.likes.tracks.contains(id);
            let table = Table {
                tracks: &self.browse.tracks,
                columns: &columns,
                now: (playing_id, playing),
                liked: &liked,
                menu: RowMenu { playlists: &self.library.playlists, removable },
            };
            widgets::track_rows(ui, &mut self.covers, &table, first..last, origin + vec2(0.0, header_height), table_width, acts);
        });
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
                art: coll.picture.clone(),
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

    fn search_header(&mut self, ui: &mut Ui, head: Rect, st: &Status, acts: &mut Actions) {
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
                card_row(ui, &mut self.covers, ("search-row", i), &section.title, &section.items, st, acts);
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
        let CollSource::Album(_) = coll.source else { return None };
        let main = self.browse.tracks.first()?.artists.first()?.clone();
        self.browse.tracks.iter().all(|t| t.artists.iter().any(|a| a.id == main.id)).then_some(main)
    }

    fn collection_header(&mut self, ui: &mut Ui, head: Rect, acts: &mut Actions) {
        let p = colors();
        let Header { kind, title, subtitle, art, tile, round } = self.header();
        header_band(ui.painter(), Rect::from_min_size(head.min, vec2(head.width(), HEADER_BAND)));
        let art_rect = Rect::from_min_size(head.min + vec2(0.0, 18.0), Vec2::splat(metrics::HEADER_ART));
        let corner = if round { (metrics::HEADER_ART / 2.0) as u8 } else { radius::CONTROL };
        match (art, tile) {
            (Some(art), _) => widgets::cover(ui, &mut self.covers, Some(&art), metrics::HEADER_PX, art_rect, corner),
            (None, Some((icon, color))) => widgets::tile(ui, art_rect, icon, color, radius::CONTROL),
            _ => widgets::cover(ui, &mut self.covers, None, metrics::HEADER_PX, art_rect, radius::CONTROL),
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
                acts.extend(clicked.map(Action::OpenArtist));
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
            let play_from = |shuffle| Action::PlayFrom { tracks: self.browse.tracks.clone(), index: 0, shuffle };
            if count > 0 && widgets::pill(ui, "Play", Some(Icon::Play), true).clicked() {
                acts.push(play_from(false));
            }
            if count > 1 && !podcast && widgets::pill(ui, "Shuffle", None, false).clicked() {
                acts.push(play_from(true));
            }
            if count > 0 {
                // Briefly confirms, since the queue panel may be closed.
                let just_added = self.queued_flash.is_some_and(|t| t.elapsed() < QUEUED_FLASH);
                let (label, icon) = if just_added { ("Added", Some(Icon::Check)) } else { ("Add to queue", None) };
                let hint = format!("Add every {} to the end of the queue", if podcast { "episode" } else { "track" });
                if widgets::pill(ui, label, icon, false).on_hover_text(hint).clicked() {
                    acts.push(Action::QueueAll(self.browse.tracks.clone()));
                }
                if just_added {
                    ui.ctx().request_repaint_after(QUEUED_FLASH);
                }
            }
            // Albums can be liked from their header.
            if let View::Collection(coll) = &self.browse.view
                && let CollSource::Album(id) = &coll.source
            {
                let what = Likeable::Album(id.clone());
                if heart_square(ui, self.library.likes.likes.contains(&what)).clicked() {
                    acts.push(Action::ToggleLike(what));
                }
            }
            // Episodes can't go into playlists.
            if count > 0 && !podcast {
                let editable = self.showing_own_playlist().cloned();
                let more = widgets::icon_button(ui, Icon::More, 18.0, colors().dim).on_hover_text("More");
                egui::Popup::menu(&more).width(220.0).show(|ui| {
                    // Every playable track of the list, to a playlist or a new one.
                    let mut pick = widgets::add_to_playlist(ui, &self.library.playlists).map(|choice| match choice {
                        TrackMenu::AddTo(playlist) => Action::EditPlaylist(Edit::Add { playlist, tracks: self.browse.tracks.clone() }),
                        _ => Action::NewPlaylist(playable(self.browse.tracks.clone())),
                    });
                    if let Some(playlist) = editable {
                        widgets::divider(ui);
                        if widgets::menu_row(ui, None, "Rename…", None, false, colors().text).clicked() {
                            pick =
                                Some(Action::PlaylistDialog(Dialog::Rename { title: playlist.title.clone(), playlist: playlist.clone() }));
                        }
                        if widgets::menu_row(ui, Some(Icon::Close), "Delete playlist…", None, false, colors().danger).clicked() {
                            pick = Some(Action::PlaylistDialog(Dialog::Delete { playlist }));
                        }
                    }
                    if let Some(pick) = pick {
                        acts.push(pick);
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
        let resp = ui.interact(rect, ui.scope_id().with(("more-by", k)), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
        }
        let art = Rect::from_min_size(pos2(rect.left() + 2.0, rect.center().y - 21.0), Vec2::splat(42.0));
        widgets::cover(ui, covers, item.picture.as_ref(), metrics::THUMB_PX, art, radius::THUMB);
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

impl App {
    /// The own playlist on screen, if that's what is shown.
    pub(in crate::ui) fn showing_own_playlist(&self) -> Option<&Playlist> {
        let View::Collection(coll) = &self.browse.view else { return None };
        let CollSource::Playlist(id) = &coll.source else { return None };
        self.library.playlists.iter().find(|p| p.id == *id && p.owned)
    }

    /// Whether an album's tracks are on screen.
    fn showing_album(&self) -> bool {
        matches!(&self.browse.view, View::Collection(coll) if matches!(coll.source, CollSource::Album(_)))
    }

    /// Whether a podcast's episodes are on screen.
    fn showing_podcast(&self) -> bool {
        matches!(&self.browse.view, View::Collection(coll) if matches!(coll.source, CollSource::Show(_)))
    }
}
