//! The Crate, where it shows: the *crate* tab of the panel on the right (save what's
//! playing, jot a name down, tick off what you've heard) and its page (everything
//! in it, filtered, one entry in detail). See [`crate::ui::crate_store`].

use crate::model::Likeable;
use crate::player::{Cmd, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::crate_store::{Change, Entry, Thing};
use crate::ui::plays::{flow_label, when};
use crate::ui::state::{ArtistView, Coll, CollSource, PlayMode, View};
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, TrackMenu, text_left};
use eframe::egui::{
    self, Align2, CornerRadius, Key, KeyboardShortcut, Modifiers, PopupCloseBehavior, Rect, RectAlign, RichText, Sense, Ui, UiBuilder,
    Vec2, pos2, vec2,
};

/// Saves the track playing to the Crate.
const SAVE_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
const SAVE_KEYS: &str = if cfg!(target_os = "macos") { "⌘⇧S" } else { "Ctrl+Shift+S" };
const JOT_HINT: &str = "Jot down a name, a link or a note";
const ROW: f32 = 56.0;
/// The page's entry details, right of the list.
const DETAIL_WIDTH: f32 = 340.0;

/// Which entries the Crate page lists.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Filter {
    #[default]
    All,
    New,
    Heard,
    /// Only tracks, albums, artists or notes (by [`Thing::kind`]).
    Kind(&'static str),
}

impl Filter {
    fn keeps(self, entry: &Entry) -> bool {
        match self {
            Filter::All => true,
            Filter::New => !entry.heard,
            Filter::Heard => entry.heard,
            Filter::Kind(kind) => entry.thing.kind() == kind,
        }
    }
}

/// The Crate page's and tab's own state.
#[derive(Default)]
pub struct CrateView {
    pub filter: Filter,
    /// The entry shown in detail.
    pub selected: Option<String>,
    /// What's typed in the page's and the tab's fields.
    jot: String,
    tab_jot: String,
    /// The selected entry's note being edited: (entry id, text).
    note: Option<(String, String)>,
}

impl App {
    /// ⌘⇧S / Ctrl+Shift+S: the track playing goes into the Crate.
    pub(in crate::ui) fn crate_shortcut(&self, ctx: &egui::Context, st: &Status, acts: &mut Actions) {
        if ctx.input_mut(|i| i.consume_shortcut(&SAVE_SHORTCUT)) {
            acts.extend(self.save_playing(st));
        }
    }

    /// Saving the track playing, with the Flow it came from.
    fn save_playing(&self, st: &Status) -> Option<Action> {
        let track = st.track.as_ref().filter(|t| !t.is_episode())?;
        let from = st.flow.as_ref().map(|f| flow_label(f, !self.features().flow)).unwrap_or_default();
        Some(Action::SaveToCrate(Thing::Track(track.clone()), from))
    }

    /// Put `thing` in the Crate (or say it's there already).
    pub(in crate::ui) fn save_to_crate(&mut self, ctx: &egui::Context, thing: Thing, from: String) {
        let Some(client) = &self.client else { return };
        let source = client.id().key;
        let message = match self.crate_box.find(source, &thing).map(|e| (e.id.clone(), e.heard)) {
            Some((id, true)) => {
                self.crate_box.change(Change::Heard { id, heard: false });
                "Back in your Crate as new"
            }
            Some(_) => "Already in your Crate",
            None => {
                self.crate_box.add(source, thing, &from, "");
                "Saved to your Crate"
            }
        };
        self.toasts.info(ctx, message);
    }

    pub(in crate::ui) fn crate_note(&mut self, note: String) {
        if !note.trim().is_empty() {
            self.crate_box.add("", Thing::Note, "", &note);
        }
    }

    /// Open the Crate page, showing `entry` (or the filter's first).
    pub(in crate::ui) fn open_crate(&mut self, entry: Option<String>, filter: Option<Filter>) {
        if let Some(filter) = filter {
            self.crate_view.filter = filter;
        }
        if entry.is_some() {
            self.crate_view.filter = Filter::All;
            self.crate_view.selected = entry;
        }
        self.open_view(View::Crate);
    }

