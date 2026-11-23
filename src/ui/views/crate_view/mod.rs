//! The Crate, where it shows: the *crate* tab of the panel on the right (save what's
//! playing, jot a name down, tick off what you've heard) and its page (everything
//! in it, filtered, one entry in detail). See [`crate::ui::crate_store`].

use crate::model::{Likeable, Track};
use crate::player::{Cmd, Status};
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::crate_store::{About, Change, Crate, Entry, Thing};
use crate::ui::load::Load;
use crate::ui::plays::{flow_label, when};
use crate::ui::shortcuts;
use crate::ui::state::{ArtistView, Coll, CollSource, View};
use crate::ui::style::icons::{self, Icon};
use crate::ui::style::{colors, metrics, radius, typography as ty};
use crate::ui::widgets::{self, text_left};
use eframe::egui::{
    self, Align2, CornerRadius, Key, PopupCloseBehavior, Rect, RectAlign, RichText, Sense, Ui, UiBuilder, Vec2, pos2, vec2,
};
use std::collections::HashSet;

mod jobs;
mod page;
mod panel;

/// What the jot fields say while empty.
const JOT_HINT: &str = "Jot down a name, a link or a note";
const ROW: f32 = 56.0;
/// Tracks played from each new album or artist.
const EACH: usize = 3;
/// The page's entry details, right of the list.
const DETAIL_WIDTH: f32 = 340.0;
/// Below this width the page shows the list or one entry, not both.
const NARROW_PAGE: f32 = 620.0;

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

/// The Crate, what's shown of it, and what's being done with it in the
/// background.
pub struct Crates {
    pub store: Crate,
    pub view: CrateView,
    /// Its new tracks being put together to play.
    pub radio: Load<Vec<Track>>,
    /// What its albums saved without it are (type, year, tracks), being looked
    /// up; and the entries looked up this run, found or not.
    pub about: Load<Vec<(String, About)>>,
    pub about_tried: HashSet<String>,
}

impl Crates {
    pub fn new(store: Crate) -> Self {
        Crates { store, view: CrateView::default(), radio: Load::default(), about: Load::default(), about_tried: HashSet::new() }
    }

