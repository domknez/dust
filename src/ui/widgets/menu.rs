//! Popover menus and their rows.

use super::text::text_left;
use crate::model::{Playlist, PlaylistId};
use crate::ui::state::PlayMode;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, radius, typography as ty};
use eframe::egui::{self, Color32, CornerRadius, Key, Response, Sense, Ui, Vec2, pos2, vec2};

const CONTEXT_MENU_WIDTH: f32 = 190.0;
/// Playlists used last, offered on top of "Add to playlist".
const RECENT: usize = 3;
/// The shortcut that adds what's playing to the playlist used last, as menus show it.
pub const ADD_TO_RECENT_KEYS: &str = if cfg!(target_os = "macos") { "⌘D" } else { "Ctrl+D" };

/// The playlists "Add to playlist" offers: the account's own ones of `all`, those
/// tracks were added to last (`recent`, the latest first) on top.
#[derive(Clone, Copy)]
pub struct Playlists<'a> {
    pub all: &'a [Playlist],
    pub recent: &'a [PlaylistId],
}

/// Frame for our popovers.
pub fn popover_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::new()
        .fill(colors().surface)
        .stroke(egui::Stroke::new(1.0, colors().line))
        .corner_radius(CornerRadius::same(radius::POPOVER))
        .inner_margin(egui::Margin::same(10))
        .shadow(ctx.global_style().visuals.popup_shadow)
}

/// Menu entry: optional icon, title, optional subtitle, check mark when selected.
pub fn menu_row(ui: &mut Ui, icon: Option<Icon>, title: &str, subtitle: Option<&str>, checked: bool, color: Color32) -> Response {
    let height = if subtitle.is_some() { 46.0 } else { 36.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(radius::CONTROL - 1), colors().raised);
    }
    let mut x = rect.left() + 10.0;
    if let Some(i) = icon {
        icons::paint(ui.painter(), egui::Rect::from_center_size(pos2(x + 8.0, rect.center().y), Vec2::splat(16.0)), i, color);
        x += 28.0;
    }
    let width = rect.right() - x - 34.0;
    let painter = ui.painter();
    match subtitle {
        Some(sub) => {
            let style = if checked { ty::ITEM.strong() } else { ty::ITEM };
            text_left(painter, pos2(x, rect.center().y - 8.0), title, style, color, width);
            text_left(painter, pos2(x, rect.center().y + 10.0), sub, ty::CAPTION, colors().faint, width);
        }
        None => {
            text_left(painter, pos2(x, rect.center().y), title, ty::ITEM, color, width);
        }
    }
    if checked {
        icons::paint(
            painter,
            egui::Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(14.0)),
            Icon::Check,
            colors().accent,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Menu entry with its keyboard shortcut right-aligned, as in native menus.
pub fn shortcut_row(ui: &mut Ui, icon: Icon, title: &str, keys: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let p = colors();
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(radius::CONTROL - 1), p.raised);
    }
    let x = rect.left() + 10.0;
    icons::paint(ui.painter(), egui::Rect::from_center_size(pos2(x + 8.0, rect.center().y), Vec2::splat(16.0)), icon, p.text);
    let keys_width = ui.painter().layout_no_wrap(keys.to_string(), ty::SECONDARY.font(), p.faint).size().x;
    let painter = ui.painter();
    painter.text(pos2(rect.right() - 10.0, rect.center().y), egui::Align2::RIGHT_CENTER, keys, ty::SECONDARY.font(), p.faint);
    text_left(painter, pos2(x + 28.0, rect.center().y), title, ty::ITEM, p.text, rect.right() - x - 48.0 - keys_width);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Uppercase caption inside menus and panels.
pub fn caption(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    text_left(ui.painter(), pos2(rect.left() + 4.0, rect.center().y), text, ty::OVERLINE, colors().faint, rect.width());
}

/// Hairline divider with vertical breathing room.
pub fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 13.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, egui::Stroke::new(1.0, colors().line));
}

/// Right-click menu offering Play next / Add to queue.
pub fn queue_menu(resp: &Response) -> Option<PlayMode> {
    let mut pick = None;
    resp.context_menu(|ui| {
        // Menu rows fill the available width; cap it or the menu spans the window.
        ui.set_min_width(CONTEXT_MENU_WIDTH);
        ui.set_max_width(CONTEXT_MENU_WIDTH);
        if menu_row(ui, Some(Icon::Play), "Play next", None, false, colors().text).clicked() {
            pick = Some(PlayMode::Next);
            ui.close();
        }
        if menu_row(ui, Some(Icon::Queue), "Add to queue", None, false, colors().text).clicked() {
            pick = Some(PlayMode::Queue);
            ui.close();
        }
    });
    pick
}

