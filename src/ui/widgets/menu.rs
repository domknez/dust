//! Popover menus and their rows.

use super::text::text_left;
use crate::model::Playlist;
use crate::ui::state::PlayMode;
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, radius, typography as ty};
use eframe::egui::{self, Color32, CornerRadius, Response, Sense, Ui, Vec2, pos2, vec2};

const CONTEXT_MENU_WIDTH: f32 = 190.0;

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
}

/// Right-click menu for a track: Play next, Add to queue, Start track mix (with
/// `mix`), Add to playlist › (the own ones of `playlists`), and Remove from this
/// playlist when `removable`.
pub fn track_menu(resp: &Response, playlists: &[Playlist], removable: bool, mix: bool) -> Option<TrackMenu> {
    let mut pick = None;
    resp.context_menu(|ui| {
        ui.set_min_width(CONTEXT_MENU_WIDTH);
        ui.set_max_width(CONTEXT_MENU_WIDTH);
        if menu_row(ui, Some(Icon::Play), "Play next", None, false, colors().text).clicked() {
            pick = Some(TrackMenu::Play(PlayMode::Next));
        }
        if menu_row(ui, Some(Icon::Queue), "Add to queue", None, false, colors().text).clicked() {
            pick = Some(TrackMenu::Play(PlayMode::Queue));
        }
        if mix && menu_row(ui, Some(Icon::Streaming), "Start track mix", None, false, colors().text).clicked() {
            pick = Some(TrackMenu::Mix);
        }
        if let Some(choice) = add_to_playlist(ui, playlists) {
            pick = Some(choice);
        }
        if removable {
            divider(ui);
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

/// "Add to playlist ›" row with a submenu: New playlist…, then those of
/// `playlists` the account owns. Returns AddTo or NewPlaylist when one was chosen.
pub fn add_to_playlist(ui: &mut Ui, playlists: &[Playlist]) -> Option<TrackMenu> {
    let row = menu_row(ui, Some(Icon::Plus), "Add to playlist", None, false, colors().text);
    let chevron = egui::Rect::from_center_size(pos2(row.rect.right() - 16.0, row.rect.center().y), Vec2::splat(12.0));
    icons::paint(ui.painter(), chevron, Icon::ChevronRight, colors().faint);
    let mut pick = None;
    egui::containers::menu::SubMenu::new().show(ui, &row, |ui| {
        ui.set_min_width(CONTEXT_MENU_WIDTH);
        ui.set_max_width(CONTEXT_MENU_WIDTH + 40.0);
        if menu_row(ui, Some(Icon::Plus), "New playlist…", None, false, colors().accent).clicked() {
            pick = Some(TrackMenu::NewPlaylist);
        }
        let own: Vec<&Playlist> = playlists.iter().filter(|p| p.owned).collect();
        if !own.is_empty() {
            divider(ui);
        }
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for p in own {
                if menu_row(ui, None, &p.title, None, false, colors().text).clicked() {
                    pick = Some(TrackMenu::AddTo(p.clone()));
                }
            }
        });
        if pick.is_some() {
            ui.close();
        }
    });
    pick
}
