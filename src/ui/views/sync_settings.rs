//! Settings → *Crate and history*: keep them on this computer or sync them through a
//! folder, see how syncing goes, export the Crate and bring back a daily backup.
//! The work behind it (checking the folder, the daily backup) is here too.

use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::crate_store::{Crate, Entry, Thing};
use crate::ui::format::{self, thousands};
use crate::ui::plays::{Plays, when};
use crate::ui::style::icons::Icon;
use crate::ui::style::{colors, typography as ty};
use crate::ui::sync::{self, Log, SyncDir, SyncState};
use crate::ui::widgets;
use eframe::egui::{self, RichText, Ui, vec2};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How the Crate can be exported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Export {
    /// As a Markdown list, to the clipboard.
    Markdown,
    /// As JSON, into Downloads.
    Json,
}

impl App {
    /// Look at the sync folder for other computers' changes, and back up once a day.
    pub(in crate::ui) fn sync_tick(&mut self, ctx: &egui::Context) {
        if !self.sync.due() {
            return;
        }
        self.sync.checked = Some(std::time::Instant::now());
        self.backup_if_due();
        if let Some(dir) = self.sync.dir.clone() {
            if dir.dir.is_dir() {
                self.crate_box.log().fetch_from_icloud();
                self.plays.log().fetch_from_icloud();
                let stamp: Vec<_> = self.crate_box.log().stamp().into_iter().chain(self.plays.log().stamp()).collect();
                if stamp != self.sync.stamp {
                    self.crate_box.reload();
                    self.plays.reload();
                    self.sync.stamp = stamp;
                }
                self.sync.synced = Some(SystemTime::now());
                self.sync.error = None;
            } else {
                self.sync.error = Some(format!("Can't reach {}", dir.dir.display()));
            }
            ctx.request_repaint_after(sync::CHECK_EVERY);
        }
    }

    /// Today's backup of the Crate and the history, if it isn't made yet.
    fn backup_if_due(&mut self) {
        let today = sync::backup_name(when::today(&when::zone()));
        if self.sync.backed_up.as_deref() == Some(today.as_str()) {
            return;
        }
        let Some(root) = self.crate_box.log().backups_dir() else { return };
        let day = root.join(&today);
        if !day.is_dir() && !self.write_backup(&day) {
            return;
        }
        sync::prune_backups(&root);
        self.sync.backed_up = Some(today);
    }

    /// The Crate and the history as they are now, into `dir`.
    fn write_backup(&self, dir: &Path) -> bool {
        sync::write_lines(&dir.join("crate.jsonl"), &self.crate_box.lines())
            && sync::write_lines(&dir.join("history.jsonl"), &self.plays.lines())
    }

    /// Sync through `folder`, or (None) keep things on this computer only.
    pub(in crate::ui) fn set_sync_folder(&mut self, ctx: &egui::Context, folder: Option<PathBuf>) {
        let local = self.queues.dir().map(Path::to_path_buf);
        let sync_dir = match &folder {
            Some(folder) => {
                if self.prefs.device.is_empty() {
                    let device = sync::new_device_name();
                    self.prefs.update(|p| p.device = device);
                }
                let dir = SyncDir::new(folder, &self.prefs.device);
                if let Err(e) = std::fs::create_dir_all(&dir.dir) {
                    self.toasts.error(ctx, format!("Can't use {}: {e}", folder.display()));
                    return;
                }
                Some(dir)
            }
            None => None,
        };
        self.crate_box.move_to(Log::new("crate", local.as_deref(), sync_dir.as_ref()));
        self.plays.move_to(Log::new("history", local.as_deref(), sync_dir.as_ref()));
        let backed_up = self.sync.backed_up.take();
        self.sync = SyncState { backed_up, ..SyncState::new(sync_dir) };
        self.sync_tick(ctx);
        self.prefs.update(|p| p.sync_folder = folder);
        let message =
            if self.sync.dir.is_some() { "Syncing your Crate and history" } else { "Keeping your Crate and history on this computer" };
        self.toasts.info(ctx, message);
    }

    pub(in crate::ui) fn export_crate(&mut self, ctx: &egui::Context, export: Export) {
        let entries = self.crate_box.entries();
        match export {
            Export::Markdown => {
                ctx.copy_text(markdown(entries));
                self.toasts.info(ctx, "Copied your Crate as Markdown");
            }
            Export::Json => {
                let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
                let Some(downloads) = home.map(|h| h.join("Downloads")) else { return };
                let name = format!("dust-crate-{}.json", sync::backup_name(when::today(&when::zone())));
                let file = downloads.join(name);
                let text = serde_json::to_string_pretty(&json!({ "crate": entries.iter().map(entry_json).collect::<Vec<_>>() }));
                match text
                    .map_err(std::io::Error::other)
                    .and_then(|t| std::fs::create_dir_all(&downloads).and_then(|()| std::fs::write(&file, t)))
                {
                    Ok(()) => self.toasts.info(ctx, format!("Saved {} in Downloads", file.file_name().unwrap_or_default().display())),
                    Err(e) => self.toasts.error(ctx, format!("Couldn't save the Crate: {e}")),
                }
            }
        }
    }