/// A choice in a track's right-click menu.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackMenu {
    Play(PlayMode),
    /// Add to this (own) playlist.
    AddTo(Playlist),
    /// Name a new playlist holding the track.
    NewPlaylist,
    /// Take it out of the playlist being shown.
    Remove,
    /// Play songs like it, starting with it.
    Mix,
    /// Save to the Crate, to check out later.
    Crate,
}

/// What a track menu acts on: one track or several picked together, and whether
/// any of them can be played.
#[derive(Clone, Copy)]
pub struct MenuTarget {
    pub count: usize,
    pub playable: bool,
}

/// Right-click menu for a track, or for the `target.count` picked: Play next, Add to
/// queue, Start track mix (one track, with `mix`), Add to playlist › (the own ones of
/// `playlists`), and Remove from this playlist when `removable`. Only removing is
/// offered for tracks that can't be played.
pub fn track_menu(resp: &Response, playlists: Playlists, removable: bool, mix: bool, target: MenuTarget) -> Option<TrackMenu> {
    let mut pick = None;
    resp.context_menu(|ui| {
        ui.set_min_width(CONTEXT_MENU_WIDTH);
        ui.set_max_width(CONTEXT_MENU_WIDTH);
        if target.count > 1 {
            caption(ui, &format!("{} TRACKS", target.count));
        }
        if target.playable {
            if menu_row(ui, Some(Icon::Play), "Play next", None, false, colors().text).clicked() {
                pick = Some(TrackMenu::Play(PlayMode::Next));
            }
            if menu_row(ui, Some(Icon::Queue), "Add to queue", None, false, colors().text).clicked() {
                pick = Some(TrackMenu::Play(PlayMode::Queue));
            }
            if mix && target.count == 1 && menu_row(ui, Some(Icon::Streaming), "Start track mix", None, false, colors().text).clicked() {
                pick = Some(TrackMenu::Mix);
            }
            if let Some(choice) = add_to_playlist(ui, playlists) {
                pick = Some(choice);
            }
            if menu_row(ui, Some(Icon::Crate), "Save to Crate", None, false, colors().text).clicked() {
                pick = Some(TrackMenu::Crate);
            }
        }
        if removable {
            if target.playable {
                divider(ui);
            }
            if menu_row(ui, Some(Icon::Close), "Remove from this playlist", None, false, colors().danger).clicked() {
                pick = Some(TrackMenu::Remove);
            }
        }
        if pick.is_some() {
            ui.close();
        }
    });
    pick
}

/// "Add to playlist ›" row with a submenu: New playlist…, the playlists used last,
/// then every own one. Returns AddTo or NewPlaylist when one was chosen.
pub fn add_to_playlist(ui: &mut Ui, playlists: Playlists) -> Option<TrackMenu> {
    let row = menu_row(ui, Some(Icon::Plus), "Add to playlist", None, false, colors().text);
    let chevron = egui::Rect::from_center_size(pos2(row.rect.right() - 16.0, row.rect.center().y), Vec2::splat(12.0));
    icons::paint(ui.painter(), chevron, Icon::ChevronRight, colors().faint);
    let mut pick = None;
    egui::containers::menu::SubMenu::new().show(ui, &row, |ui| {
        ui.set_min_width(CONTEXT_MENU_WIDTH);
        ui.set_max_width(CONTEXT_MENU_WIDTH + 40.0);
        pick = playlist_picker(ui, playlists, None);
        if pick.is_some() {
            ui.close();
        }
    });
    pick
}