    /// Search the source for an entry from another source, or a note.
    pub(in crate::ui) fn search_for(&mut self, query: String) {
        self.browse.search = query;
        self.open_view(View::Search);
    }

    // ------------------------------------------------------------ the panel's tab

    pub(in crate::ui) fn crate_panel(&mut self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        ui.add_space(metrics::TOP_INSET);
        self.side_panel_tabs(ui, acts);
        ui.add_space(4.0);
        self.save_playing_row(ui, st, acts);
        ui.add_space(8.0);
        if let Some(note) = jot_field(ui, "crate-tab-jot", &mut self.crate_view.tab_jot) {
            acts.push(Action::CrateNote(note));
        }
        ui.add_space(14.0);
        let fresh: Vec<Entry> = self.crate_box.entries().iter().filter(|e| !e.heard).cloned().collect();
        let heard = self.crate_box.entries().len() - fresh.len();
        let p = colors();
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
        text_left(ui.painter(), pos2(row.left() + 4.0, row.center().y), &format!("NEW · {}", fresh.len()), ty::OVERLINE, p.faint, 120.0);
        if !fresh.is_empty() {
            let hint = "tick when you've heard it";
            ui.painter().text(pos2(row.right() - 4.0, row.center().y), Align2::RIGHT_CENTER, hint, ty::CAPTION.font(), p.faint);
        }
        let current = self.client.as_ref().map(|c| c.id().key).unwrap_or_default();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if fresh.is_empty() {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Nothing new to check out").color(p.dim).font(ty::TITLE.font()));
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("{SAVE_KEYS} saves what's playing")).color(p.faint).font(ty::SMALL.font()));
                });
            }
            for entry in &fresh {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let hovered = ui.rect_contains_pointer(rect);
                if hovered {
                    ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), p.hover);
                }
                let tick = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(26.0));
                entry_row(ui, &mut self.covers, rect.with_max_x(tick.left() - 4.0), entry, current);
                let over_tick = resp.hover_pos().is_some_and(|pos| tick.contains(pos));
                tick_box(ui, tick, false, over_tick);
                if resp.clicked() {
                    acts.push(if over_tick {
                        Action::Crate(Change::Heard { id: entry.id.clone(), heard: true })
                    } else {
                        Action::OpenCrate(Some(entry.id.clone()), None)
                    });
                }
                resp.on_hover_text(if over_tick { "Mark as heard" } else { "Open in your Crate" })
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
            }
            if heard > 0 {
                ui.add_space(8.0);
                let label = RichText::new(format!("heard · {heard} ›")).font(ty::META.font());
                if ui.link(label).clicked() {
                    acts.push(Action::OpenCrate(None, Some(Filter::Heard)));
                }
            }
            ui.add_space(10.0);
        });
    }

    /// "Save what's playing", with the track and the shortcut; "In your Crate" once
    /// it's there.
    fn save_playing_row(&self, ui: &mut Ui, st: &Status, acts: &mut Actions) {
        let p = colors();
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click());
        let track = st.track.as_ref().filter(|t| !t.is_episode());
        let source = self.client.as_ref().map(|c| c.id().key).unwrap_or_default();
        let saved = track.and_then(|t| self.crate_box.find(source, &Thing::Track(t.clone()))).is_some_and(|e| !e.heard);
        let enabled = track.is_some() && !saved;
        let corner = CornerRadius::same(radius::CONTROL);
        ui.painter().rect_filled(rect, corner, if enabled && resp.hovered() { p.raised } else { p.surface });
        ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
        let icon = Rect::from_center_size(pos2(rect.left() + 24.0, rect.center().y), Vec2::splat(18.0));
        let (icon_kind, icon_color) = if saved { (Icon::Check, p.accent) } else { (Icon::Crate, if enabled { p.accent } else { p.faint }) };
        icons::paint(ui.painter(), icon, icon_kind, icon_color);
        let x = icon.right() + 14.0;
        let width = rect.right() - 64.0 - x;
        let title = if saved { "In your Crate" } else { "Save what's playing" };
        text_left(
            ui.painter(),
            pos2(x, rect.center().y - 8.0),
            title,
            ty::ITEM.strong(),
            if enabled || saved { p.text } else { p.dim },
            width,
        );
        let line = track.map_or_else(|| "Nothing playing".to_string(), |t| format!("{} · {}", t.title, t.artist));
        text_left(ui.painter(), pos2(x, rect.center().y + 10.0), &line, ty::SMALL, p.dim, width);
        ui.painter().text(pos2(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, SAVE_KEYS, ty::META.font(), p.faint);
        if enabled && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            acts.extend(self.save_playing(st));
        }
    }

    // ------------------------------------------------------------ the page

    pub(in crate::ui) fn crate_page(&mut self, ui: &mut Ui, acts: &mut Actions) {
        let p = colors();
        let head = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover()).0;
        text_left(ui.painter(), head.min + vec2(0.0, 18.0), "Crate", ty::PAGE_TITLE, p.text, head.width());
        let line = "Things to check out later, from any source and from anyone.";
        text_left(ui.painter(), head.min + vec2(0.0, 50.0), line, ty::LEAD, p.dim, head.width());
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.set_max_width(560.0);
            let jot = &mut self.crate_view.jot;
            let typed = !jot.trim().is_empty();
            let mut save = jot_field_sized(ui, "crate-page-jot", jot, ui.available_width() - 90.0);
            if typed && widgets::pill(ui, "Save", None, true).clicked() {
                save = Some(std::mem::take(jot));
            }
            if let Some(note) = save {
                acts.push(Action::CrateNote(note));
            }
        });
        ui.add_space(14.0);
        self.filter_chips(ui);
        ui.add_space(12.0);

        let filter = self.crate_view.filter;
        let shown: Vec<Entry> = self.crate_box.entries().iter().filter(|e| filter.keeps(e)).cloned().collect();
        if self.crate_view.selected.as_ref().is_none_or(|id| !shown.iter().any(|e| e.id == *id)) {
            self.crate_view.selected = shown.first().map(|e| e.id.clone());
        }
        if shown.is_empty() {
            ui.add_space(30.0);
            let (title, hint) = if self.crate_box.entries().is_empty() {
                ("Your Crate is empty", format!("Save what's playing with {SAVE_KEYS}, or right-click a track → Save to Crate"))
            } else {
                ("Nothing here", "Try another filter".to_string())
            };
            ui.label(RichText::new(title).color(p.dim).font(ty::TITLE.font()));
            ui.add_space(4.0);
            ui.label(RichText::new(hint).color(p.faint).font(ty::SMALL.font()));
            return;
        }
        let area = ui.available_rect_before_wrap();
        let detail_width = DETAIL_WIDTH.min(area.width() * 0.45);
        let list = Rect::from_min_max(area.min, pos2(area.right() - detail_width - 24.0, area.bottom()));
        let detail = Rect::from_min_max(pos2(list.right() + 24.0, area.top()), area.max);
        let current = self.client.as_ref().map(|c| c.id().key).unwrap_or_default();
        ui.scope_builder(UiBuilder::new().max_rect(list), |ui| {
            egui::ScrollArea::vertical().id_salt("crate-list").auto_shrink([false, false]).show(ui, |ui| {
                for entry in &shown {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW + 6.0), Sense::click());
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let selected = self.crate_view.selected.as_ref() == Some(&entry.id);
                    if selected || resp.hovered() {
                        let fill = if selected { p.raised } else { p.hover };
                        ui.painter().rect_filled(rect, CornerRadius::same(radius::ROW), fill);
                    }
                    let status = Rect::from_min_max(pos2(rect.right() - 90.0, rect.top()), rect.max);
                    entry_row(ui, &mut self.covers, rect.with_max_x(status.left()), entry, current);
                    status_line(ui, status, entry);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.crate_view.selected = Some(entry.id.clone());
                    }
                }
            });
        });
        let entry = self.crate_view.selected.as_ref().and_then(|id| self.crate_box.get(id)).cloned();
        if let Some(entry) = entry {
            ui.scope_builder(UiBuilder::new().max_rect(detail), |ui| {
                egui::ScrollArea::vertical().id_salt("crate-detail").auto_shrink([false, false]).show(ui, |ui| {
                    self.entry_detail(ui, &entry, current, acts);
                });
            });
        }
        ui.allocate_rect(area, Sense::hover());
    }

    /// All · New · Heard, then the kinds there are, each with its count.
    fn filter_chips(&mut self, ui: &mut Ui) {
        let entries = self.crate_box.entries();
        let count = |f: Filter| entries.iter().filter(|e| f.keeps(e)).count();
        let mut chips = vec![("All", Filter::All), ("New", Filter::New), ("Heard", Filter::Heard)];
        for (label, kind) in [("Tracks", "track"), ("Albums", "album"), ("Artists", "artist"), ("Notes", "note")] {
            if count(Filter::Kind(kind)) > 0 {
                chips.push((label, Filter::Kind(kind)));
            }
        }
        let p = colors();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (label, filter) in chips {
                let text = format!("{label}  {}", count(filter));
                let galley = ui.painter().layout_no_wrap(text.clone(), ty::SMALL.font(), p.text);
                let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 24.0, 30.0), Sense::click());
                let on = self.crate_view.filter == filter;
                let corner = CornerRadius::same(15);
                if on {
                    ui.painter().rect_filled(rect, corner, p.text);
                } else {
                    if resp.hovered() {
                        ui.painter().rect_filled(rect, corner, p.hover);
                    }
                    ui.painter().rect_stroke(rect, corner, egui::Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
                }
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, ty::SMALL.font(), if on { p.bg } else { p.text });
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.crate_view.filter = filter;
                }
            }
        });
    }

    /// One entry in full: what it is, how to get to it, its note, where it came from
    /// and when it was saved, and marking it heard or removing it.
    fn entry_detail(&mut self, ui: &mut Ui, entry: &Entry, current: &str, acts: &mut Actions) {
        let p = colors();
        let here = entry.source.is_empty() || entry.source == current;
        let mut kind = entry.thing.kind().to_uppercase();
        if !here {
            kind = format!("{kind} · ON {}", source_name(&entry.source).to_uppercase());
        }
        let picture = entry.picture().cloned();
        if !matches!(entry.thing, Thing::Note) {
            let size = 120.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
            let corner = if matches!(entry.thing, Thing::Artist { .. }) { (size / 2.0) as u8 } else { radius::CARD };
            widgets::cover(ui, &mut self.covers, picture.as_ref(), metrics::HOME_CARD_PX, rect, corner);
            ui.add_space(12.0);
        }
        ui.label(RichText::new(kind).font(ty::OVERLINE.font()).color(p.dim));
        ui.add_space(2.0);
        let title = if entry.thing == Thing::Note { format!("“{}”", entry.title()) } else { entry.title().to_string() };
        ui.add(egui::Label::new(RichText::new(title).font(ty::SECTION_TITLE.font()).color(p.text)).wrap());
        if !entry.by().is_empty() {
            ui.label(RichText::new(entry.by()).font(ty::SECONDARY.font()).color(p.dim));
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            match (&entry.thing, here) {
                (Thing::Track(track), true) if track.available => {
                    if widgets::pill(ui, "Play", Some(Icon::Play), true).clicked() {
                        acts.push(Action::TrackMenu(vec![track.clone()], TrackMenu::Play(PlayMode::Next)));
                        acts.push(Action::Player(Cmd::Next));
                    }
                    self.crate_add_to_playlist(ui, track, acts);
                }
                (Thing::Album { id, title, artist, picture }, true) => {
                    if widgets::pill(ui, "Open album", None, true).clicked() {
                        acts.push(Action::OpenCollection(Coll {
                            source: CollSource::Album(id.clone()),
                            kind: "ALBUM",
                            title: title.clone(),
                            subtitle: artist.clone(),
                            picture: picture.clone(),
                        }));
                    }
                }
                (Thing::Artist { id, name, picture }, true) => {
                    if widgets::pill(ui, "Open artist", None, true).clicked() {
                        let artist = ArtistView { id: id.clone(), name: name.clone(), picture: picture.clone() };
                        acts.push(Action::Open(View::Artist(Box::new(artist))));
                    }
                }
                _ => {
                    let query = match &entry.thing {
                        Thing::Note => entry.note.clone(),
                        _ if entry.by().is_empty() => entry.title().to_string(),
                        _ => format!("{} {}", entry.by(), entry.title()),
                    };
                    let hint = format!("Search {} for it", self.client.as_ref().map_or("", |c| c.id().name));
                    if widgets::pill(ui, "Search for it", Some(Icon::Search), true).on_hover_text(hint).clicked() {
                        acts.push(Action::SearchFor(query));
                    }
                }
            }
        });
        if let Thing::Track(track) = &entry.thing
            && here
            && !track.is_episode()
        {
            let what = Likeable::Track(track.id.clone());
            let liked = self.library.likes.likes.contains(&what);
            ui.add_space(6.0);
            let label = if liked { "In Loved tracks" } else { "Add to Loved tracks" };
            if ui.link(RichText::new(label).font(ty::SMALL.font())).clicked() {
                acts.push(Action::ToggleLike(what));
            }
        }

        ui.add_space(18.0);
        if entry.thing != Thing::Note {
            self.note_field(ui, entry, acts);
            ui.add_space(14.0);
        }
        let zone = when::zone();
        let saved = format!(
            "{}, {}",
            when::day(entry.saved, &zone).map_or_else(String::new, |d| when::day_name(d, when::today(&zone))),
            when::clock(entry.saved, &zone)
        );
        for (label, value) in [("From", entry.from.as_str()), ("Saved", saved.as_str())] {
            if value.is_empty() {
                continue;
            }
            ui.label(RichText::new(label.to_uppercase()).font(ty::OVERLINE.font()).color(p.faint));
            ui.label(RichText::new(value).font(ty::BODY.font()).color(p.text));
            ui.add_space(10.0);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (label, heard) = if entry.heard { ("Mark as new", false) } else { ("Mark as heard", true) };
            if widgets::pill(ui, label, Some(Icon::Check), false).clicked() {
                acts.push(Action::Crate(Change::Heard { id: entry.id.clone(), heard }));
            }
            if ui.link(RichText::new("Remove").font(ty::SMALL.font()).color(p.danger)).clicked() {
                acts.push(Action::Crate(Change::Remove { id: entry.id.clone() }));
            }
        });
    }

    /// The entry's note, saved when the field is left.
    fn note_field(&mut self, ui: &mut Ui, entry: &Entry, acts: &mut Actions) {
        let p = colors();
        ui.label(RichText::new("NOTE").font(ty::OVERLINE.font()).color(p.faint));
        let note = &mut self.crate_view.note;
        if note.as_ref().is_none_or(|(id, _)| *id != entry.id) {
            // Another entry was picked while its note was being typed: keep it.
            if let Some((id, text)) = note.take()
                && self.crate_box.get(&id).is_some_and(|e| e.note != text.trim())
            {
                acts.push(Action::Crate(Change::Note { id, note: text.trim().to_string() }));
            }
            *note = Some((entry.id.clone(), entry.note.clone()));
        }
        let Some((_, text)) = note.as_mut() else { return };
        let field = egui::TextEdit::multiline(text)
            .hint_text("Who told you, what to start with…")
            .font(ty::BODY.font())
            .desired_rows(3)
            .desired_width(f32::INFINITY);
        let resp = ui.add(field);
        if resp.lost_focus() && text.trim() != entry.note {
            acts.push(Action::Crate(Change::Note { id: entry.id.clone(), note: text.trim().to_string() }));
        }
    }

    /// "Add to playlist" with the playlist picker, for a track in the Crate.
    fn crate_add_to_playlist(&self, ui: &mut Ui, track: &crate::model::Track, acts: &mut Actions) {
        let filter = ui.scope_id().with("crate-playlist-filter");
        let resp = widgets::pill(ui, "Add to playlist", Some(Icon::Plus), false);
        if resp.clicked() {
            ui.data_mut(|d| d.remove::<String>(filter));
        }
        let mut pick = None;
        egui::Popup::menu(&resp).width(240.0).align(RectAlign::BOTTOM_START).close_behavior(PopupCloseBehavior::CloseOnClickOutside).show(
            |ui| {
                pick = widgets::playlist_picker(ui, self.playlist_choices(), Some(filter));
                if pick.is_some() {
                    ui.close();
                }
            },
        );
        if let Some(choice) = pick {
            acts.push(Action::TrackMenu(vec![track.clone()], choice));
        }
    }
}

