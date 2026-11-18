//! The import dialog: a view per [`Step`], each answering with where to go next.

use super::find::Report;
use super::{KINDS, Step};
use crate::model::Track;
use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::files::expand_home;
use crate::ui::format::count;
use crate::ui::load::Load;
use crate::ui::playlist_edit::Dialog;
use crate::ui::playlist_file::Imported;
use crate::ui::style::{colors, typography as ty};
use crate::ui::widgets;
use eframe::egui::{self, Key, RichText, Ui};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const WIDTH: f32 = 380.0;
const GAP: f32 = 10.0;
/// The tallest the list of playlists in a file gets before it scrolls.
const LISTS_HEIGHT: f32 = 320.0;
/// The same for the tracks the playlist won't have.
const LEFT_OUT_HEIGHT: f32 = 180.0;
/// How often the dialog redraws while tracks are being found.
const PROGRESS: Duration = Duration::from_millis(250);

/// Where the dialog goes from here.
enum Next {
    File(PathBuf),
    /// The file's playlist at this index.
    List(usize),
    Create(String, Vec<Track>),
    Close,
}

impl App {
    /// The import dialog, if one is open (call once per frame).
    pub(in crate::ui) fn import_dialog(&mut self, ctx: &egui::Context, acts: &mut Actions) {
        let Some(mut step) = self.importing.step.take() else { return };
        if matches!(&step, Step::Read { .. }) || matches!(&step, Step::Find { report, .. } if report.is_loading()) {
            ctx.request_repaint_after(PROGRESS);
        }
        let service = self.client.as_ref().map_or("", |c| c.id().name);
        let mut next = None;
        let modal = egui::Modal::new(egui::Id::unique("import-dialog")).frame(widgets::popover_frame(ctx)).show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.spacing_mut().item_spacing.y = GAP;
            ui.label(RichText::new("Import a playlist").font(ty::SECTION_TITLE.font()).color(colors().text));
            next = match &mut step {
                Step::Pick { path, problem } => pick(ui, path, problem.as_deref()),
                Step::Read { path, .. } => reading(ui, path),
                Step::Choose(lists) => choose(ui, lists),
                Step::Find { list, done, stop, report } => finding(ui, list, done.load(Ordering::Relaxed), stop, report, service),
            };
        });
        // A click beside the dialog or Esc closes it, but not over a search's
        // results: those took a while.
        if modal.should_close() && !matches!(step, Step::Find { .. }) {
            next = next.or(Some(Next::Close));
        }
        // Put back as it was: only the step taken next stops its search.
        self.importing.step = Some(step);
        match next {
            None => {}
            Some(Next::File(file)) => self.import_file(&file),
            Some(Next::List(k)) => {
                if let Some(Step::Choose(mut lists)) = self.importing.step.take() {
                    let step = self.find_tracks(lists.swap_remove(k));
                    self.importing.set(Some(step));
                }
            }
            Some(Next::Create(title, tracks)) => {
                self.importing.set(None);
                acts.push(Action::PlaylistDialog(Dialog::Create { title, private: false, tracks }));
            }
            Some(Next::Close) => self.importing.set(None),
        }
    }
}

/// Ask for the file: a path pasted (or a file dropped on the window instead).
fn pick(ui: &mut Ui, path: &mut String, problem: Option<&str>) -> Option<Next> {
    let p = colors();
    ui.label(RichText::new(format!("Drop a playlist file on the window, or paste its path. dust reads {KINDS}.")).color(p.dim));
    ui.add(egui::TextEdit::singleline(path).hint_text("Path to the file").desired_width(f32::INFINITY)).request_focus();
    if let Some(problem) = problem {
        ui.label(RichText::new(problem).color(p.danger));
    }
    // Whether it's there is for the reading to say.
    let file = expand_home(path.trim().trim_matches(['"', '\'']));
    let mut next = None;
    ui.horizontal(|ui| {
        let import = ui.add_enabled(file.is_some(), egui::Button::new(RichText::new("Import").strong())).clicked()
            || ui.input(|i| i.key_pressed(Key::Enter));
        if import {
            next = file.map(Next::File);
        }
        if ui.button("Cancel").clicked() {
            next = Some(Next::Close);
        }
    });
    next
}