/// The rows of a playlist choice: New playlist…, the playlists used last, then every
/// own one. With `filter` (where the typed text is kept), a field on top narrows the
/// list as you type and Enter takes the first match.
pub fn playlist_picker(ui: &mut Ui, playlists: Playlists, filter: Option<egui::Id>) -> Option<TrackMenu> {
    let p = colors();
    let own: Vec<&Playlist> = playlists.all.iter().filter(|pl| pl.owned).collect();
    let mut query = filter.map(|id| ui.data(|d| d.get_temp::<String>(id)).unwrap_or_default());
    if let Some(query) = query.as_mut() {
        let field =
            ui.add(egui::TextEdit::singleline(query).hint_text("Find a playlist").desired_width(f32::INFINITY).margin(vec2(8.0, 6.0)));
        field.request_focus();
        ui.add_space(6.0);
    }
    let needle = query.as_deref().unwrap_or_default().trim().to_lowercase();
    let matches: Vec<&Playlist> = own.iter().copied().filter(|pl| pl.title.to_lowercase().contains(&needle)).collect();
    let mut pick = None;
    if needle.is_empty() {
        if menu_row(ui, Some(Icon::Plus), "New playlist…", None, false, p.accent).clicked() {
            pick = Some(TrackMenu::NewPlaylist);
        }
        let recent: Vec<&Playlist> =
            playlists.recent.iter().filter_map(|id| own.iter().find(|pl| pl.id == *id).copied()).take(RECENT).collect();
        if !recent.is_empty() {
            divider(ui);
            caption(ui, "RECENT");
            for (i, pl) in recent.into_iter().enumerate() {
                let mut row = menu_row(ui, None, &pl.title, None, false, p.text);
                if i == 0 {
                    row = row.on_hover_text(format!("{ADD_TO_RECENT_KEYS} adds what's playing here"));
                }
                if row.clicked() {
                    pick = Some(TrackMenu::AddTo(pl.clone()));
                }
            }
            if !own.is_empty() {
                divider(ui);
                caption(ui, "ALL");
            }
        } else if !own.is_empty() {
            divider(ui);
        }
    }
    if matches.is_empty() && !needle.is_empty() {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
        text_left(
            ui.painter(),
            pos2(rect.left() + 10.0, rect.center().y),
            "No playlist by that name",
            ty::ITEM,
            p.faint,
            rect.width() - 20.0,
        );
    }
    egui::ScrollArea::vertical().id_salt("playlists").max_height(320.0).show(ui, |ui| {
        for pl in &matches {
            if menu_row(ui, None, &pl.title, None, false, p.text).clicked() {
                pick = Some(TrackMenu::AddTo((*pl).clone()));
            }
        }
    });
    if !needle.is_empty()
        && pick.is_none()
        && let Some(first) = matches.first()
        && ui.input(|i| i.key_pressed(Key::Enter))
    {
        pick = Some(TrackMenu::AddTo((*first).clone()));
    }
    if let (Some(id), Some(query)) = (filter, query) {
        ui.data_mut(|d| {
            if pick.is_some() {
                d.remove::<String>(id);
            } else {
                d.insert_temp(id, query);
            }
        });
    }
    pick
}

/// A choice in the right-click menu of the track playing.
#[derive(Clone, Debug, PartialEq)]
pub enum NowPlayingMenu {
    Track(TrackMenu),
    /// Like it, or take the like back.
    Like,
    Artist,
    Album,
}

/// What the right-click menu of the track playing offers.
#[derive(Clone, Copy)]
pub struct NowPlayingOffer {
    /// It can be liked (not an episode); whether it is.
    pub liked: Option<bool>,
    /// "Start track mix".
    pub mix: bool,
    pub artist: bool,
    /// "Go to album", or "Go to podcast" for an episode.
    pub album: Option<&'static str>,
}

/// The rows of the right-click menu of the track playing: Add to playlist ›, like,
/// Start track mix, then Go to artist / album.
pub fn now_playing_menu(ui: &mut Ui, playlists: Playlists, offer: NowPlayingOffer) -> Option<NowPlayingMenu> {
    ui.set_min_width(CONTEXT_MENU_WIDTH + 20.0);
    ui.set_max_width(CONTEXT_MENU_WIDTH + 20.0);
    let p = colors();
    let mut pick = None;
    if let Some(liked) = offer.liked {
        pick = add_to_playlist(ui, playlists).map(NowPlayingMenu::Track);
        let (icon, title) = if liked { (Icon::Heart, "Remove from Loved tracks") } else { (Icon::HeartOutline, "Add to Loved tracks") };
        if menu_row(ui, Some(icon), title, None, false, p.text).clicked() {
            pick = Some(NowPlayingMenu::Like);
        }
        if offer.mix && menu_row(ui, Some(Icon::Streaming), "Start track mix", None, false, p.text).clicked() {
            pick = Some(NowPlayingMenu::Track(TrackMenu::Mix));
        }
        if menu_row(ui, Some(Icon::Crate), "Save to Crate", None, false, p.text).clicked() {
            pick = Some(NowPlayingMenu::Track(TrackMenu::Crate));
        }
    }
    if offer.artist || offer.album.is_some() {
        if offer.liked.is_some() {
            divider(ui);
        }
        if offer.artist && menu_row(ui, None, "Go to artist", None, false, p.text).clicked() {
            pick = Some(NowPlayingMenu::Artist);
        }
        if let Some(title) = offer.album
            && menu_row(ui, None, title, None, false, p.text).clicked()
        {
            pick = Some(NowPlayingMenu::Album);
        }
    }
    if pick.is_some() {
        ui.close();
    }
    pick
}
