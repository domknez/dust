//! Importing a playlist from a file (a Spotify export, a CSV, an M3U, a list of
//! "Artist - Title" lines): read it, find each track at the source, then name and
//! create the playlist with what was found. A file dropped on the window starts
//! it too.
//!
//! - [`find`]: looking the tracks up, in the background
//! - [`dialog`]: the dialog, one view per [`Step`]

mod dialog;
mod find;

use crate::model::Error;
use crate::ui::app::App;
use crate::ui::load::Load;
use crate::ui::playlist_file::{self, Imported, Unreadable};
use crate::ui::style::{colors, typography as ty};
use eframe::egui;
use find::{Paced, Report};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Bigger files aren't playlists.
const MAX_FILE: u64 = 20 * 1024 * 1024;
/// The file types, for messages.
const KINDS: &str = "CSV, TSV, M3U, TXT or a Spotify data export (JSON)";

/// Where an import is.
pub enum Step {
    /// Waiting for a file: dropped on the window, or its path pasted.
    Pick { path: String, problem: Option<String> },
    /// Reading the file's playlists, in the background (it can be big).
    Read { path: PathBuf, lists: Load<Vec<Imported>> },
    /// The file holds several playlists: which one.
    Choose(Vec<Imported>),
    /// Finding the list's tracks (`done` of them so far), then what was found.
    Find { list: Imported, done: Arc<AtomicUsize>, stop: Arc<AtomicBool>, report: Load<Report> },
}

/// The import under way, if any. Dropping it stops a search still running.
#[derive(Default)]
pub struct Importing {
    step: Option<Step>,
}

impl Importing {
    /// Leave the step shown, stopping its search.
    fn set(&mut self, step: Option<Step>) {
        if let Some(Step::Find { stop, .. }) = &self.step {
            stop.store(true, Ordering::Relaxed);
        }
        self.step = step;
    }
}

impl Drop for Importing {
    fn drop(&mut self) {
        self.set(None);
    }
}

impl App {
    /// Ask for a playlist file.
    pub(in crate::ui) fn open_import(&mut self) {
        self.importing.set(Some(Step::Pick { path: String::new(), problem: None }));
    }

    /// Read the playlists in `path`, then find the tracks of the only one, or ask
    /// which ([`App::poll_import`] moves on once it's read).
    pub(in crate::ui) fn import_file(&mut self, path: &Path) {
        let mut lists = Load::default();
        let file = path.to_path_buf();
        lists.start(&self.jobs, move || read_playlists(&file).map_err(Error::Other));
        self.importing.set(Some(Step::Read { path: path.to_path_buf(), lists }));
    }

    /// Close the import, stopping its search (another source took over).
    pub(in crate::ui) fn close_import(&mut self) {
        self.importing.set(None);
    }

    /// Move an import on as its background work arrives; a search's failure
    /// joins `failed`, so an expired session ends it as everywhere else.
    pub(in crate::ui) fn poll_import(&mut self, failed: &mut Vec<Error>) {
        let read = match &mut self.importing.step {
            Some(Step::Read { path, lists }) => lists.poll_take().map(|read| (path.display().to_string(), read)),
            Some(Step::Find { report, .. }) => {
                failed.extend(report.poll());
                None
            }
            _ => None,
        };
        let step = match read {
            None => return,
            Some((path, Err(problem))) => Step::Pick { path, problem: Some(problem.to_string()) },
            Some((_, Ok(mut lists))) if lists.len() == 1 => self.find_tracks(lists.remove(0)),
            Some((_, Ok(lists))) => Step::Choose(lists),
        };
        self.importing.set(Some(step));
    }

    /// Start looking for `list`'s tracks at the source.
    fn find_tracks(&self, list: Imported) -> Step {
        let Some(client) = self.client.clone() else {
            return Step::Pick { path: String::new(), problem: Some("Log in to Deezer or a music server first".into()) };
        };
        let done = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let mut report = Load::default();
        let (wanted, counter, stopped) = (list.wanted.clone(), done.clone(), stop.clone());
        report.start(&self.jobs, move || find::find_all(&mut Paced::new(&*client), &wanted, &counter, &stopped));
        Step::Find { list, done, stop, report }
    }

    /// A playlist file dropped on the window is imported; while one is dragged
    /// over it, a hint says so.
    /// Only while no import is open (a stray drop would end it) and not in the
    /// mini player (which has no room for the dialog).
    pub(in crate::ui) fn dropped_files(&mut self, ctx: &egui::Context) {
        if self.client.is_none() || self.importing.step.is_some() || self.mini_player.is_some() {
            return;
        }
        let (dropped, hovering) =
            ctx.input(|i| (i.raw.dropped_files.first().map(|f| f.path().to_path_buf()), !i.raw.hovered_files.is_empty()));
        if hovering {
            let p = colors();
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop-hint")));
            painter.rect_filled(screen, 0.0, p.veil);
            painter.text(screen.center(), egui::Align2::CENTER_CENTER, "Drop to import a playlist", ty::SECTION_TITLE.font(), p.text);
        }
        if let Some(path) = dropped {
            self.import_file(&path);
        }
    }
}