    /// Drop what's being done for the source left: its results belong to it.
    pub fn forget_jobs(&mut self) {
        self.radio = Load::default();
        self.about = Load::default();
        // Those being looked up are tried again when their source is back.
        self.about_tried.clear();
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
    /// On a narrow page, the selected entry is shown in place of the list.
    pub open: bool,
}

impl App {
    /// ⌘⇧S / Ctrl+Shift+S: the track playing goes into the Crate.
    pub(in crate::ui) fn crate_shortcut(&self, ctx: &egui::Context, st: &Status, acts: &mut Actions) {
        if ctx.input_mut(|i| i.consume_shortcut(&shortcuts::SAVE_TO_CRATE)) {
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
        let source = client.data_key();
        let message = match self.crates.store.find(source, &thing).map(|e| (e.id.clone(), e.heard)) {
            Some((id, true)) => {
                self.crates.store.change(Change::Heard { id, heard: false });
                "Back in your Crate as new"
            }
            Some(_) => "Already in your Crate",
            None => {
                self.crates.store.add(source, thing, &from, "");
                "Saved to your Crate"
            }
        };
        self.toasts.info(ctx, message);
    }

    pub(in crate::ui) fn crate_note(&mut self, note: String) {
        if !note.trim().is_empty() {
            self.crates.store.add("", Thing::Note, "", &note);
        }
    }

    /// New entries of the source in use that can be played (not notes).
    pub(in crate::ui) fn crate_playable(&self) -> usize {
        let source = self.data_key();
        self.crates.store.entries().iter().filter(|e| !e.heard && e.source == source && e.thing != Thing::Note).count()
    }

    /// Open the Crate page, showing `entry` (or the filter's first).
    pub(in crate::ui) fn open_crate(&mut self, entry: Option<String>, filter: Option<Filter>) {
        if let Some(filter) = filter {
            self.crates.view.filter = filter;
        }
        if entry.is_some() {
            self.crates.view.filter = Filter::All;
            self.crates.view.selected = entry;
            self.crates.view.open = true;
        }
        self.open_view(View::Crate);
    }

    /// Search the source for an entry from another source, or a note.
    pub(in crate::ui) fn search_for(&mut self, query: String) {
        self.browse.search = query;
        self.open_view(View::Search);
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
    text_left(ui.painter(), pos2(x, rect.center().y - widgets::TITLE_ABOVE), &title, ty::ITEM, color, width);
    let mut parts: Vec<String> = Vec::new();
    match &entry.thing {
        Thing::Track(_) | Thing::Album { .. } => parts.push(entry.by().to_string()),
        Thing::Artist { .. } => parts.push("artist".into()),
        Thing::Note => parts.push("note".into()),
    }
    if let Thing::Album { about, .. } = &entry.thing {
        let summary = about.summary();
        parts.push(if summary.is_empty() { "album".into() } else { summary });
    }
    if !entry.from.is_empty() {
        parts.push(entry.from.clone());
    }
    if !entry.source.is_empty() && entry.source != current {
        parts.push(format!("on {}", source_name(&entry.source)));
    }
    text_left(ui.painter(), pos2(x, rect.center().y + widgets::SUBTITLE_BELOW), &parts.join(" · "), ty::SMALL, p.dim, width);
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
    ui.painter().text(pos2(right, rect.center().y + widgets::SUBTITLE_BELOW), Align2::RIGHT_CENTER, day, ty::CAPTION.font(), p.faint);
}

/// The entry's right-click menu: mark it heard (or new again), or remove it.
fn entry_menu(resp: &egui::Response, entry: &Entry) -> Option<Action> {
    widgets::crate_menu(resp, entry.heard).map(|pick| menu_change(pick, entry))
}

/// What a pick in an entry's menu does to it.
fn menu_change(pick: widgets::CrateMenu, entry: &Entry) -> Action {
    let id = entry.id.clone();
    match pick {
        widgets::CrateMenu::Heard(heard) => Action::Crate(Change::Heard { id, heard }),
        widgets::CrateMenu::Remove => Action::Crate(Change::Remove { id }),
    }
}

/// A round tick to mark an entry heard: empty, lit on hover.
fn tick_box(ui: &Ui, rect: Rect, hovered: bool) {
    let p = colors();
    let center = rect.center();
    let color = if hovered { p.accent } else { p.faint };
    ui.painter().circle_stroke(center, 9.0, egui::Stroke::new(1.5, color));
    if hovered {
        icons::paint(ui.painter(), Rect::from_center_size(center, Vec2::splat(11.0)), Icon::Check, p.accent);
    }
}

/// "Deezer", "Navidrome": a source's name from its key.
fn source_name(key: &str) -> &'static str {
    crate::source::KINDS.iter().find(|k| k.key == key.split(':').next().unwrap_or(key)).map_or("another source", |k| k.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app::testing::*;

    #[test]
    fn a_family_members_entries_name_their_source() {
        assert_eq!(source_name("deezer"), "Deezer");
        assert_eq!(source_name("deezer:2"), "Deezer");
        assert_eq!(source_name("navidrome"), "Navidrome");
        assert_eq!(source_name("elsewhere"), "another source");
    }

    #[test]
    fn the_entry_menu_marks_heard_and_removes() {
        let mut t = TestApp::new("");
        t.app.source_ready(deezer());
        let id = t.app.crates.store.add("deezer", Thing::Note, "", "that Turkish psych band");
        let entry = t.app.crates.store.get(&id).cloned().expect("saved");
        assert!(!entry.heard);

        t.update(menu_change(widgets::CrateMenu::Heard(true), &entry));
        assert!(t.app.crates.store.get(&id).is_some_and(|e| e.heard), "marked heard");
        t.update(menu_change(widgets::CrateMenu::Heard(false), &entry));
        assert!(t.app.crates.store.get(&id).is_some_and(|e| !e.heard), "new again");
        t.update(menu_change(widgets::CrateMenu::Remove, &entry));
        assert!(t.app.crates.store.get(&id).is_none(), "removed");
    }

    #[test]
    fn filters_keep_what_they_say() {
        let mut c = Crate::default();
        let note = c.add("", Thing::Note, "", "a note");
        let song = c.add("deezer", Thing::Track(crate::model::Track::test(1)), "", "");
        c.change(Change::Heard { id: song.clone(), heard: true });
        let kept = |f: Filter| c.entries().iter().filter(|e| f.keeps(e)).map(|e| e.id.clone()).collect::<Vec<_>>();
        assert_eq!(kept(Filter::New), std::slice::from_ref(&note));
        assert_eq!(kept(Filter::Heard), std::slice::from_ref(&song));
        assert_eq!(kept(Filter::Kind("track")), [song]);
        assert_eq!(kept(Filter::All).len(), 2);
    }

    #[test]
    fn opening_an_entry_from_the_panel_shows_it_on_a_narrow_page() {
        let mut t = TestApp::new("");
        t.app.source_ready(deezer());
        let id = t.app.crates.store.add("deezer", Thing::Note, "", "a note");
        t.update(Action::OpenCrate(Some(id.clone()), None));
        assert_eq!(t.app.crates.view.selected.as_deref(), Some(id.as_str()));
        assert!(t.app.crates.view.open, "the entry, not the list");
    }
}