    /// Bring back the Crate as it was in the backup in `dir`, and any plays missing
    /// from the history; what's there now is backed up first.
    pub(in crate::ui) fn restore_backup(&mut self, ctx: &egui::Context, dir: PathBuf) {
        let backup_crate = Crate::load(Log::new("crate", Some(&dir), None));
        let backup_plays = Plays::load(Log::new("history", Some(&dir), None));
        let now = jiff::Zoned::now().strftime("%Y-%m-%d before restoring %H.%M").to_string();
        let saved = self.crate_box.log().backups_dir().is_some_and(|root| self.write_backup(&root.join(now)));
        if !saved {
            self.toasts.error(ctx, "Nothing restored: what's there now couldn't be backed up first");
            return;
        }
        self.crate_box.restore(&backup_crate);
        self.plays.restore(&backup_plays);
        let day = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        self.toasts.info(ctx, format!("Your Crate is back as it was on {day}"));
    }

    /// The *Crate and history* section of Settings.
    pub(in crate::ui) fn sync_settings(&self, ui: &mut Ui, acts: &mut Actions) {
        let p = colors();
        widgets::caption(ui, "CRATE AND HISTORY");
        let syncing = self.prefs.sync_folder.clone();
        let open_id = ui.make_persistent_id("sync-places-open");
        let mut places_open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) || syncing.is_some();
        let local_sub = format!("Backed up here every day, the last {} kept", sync::BACKUPS);
        if widgets::menu_row(ui, None, "Keep on this computer only", Some(&local_sub), syncing.is_none(), p.text).clicked()
            && syncing.is_some()
        {
            acts.push(Action::SyncFolder(None));
            places_open = false;
        }
        let sync_sub = "The same Crate and history on every computer";
        if widgets::menu_row(ui, None, "Sync through a folder", Some(sync_sub), syncing.is_some(), p.text).clicked() {
            places_open = !places_open || syncing.is_some();
        }
        ui.data_mut(|d| d.insert_temp(open_id, places_open));
        if places_open {
            ui.indent("sync-places", |ui| self.sync_places(ui, syncing.as_deref(), acts));
        }
        if let Some(dir) = &self.sync.dir {
            ui.add_space(6.0);
            self.sync_status(ui, dir);
        }
        ui.add_space(6.0);
        if widgets::menu_row(ui, None, "Copy Crate as Markdown", Some("A list to paste anywhere"), false, p.text).clicked() {
            acts.push(Action::ExportCrate(Export::Markdown));
        }
        if widgets::menu_row(ui, None, "Save Crate as JSON", Some("Into your Downloads folder"), false, p.text).clicked() {
            acts.push(Action::ExportCrate(Export::Json));
        }
        self.backup_list(ui, acts);
    }

    /// The sync services found here, then any other folder.
    fn sync_places(&self, ui: &mut Ui, syncing: Option<&Path>, acts: &mut Actions) {
        let p = colors();
        let mut known = false;
        for place in sync::places() {
            match &place.path {
                Some(path) => {
                    let on = syncing == Some(path.as_path());
                    known |= on;
                    if widgets::menu_row(ui, None, place.name, Some(&sync::shown_path(path)), on, p.text).clicked() && !on {
                        acts.push(Action::SyncFolder(Some(path.clone())));
                    }
                }
                None => {
                    widgets::menu_row(ui, None, place.name, Some("Not found on this computer"), false, p.faint);
                }
            }
        }
        if let Some(folder) = syncing.filter(|_| !known) {
            widgets::menu_row(ui, None, "Another folder", Some(&sync::shown_path(folder)), true, p.text);
        }
        // Any other folder, typed or pasted.
        let id = ui.make_persistent_id("sync-other-folder");
        let mut typed = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_default();
        ui.horizontal(|ui| {
            let field = egui::TextEdit::singleline(&mut typed)
                .hint_text("Another folder: paste its path")
                .font(ty::SMALL.font())
                .desired_width(ui.available_width() - 60.0)
                .margin(vec2(8.0, 6.0));
            let resp = ui.add(field);
            let entered = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let path = expand_home(typed.trim());
            let usable = path.as_ref().is_some_and(|p| p.is_dir());
            if (ui.add_enabled(usable, egui::Button::new("Use")).clicked() || (entered && usable))
                && let Some(path) = path
            {
                acts.push(Action::SyncFolder(Some(path)));
                typed.clear();
            }
        });
        ui.data_mut(|d| d.insert_temp(id, typed));
    }

    /// When the folder was last read (or why it couldn't be), the computers using
    /// it and how much there is.
    fn sync_status(&self, ui: &mut Ui, dir: &SyncDir) {
        let p = colors();
        egui::Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.line)).corner_radius(10).inner_margin(12).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (dot, line) = match (&self.sync.error, self.sync.synced) {
                (Some(error), _) => (p.danger, error.clone()),
                (None, Some(at)) => (p.accent, format!("Synced {}", ago(at))),
                (None, None) => (p.faint, "Not synced yet".to_string()),
            };
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, dot);
                ui.label(RichText::new(line).font(ty::BODY.font()).color(p.text));
            });
            let devices = dir.devices();
            let used = match devices.len() {
                0 | 1 => "Used on this computer so far".to_string(),
                n => format!("Used on {n} computers: {}", devices.join(", ")),
            };
            ui.label(RichText::new(used).font(ty::SMALL.font()).color(p.dim));
            let counts = format!(
                "{} in Crate · {} in history",
                thousands(self.crate_box.entries().len() as u64),
                thousands(self.plays.len() as u64)
            );
            ui.label(RichText::new(counts).font(ty::META.font()).color(p.faint));
        });
    }

    /// "Restore a backup…", opening the list of daily backups.
    fn backup_list(&self, ui: &mut Ui, acts: &mut Actions) {
        let p = colors();
        let Some(root) = self.crate_box.log().backups_dir() else { return };
        let backups = sync::backups(&root);
        if backups.is_empty() {
            return;
        }
        let open_id = ui.make_persistent_id("backups-open");
        let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
        let sub = "Puts the Crate back as it was that day";
        if widgets::menu_row(ui, Some(Icon::Refresh), "Restore a backup…", Some(sub), false, p.text).clicked() {
            ui.data_mut(|d| d.insert_temp(open_id, !open));
        }
        if open {
            ui.indent("backups", |ui| {
                for (name, path) in backups {
                    let entries = Crate::load(Log::new("crate", Some(&path), None)).entries().len();
                    let sub = format!("{entries} in Crate · what's there now is backed up first");
                    if widgets::menu_row(ui, None, &name, Some(&sub), false, p.text).clicked() {
                        acts.push(Action::RestoreBackup(path));
                        ui.data_mut(|d| d.insert_temp(open_id, false));
                    }
                }
            });
        }
    }
}