/// A one-line field; Enter returns what was typed (and clears it).
fn jot_field(ui: &mut Ui, id: &str, text: &mut String) -> Option<String> {
    jot_field_sized(ui, id, text, ui.available_width())
}

fn jot_field_sized(ui: &mut Ui, id: &str, text: &mut String, width: f32) -> Option<String> {
    let resp = ui.add(
        egui::TextEdit::singleline(text).id_salt(id).hint_text(JOT_HINT).font(ty::BODY.font()).desired_width(width).margin(vec2(10.0, 8.0)),
    );
    let entered = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    (entered && !text.trim().is_empty()).then(|| std::mem::take(text))
}

/// Picture (or a tile for a note), title and what it is, where it's from.
fn entry_row(ui: &Ui, covers: &mut crate::ui::covers::Covers, rect: Rect, entry: &Entry, current: &str) {
    let p = colors();
    let size = metrics::LIST_ART;
    let art = Rect::from_min_size(pos2(rect.left() + 6.0, rect.center().y - size / 2.0), Vec2::splat(size));
    match &entry.thing {
        Thing::Note => widgets::tile(ui, art, Icon::Note, p.raised, radius::THUMB),
        Thing::Artist { .. } => widgets::cover(ui, covers, entry.picture(), metrics::THUMB_PX, art, (size / 2.0) as u8),
        _ => widgets::cover(ui, covers, entry.picture(), metrics::THUMB_PX, art, radius::THUMB),
    }
    let x = art.right() + 12.0;
    let width = rect.right() - x - 8.0;
    let title = if entry.thing == Thing::Note { format!("“{}”", entry.title()) } else { entry.title().to_string() };
    let color = if entry.heard { p.dim } else { p.text };
    text_left(ui.painter(), pos2(x, rect.center().y - 8.0), &title, ty::ITEM, color, width);
    let mut parts: Vec<String> = Vec::new();
    match &entry.thing {
        Thing::Track(_) | Thing::Album { .. } => parts.push(entry.by().to_string()),
        Thing::Artist { .. } => parts.push("artist".into()),
        Thing::Note => parts.push("note".into()),
    }
    if matches!(entry.thing, Thing::Album { .. }) {
        parts.push("album".into());
    }
    if !entry.from.is_empty() {
        parts.push(entry.from.clone());
    }
    if !entry.source.is_empty() && entry.source != current {
        parts.push(format!("on {}", source_name(&entry.source)));
    }
    text_left(ui.painter(), pos2(x, rect.center().y + 10.0), &parts.join(" · "), ty::SMALL, p.dim, width);
}