/// The file being read.
fn reading(ui: &mut Ui, path: &Path) -> Option<Next> {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().to_string());
    ui.label(RichText::new(format!("Reading {name}…")).color(colors().dim));
    ui.button("Cancel").clicked().then_some(Next::Close)
}

/// Ask which of the file's playlists to import.
fn choose(ui: &mut Ui, lists: &[Imported]) -> Option<Next> {
    let p = colors();
    ui.label(RichText::new("The file holds several playlists. Which one?").color(p.dim));
    let mut next = None;
    egui::ScrollArea::vertical().max_height(LISTS_HEIGHT).show(ui, |ui| {
        for (k, list) in lists.iter().enumerate() {
            if widgets::menu_row(ui, None, &list.title, Some(&count(list.wanted.len(), "track")), false, p.text).clicked() {
                next = Some(Next::List(k));
            }
        }
    });
    if ui.button("Cancel").clicked() {
        next = Some(Next::Close);
    }
    next
}

/// The search under way (`done` tracks looked up so far; Stop keeps what it
/// found), then what it found.
fn finding(ui: &mut Ui, list: &Imported, done: usize, stop: &AtomicBool, report: &Load<Report>, service: &str) -> Option<Next> {
    let p = colors();
    ui.label(RichText::new(&list.title).strong().color(p.text));
    match report {
        Load::Ready(report) => found(ui, list, report, service),
        Load::Failed(e) => {
            ui.label(RichText::new(format!("Couldn't look for the tracks: {e}")).color(p.danger));
            ui.button("Close").clicked().then_some(Next::Close)
        }
        Load::Idle | Load::Loading(_) => {
            let total = list.wanted.len();
            let stopping = stop.load(Ordering::Relaxed);
            let line = if stopping { "Stopping…".to_string() } else { format!("Finding the tracks on {service}… {done} of {total}") };
            ui.label(RichText::new(line).color(p.dim));
            ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).desired_height(6.0));
            if ui.add_enabled(!stopping, egui::Button::new("Stop")).on_hover_text("Keep what's been found so far").clicked() {
                stop.store(true, Ordering::Relaxed);
            }
            None
        }
    }
}

/// What the search found, what it didn't and why, and the way on to a playlist.
fn found(ui: &mut Ui, list: &Imported, report: &Report, service: &str) -> Option<Next> {
    let p = colors();
    let note = |ui: &mut Ui, text: String| ui.label(RichText::new(text).color(p.dim));
    note(ui, format!("Found {} of {} on {service}", report.found(), list.wanted.len()));
    if report.skipped() > 0 {
        note(ui, format!("Stopped before the last {}", count(report.skipped(), "track")));
    }
    let repeats = report.found() - report.tracks.len();
    if repeats > 0 {
        note(ui, format!("{} listed more than once go on once", count(repeats, "track")));
    }
    if let Some(e) = &report.error {
        let failed = report.failed();
        let what = if failed == 1 { "1 track couldn't".to_string() } else { format!("{failed} tracks couldn't") };
        ui.label(RichText::new(format!("{what} be looked up: {e}")).color(p.danger));
    }
    if !report.left_out.is_empty() {
        left_out_list(ui, &report.left_out);
    }
    let mut next = None;
    ui.horizontal(|ui| {
        let create = egui::Button::new(RichText::new("Create playlist…").strong());
        if ui.add_enabled(!report.tracks.is_empty(), create).clicked() {
            next = Some(Next::Create(list.title.clone(), report.tracks.clone()));
        }
        if ui.button("Cancel").clicked() {
            next = Some(Next::Close);
        }
    });
    next
}

/// The tracks the playlist won't have, folded away, with a button to copy them.
fn left_out_list(ui: &mut Ui, names: &[String]) {
    let p = colors();
    let header = RichText::new(format!("Not added ({})", names.len())).color(p.dim);
    egui::CollapsingHeader::new(header).id_salt("import-left-out").show(ui, |ui| {
        egui::ScrollArea::vertical().max_height(LEFT_OUT_HEIGHT).show(ui, |ui| {
            for name in names {
                ui.label(RichText::new(name).font(ty::SMALL.font()).color(p.faint));
            }
        });
        if ui.button("Copy the list").clicked() {
            ui.ctx().copy_text(names.join("\n"));
        }
    });
}