/// "~/Music" → the home folder's Music.
fn expand_home(path: &str) -> Option<PathBuf> {
    if path.is_empty() {
        return None;
    }
    match path.strip_prefix("~/") {
        Some(rest) => Some(PathBuf::from(std::env::var_os("HOME")?).join(rest)),
        None => Some(PathBuf::from(path)),
    }
}

/// "just now", "2 minutes ago".
fn ago(at: SystemTime) -> String {
    format::ago(at.elapsed().map_or(0, |d| d.as_secs()))
}

/// The Crate as a Markdown list: new first, then heard.
fn markdown(entries: &[Entry]) -> String {
    let mut text = String::from("# Crate\n");
    for (title, heard) in [("New", false), ("Heard", true)] {
        let part: Vec<&Entry> = entries.iter().filter(|e| e.heard == heard).collect();
        if part.is_empty() {
            continue;
        }
        text += &format!("\n## {title}\n\n");
        for entry in part {
            let mut line = match &entry.thing {
                Thing::Note => format!("- {}", entry.note),
                Thing::Artist { .. } => format!("- **{}** (artist)", entry.title()),
                Thing::Album { about, .. } => {
                    let kind = if about.kind.is_empty() { "album".to_string() } else { about.kind.to_lowercase() };
                    format!("- **{}** by {} ({kind})", entry.title(), entry.by())
                }
                Thing::Track(_) => format!("- **{}** by {}", entry.title(), entry.by()),
            };
            if entry.thing != Thing::Note && !entry.note.is_empty() {
                line += &format!(": {}", entry.note);
            }
            if !entry.from.is_empty() {
                line += &format!(" ({})", entry.from);
            }
            text += &line;
            text.push('\n');
        }
    }
    text
}

fn entry_json(entry: &Entry) -> serde_json::Value {
    json!({
        "kind": entry.thing.kind(),
        "title": entry.title(),
        "by": entry.by(),
        "note": entry.note,
        "from": entry.from,
        "source": entry.source,
        "heard": entry.heard,
        "saved": entry.saved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crate_reads_as_markdown() {
        let mut c = Crate::default();
        let album =
            Thing::Album { id: "1".into(), title: "Wild God".into(), artist: "Nick Cave".into(), picture: None, about: Default::default() };
        c.add("deezer", album, "", "start with Joy");
        c.add("", Thing::Note, "", "that Turkish psych band");
        let text = markdown(c.entries());
        assert_eq!(text, "# Crate\n\n## New\n\n- that Turkish psych band\n- **Wild God** by Nick Cave (album): start with Joy\n");
        assert_eq!(expand_home("/tmp"), Some(PathBuf::from("/tmp")));
        assert_eq!(expand_home(""), None);
    }

    #[test]
    fn nothing_is_restored_unless_what_is_there_is_backed_up_first() {
        let mut t = crate::ui::app::testing::TestApp::new("");
        let ctx = egui::Context::default();
        t.app.crate_note("keep me".into());
        let root = t.app.crate_box.log().backups_dir().unwrap();
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        std::fs::write(&root, "a file where the backups go").unwrap();
        t.app.restore_backup(&ctx, root.join("empty-backup"));
        assert_eq!(t.app.crate_box.entries().len(), 1, "the Crate is as it was");
    }
}