/// "new" in lime or "heard", and the day it was saved.
fn status_line(ui: &Ui, rect: Rect, entry: &Entry) {
    let p = colors();
    let right = rect.right() - 10.0;
    let (label, color) = if entry.heard { ("heard", p.faint) } else { ("new", p.accent) };
    let y = rect.center().y - 8.0;
    let width = ui.painter().text(pos2(right, y), Align2::RIGHT_CENTER, label, ty::META.font(), color).width();
    ui.painter().circle_filled(pos2(right - width - 8.0, y), 3.0, color);
    let zone = when::zone();
    let day = when::day(entry.saved, &zone).map_or_else(String::new, |d| when::day_name(d, when::today(&zone)));
    ui.painter().text(pos2(right, rect.center().y + 10.0), Align2::RIGHT_CENTER, day, ty::CAPTION.font(), p.faint);
}

/// A round tick: empty, lit on hover, filled once heard.
fn tick_box(ui: &Ui, rect: Rect, on: bool, hovered: bool) {
    let p = colors();
    let center = rect.center();
    let color = if on || hovered { p.accent } else { p.faint };
    if on {
        ui.painter().circle_filled(center, 9.0, p.accent);
    } else {
        ui.painter().circle_stroke(center, 9.0, egui::Stroke::new(1.5, color));
    }
    if on || hovered {
        let ink = if on { p.on_accent } else { p.accent };
        icons::paint(ui.painter(), Rect::from_center_size(center, Vec2::splat(11.0)), Icon::Check, ink);
    }
}

/// "Deezer", "Navidrome": a source's name from its key.
fn source_name(key: &str) -> &'static str {
    crate::source::KINDS.iter().find(|k| k.key == key).map_or("another source", |k| k.name)
}