/// The playlists with tracks in the file at `path`, or what's wrong with it.
fn read_playlists(path: &Path) -> Result<Vec<Imported>, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if !playlist_file::readable(path) {
        return Err(format!("{name} isn't a playlist file. dust reads {KINDS}."));
    }
    let size = std::fs::metadata(path).map_err(|e| format!("Couldn't read {name}: {e}"))?.len();
    if size > MAX_FILE {
        return Err(format!("{name} is too big for a playlist"));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read {name}: {e}"))?;
    playlist_file::read(&name, &playlist_file::decode(&bytes)).map_err(|why| match why {
        Unreadable::NoTracks => format!("No tracks found in {name}"),
        Unreadable::NoTitleColumn => {
            format!("{name} has no column of track titles: dust looks for one headed \"Title\" or \"Track Name\".")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app::testing::{self, TestApp};
    use std::path::PathBuf;

    /// Logged in to Deezer, with a folder of its own for files.
    fn logged_in(name: &str) -> (TestApp, PathBuf) {
        let mut t = TestApp::new("");
        t.app.source_ready(testing::deezer());
        let dir = std::env::temp_dir().join(format!("dust-import-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        (t, dir)
    }

    /// Import `path` and let the file be read (the search it starts stays waiting).
    fn import(t: &mut TestApp, path: &Path) {
        t.app.import_file(path);
        t.jobs.run_held();
        t.app.poll_import(&mut Vec::new());
    }

    fn csv(dir: &Path) -> PathBuf {
        let file = dir.join("Road trip.csv");
        std::fs::write(&file, "Track Name,Artist Name(s)\nOne More Time,Daft Punk\n").unwrap();
        file
    }

    #[test]
    fn a_file_with_one_playlist_goes_straight_to_finding_its_tracks() {
        let (mut t, dir) = logged_in("one");
        t.app.import_file(&csv(&dir));
        assert!(matches!(&t.app.importing.step, Some(Step::Read { .. })), "read in the background");
        t.jobs.run_held();
        t.app.poll_import(&mut Vec::new());
        let Some(Step::Find { list, .. }) = &t.app.importing.step else { panic!("not finding") };
        assert_eq!((list.title.as_str(), list.wanted.len()), ("Road trip", 1));
        assert_eq!(t.jobs.waiting(), 1, "the search");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_isnt_a_playlist_says_so() {
        let (mut t, dir) = logged_in("not");
        let problem = |t: &TestApp| match &t.app.importing.step {
            Some(Step::Pick { problem: Some(problem), .. }) => problem.clone(),
            _ => panic!("no problem shown"),
        };
        let file = dir.join("cover.jpg");
        std::fs::write(&file, [0xff, 0xd8]).unwrap();
        import(&mut t, &file);
        assert!(problem(&t).contains("isn't a playlist file"));
        import(&mut t, &dir.join("gone.csv"));
        assert!(problem(&t).starts_with("Couldn't read gone.csv"));
        let empty = dir.join("empty.m3u");
        std::fs::write(&empty, "#EXTM3U\n").unwrap();
        import(&mut t, &empty);
        assert_eq!(problem(&t), "No tracks found in empty.m3u");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn several_playlists_in_a_file_ask_which() {
        let (mut t, dir) = logged_in("several");
        let file = dir.join("Playlist1.json");
        let list = |name: &str| format!(r#"{{"name":"{name}","items":[{{"track":{{"trackName":"Song","artistName":"Band"}}}}]}}"#);
        std::fs::write(&file, format!(r#"{{"playlists":[{},{}]}}"#, list("A"), list("B"))).unwrap();
        import(&mut t, &file);
        assert!(matches!(&t.app.importing.step, Some(Step::Choose(lists)) if lists.len() == 2));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_a_source_the_file_waits() {
        let mut t = TestApp::new("");
        let dir = std::env::temp_dir().join(format!("dust-import-{}-offline", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        import(&mut t, &csv(&dir));
        assert!(matches!(&t.app.importing.step, Some(Step::Pick { problem: Some(p), .. }) if p.starts_with("Log in")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn leaving_the_search_stops_it() {
        let (mut t, dir) = logged_in("close");
        for leave in [App::open_import, App::close_import] {
            import(&mut t, &csv(&dir));
            let Some(Step::Find { stop, .. }) = &t.app.importing.step else { panic!("not finding") };
            let stop = stop.clone();
            leave(&mut t.app);
            assert!(stop.load(Ordering::Relaxed));
        }
        assert!(t.app.importing.step.is_none(), "closed");
        let _ = std::fs::remove_dir_all(dir);
    }
}
