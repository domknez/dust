//! Where the Crate and the play history are kept, and keeping them the same on every
//! computer: either just here (next to `settings.conf`), or in a folder a sync
//! service keeps in step (iCloud Drive, Dropbox, Syncthing, Google Drive, any).
//!
//! Each is a log, one JSON line per change. In a sync folder every computer writes
//! only its own file (`crate-<computer>.jsonl` in `<folder>/dust/`), so two never
//! write the same file; each reads all of them and merges. A copy of both is put
//! aside every day in `backups/<date>/` next to the settings, the last 14 kept.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Daily backups kept.
pub const BACKUPS: usize = 14;
const BACKUP_DIR: &str = "backups";
/// dust's folder inside the sync folder.
const SYNC_SUBDIR: &str = "dust";
/// iCloud Drive's folder, in the home folder.
const ICLOUD_DRIVE: &str = "Library/Mobile Documents/com~apple~CloudDocs";

/// The sync folder in use, and this computer's name in it.
#[derive(Clone, Debug, PartialEq)]
pub struct SyncDir {
    /// `<chosen folder>/dust`.
    pub dir: PathBuf,
    /// This computer's part of the file names ("MacBook-Air-3f2a").
    pub device: String,
}

impl SyncDir {
    pub fn new(folder: &Path, device: &str) -> Self {
        SyncDir { dir: folder.join(SYNC_SUBDIR), device: device.to_string() }
    }

    /// The computers writing here, by name ("MacBook Air"), this one included.
    pub fn devices(&self) -> Vec<String> {
        let mut names: Vec<String> = files(&self.dir, "crate")
            .into_iter()
            .chain(files(&self.dir, "history"))
            .filter_map(|f| device_of(&f))
            .map(|d| display_name(&d))
            .collect();
        names.sort();
        names.dedup();
        names
    }
}

/// One log: the Crate's or the history's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Log {
    /// "crate", "history": the file names' first part.
    name: &'static str,
    /// The settings folder (the log when not syncing, and the backups).
    local: Option<PathBuf>,
    sync: Option<SyncDir>,
}

impl Log {
    pub fn new(name: &'static str, local: Option<&Path>, sync: Option<&SyncDir>) -> Self {
        Log { name, local: local.map(Path::to_path_buf), sync: sync.cloned() }
    }

    /// The file this computer writes.
    fn own(&self) -> Option<PathBuf> {
        match &self.sync {
            Some(sync) => Some(sync.dir.join(format!("{}-{}.jsonl", self.name, sync.device))),
            None => Some(self.local.as_ref()?.join(format!("{}.jsonl", self.name))),
        }
    }

    /// Every file of the log: this computer's, and in a sync folder every other's.
    fn sources(&self) -> Vec<PathBuf> {
        match &self.sync {
            Some(sync) => files(&sync.dir, self.name),
            None => self.own().into_iter().collect(),
        }
    }

    /// The lines of every file, each file's in order.
    pub fn read(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for file in self.sources() {
            match std::fs::read_to_string(&file) {
                Ok(text) => lines.extend(text.lines().filter(|l| !l.trim().is_empty()).map(str::to_string)),
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => log_warn!("could not read {}: {e}", file.display()),
                Err(_) => {}
            }
        }
        lines
    }

    /// How many lines this computer's file has.
    pub fn own_lines(&self) -> usize {
        self.own().and_then(|f| std::fs::read_to_string(f).ok()).map_or(0, |t| t.lines().filter(|l| !l.trim().is_empty()).count())
    }

    /// The folder written to is there to write in. A sync folder that's gone (its
    /// service not running, a drive not mounted) isn't made again here.
    fn writable(&self) -> bool {
        match &self.sync {
            Some(sync) => sync.dir.parent().is_some_and(Path::is_dir),
            None => true,
        }
    }

    /// Add a line to this computer's file.
    pub fn append(&self, line: &str) -> bool {
        let Some(file) = self.own().filter(|_| self.writable()) else { return false };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let appended = std::fs::OpenOptions::new().create(true).append(true).open(&file).and_then(|mut f| writeln!(f, "{line}"));
        if let Err(e) = &appended {
            log_warn!("could not add to {}: {e}", file.display());
        }
        appended.is_ok()
    }

    /// Replace this computer's file with `lines`.
    pub fn rewrite(&self, lines: &[String]) -> bool {
        self.writable() && self.own().is_some_and(|file| write_lines(&file, lines))
    }

    /// Ask iCloud Drive for the files of this log it keeps only in the cloud (as
    /// `.crate-iMac-0b1c.jsonl.icloud` placeholders) to save space.
    /// Runs `brctl` off the UI thread, waiting for each so none is left a zombie.
    pub fn fetch_from_icloud(&self) {
        let Some(sync) = self.sync.as_ref().filter(|_| cfg!(target_os = "macos")) else { return };
        let Ok(read) = std::fs::read_dir(&sync.dir) else { return };
        let prefix = format!("{}-", self.name);
        let wanted: Vec<PathBuf> = read
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                let real = name.strip_prefix('.')?.strip_suffix(".icloud")?;
                (real.starts_with(&prefix) && real.ends_with(".jsonl")).then(|| sync.dir.join(real))
            })
            .collect();
        if wanted.is_empty() {
            return;
        }
        let asked = std::thread::Builder::new().name("icloud-fetch".into()).spawn(move || {
            for real in wanted {
                if let Err(e) = std::process::Command::new("brctl").arg("download").arg(&real).status() {
                    log_warn!("could not ask iCloud for {}: {e}", real.display());
                }
            }
        });
        if let Err(e) = asked {
            log_warn!("could not ask iCloud for the sync files: {e}");
        }
    }

    /// What the files look like now (names, sizes, times), to notice when another
    /// computer changed one.
    pub fn stamp(&self) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
        self.sources()
            .into_iter()
            .map(|f| {
                let meta = std::fs::metadata(&f).ok();
                let len = meta.as_ref().map_or(0, |m| m.len());
                (f, len, meta.and_then(|m| m.modified().ok()))
            })
            .collect()
    }

    /// Where the backups are kept.
    pub fn backups_dir(&self) -> Option<PathBuf> {
        Some(self.local.as_ref()?.join(BACKUP_DIR))
    }
}

/// Write `lines` to `file` (via a temporary file and a rename, so a crash can't
/// leave half of it).
pub fn write_lines(file: &Path, lines: &[String]) -> bool {
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    match crate::atomic_file::write(file, text) {
        Ok(()) => true,
        Err(e) => {
            log_warn!("could not write {}: {e}", file.display());
            false
        }
    }
}

/// `<name>-*.jsonl` in `dir`.
fn files(dir: &Path, name: &str) -> Vec<PathBuf> {
    let prefix = format!("{name}-");
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".jsonl")))
        .collect();
    files.sort();
    files
}

/// "MacBook-Air-3f2a" from `crate-MacBook-Air-3f2a.jsonl`.
fn device_of(file: &Path) -> Option<String> {
    let stem = file.file_stem()?.to_str()?;
    Some(stem.split_once('-')?.1.to_string())
}

/// "MacBook Air" from "MacBook-Air-3f2a".
pub fn display_name(device: &str) -> String {
    let name = device.rsplit_once('-').map_or(device, |(name, _)| name);
    name.replace('-', " ")
}

/// A name for this computer in sync file names: its name, safe for a file name,
/// and a few random characters in case two computers share it.
pub fn new_device_name() -> String {
    let name = computer_name().unwrap_or_else(|| "computer".into());
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let safe = safe.split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");
    let safe = if safe.is_empty() { "computer".to_string() } else { safe.chars().take(40).collect() };
    format!("{safe}-{:04x}", fastrand::u16(..))
}

fn computer_name() -> Option<String> {
    let output = if cfg!(target_os = "macos") {
        std::process::Command::new("scutil").args(["--get", "ComputerName"]).output()
    } else {
        std::process::Command::new("hostname").output()
    };
    let name = String::from_utf8(output.ok()?.stdout).ok()?.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// A folder a sync service keeps in step, offered in Settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub name: &'static str,
    /// Where it is, when it's on this computer.
    pub path: Option<PathBuf>,
}

/// The sync services' folders dust knows to look for.
pub fn places() -> Vec<Place> {
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from) else {
        return Vec::new();
    };
    let cloud = home.join("Library/CloudStorage");
    let in_cloud = |prefix: &str, inner: &str| -> Option<PathBuf> {
        let mut found: Vec<PathBuf> = std::fs::read_dir(&cloud)
            .ok()?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(prefix)))
            .map(|p| if inner.is_empty() { p } else { p.join(inner) })
            .filter(|p| p.is_dir())
            .collect();
        found.sort();
        found.into_iter().next()
    };
    let dir = |p: PathBuf| p.is_dir().then_some(p);
    let mut places = Vec::new();
    if cfg!(target_os = "macos") {
        places.push(Place { name: "iCloud Drive", path: dir(home.join(ICLOUD_DRIVE)) });
    }
    places.push(Place { name: "Dropbox", path: in_cloud("Dropbox", "").or_else(|| dir(home.join("Dropbox"))) });
    places.push(Place { name: "Syncthing", path: dir(home.join("Sync")) });
    places.push(Place {
        name: "Google Drive",
        path: in_cloud("GoogleDrive-", "My Drive")
            .or_else(|| dir(home.join("Google Drive/My Drive")))
            .or_else(|| dir(home.join("Google Drive"))),
    });
    places.push(Place { name: "OneDrive", path: in_cloud("OneDrive", "").or_else(|| dir(home.join("OneDrive"))) });
    places
}

/// "~/Dropbox › dust": a folder as Settings shows it.
pub fn shown_path(folder: &Path) -> String {
    shown_path_from(folder, std::env::var_os("HOME").map(PathBuf::from).as_deref())
}

/// [`shown_path`] with the home folder at `home`.
fn shown_path_from(folder: &Path, home: Option<&Path>) -> String {
    let icloud = home.map(|h| h.join(ICLOUD_DRIVE));
    let text = match (icloud, home) {
        (Some(icloud), _) if folder.starts_with(&icloud) => {
            let rest = folder.strip_prefix(&icloud).unwrap_or(folder);
            Path::new("iCloud Drive").join(rest).display().to_string()
        }
        (_, Some(home)) if folder.starts_with(home) => format!("~/{}", folder.strip_prefix(home).unwrap_or(folder).display()),
        _ => folder.display().to_string(),
    };
    format!("{} › {SYNC_SUBDIR}", text.trim_end_matches('/'))
}

// ------------------------------------------------------------ state

/// How often the sync folder is looked at for other computers' changes, and the
/// day checked for its backup.
pub const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(15);

/// Syncing as it goes: the folder, when it was last read, what went wrong.
#[derive(Default)]
pub struct SyncState {
    /// The sync folder in use; None keeps things on this computer only.
    pub dir: Option<SyncDir>,
    /// When the folder was last looked at.
    pub checked: Option<std::time::Instant>,
    /// The logs' files as last read.
    pub stamp: Vec<(PathBuf, u64, Option<SystemTime>)>,
    /// When the folder was last read fine.
    pub synced: Option<SystemTime>,
    /// Why it couldn't be, if it couldn't.
    pub error: Option<String>,
    /// The day last backed up ("2026-11-16").
    pub backed_up: Option<String>,
    /// What Settings shows of the folders, read with each check rather than on
    /// every frame it's drawn.
    pub seen: Seen,
}

/// The sync services found here, the computers writing to the sync folder, and
/// the backups kept.
#[derive(Default)]
pub struct Seen {
    pub places: Vec<Place>,
    pub devices: Vec<String>,
    pub backups: Vec<Backup>,
}

/// A daily backup, and how many Crate entries it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Backup {
    pub name: String,
    pub path: PathBuf,
    pub entries: usize,
}

impl Seen {
    /// Look again. `crate_entries` counts a backup's Crate; each backup is counted
    /// once (they don't change once written).
    pub fn refresh(&mut self, dir: Option<&SyncDir>, backups_dir: Option<&Path>, crate_entries: impl Fn(&Path) -> usize) {
        self.places = places();
        self.devices = dir.map(SyncDir::devices).unwrap_or_default();
        let known = std::mem::take(&mut self.backups);
        self.backups = backups_dir
            .map(backups)
            .unwrap_or_default()
            .into_iter()
            .map(|(name, path)| match known.iter().find(|b| b.name == name) {
                Some(b) => b.clone(),
                None => Backup { entries: crate_entries(&path), name, path },
            })
            .collect();
    }
}

impl SyncState {
    pub fn new(dir: Option<SyncDir>) -> Self {
        SyncState { dir, ..Default::default() }
    }

    /// Look again at once, not at the next check.
    pub fn check_soon(&mut self) {
        self.checked = None;
    }

    /// Time to look again.
    pub fn due(&self) -> bool {
        self.checked.is_none_or(|t| t.elapsed() >= CHECK_EVERY)
    }
}

// ------------------------------------------------------------ backups

/// The day's backup folder name: "2026-11-16".
pub fn backup_name(day: jiff::civil::Date) -> String {
    day.to_string()
}

/// The backups kept, newest first: (folder name, path).
pub fn backups(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<(String, PathBuf)> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .filter_map(|p| Some((p.file_name()?.to_str()?.to_string(), p)))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found
}

/// Remove all but the newest [`BACKUPS`] daily backups.
pub fn prune_backups(dir: &Path) {
    for (name, path) in backups(dir).into_iter().skip(BACKUPS) {
        if let Err(e) = std::fs::remove_dir_all(&path) {
            log_warn!("could not remove the old backup {name}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dust-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn each_computer_writes_its_own_file_and_reads_all() {
        let root = dir("own");
        std::fs::create_dir_all(&root).unwrap();
        let air = Log::new("crate", Some(&root.join("air-settings")), Some(&SyncDir::new(&root, "MacBook-Air-3f2a")));
        let imac = Log::new("crate", Some(&root.join("imac-settings")), Some(&SyncDir::new(&root, "iMac-0b1c")));
        air.append("a1");
        imac.append("i1");
        air.append("a2");
        let mut all = air.read();
        all.sort();
        assert_eq!(all, ["a1", "a2", "i1"]);
        assert_eq!(air.own_lines(), 2);
        assert!(root.join("dust/crate-iMac-0b1c.jsonl").is_file());
        assert_eq!(SyncDir::new(&root, "x").devices(), ["MacBook Air", "iMac"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_sync_folder_is_not_made_again() {
        let root = dir("missing");
        let gone = root.join("Dropbox");
        let log = Log::new("crate", Some(&root.join("settings")), Some(&SyncDir::new(&gone, "Air-0001")));
        assert!(!log.append("a1"));
        assert!(!log.rewrite(&["a1".into()]));
        assert!(!gone.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn device_names_are_safe_in_file_names() {
        let name = new_device_name();
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "{name}");
        assert_eq!(display_name("Domagoj-s-MacBook-Pro-3f2a"), "Domagoj s MacBook Pro");
    }

    // Unix paths (the folder separators differ on Windows).
    #[cfg(unix)]
    #[test]
    fn folders_read_short() {
        let home = Path::new("/Users/me");
        let shown = |folder: &str| shown_path_from(Path::new(folder), Some(home));
        assert_eq!(shown("/Users/me/Library/Mobile Documents/com~apple~CloudDocs/Music"), "iCloud Drive/Music › dust");
        assert_eq!(shown("/Users/me/Dropbox/"), "~/Dropbox › dust");
        assert_eq!(shown("/Volumes/NAS/sync"), "/Volumes/NAS/sync › dust");
    }

    #[test]
    fn each_backup_is_counted_once() {
        let root = dir("seen");
        std::fs::create_dir_all(root.join("2026-11-17")).unwrap();
        let counted = std::cell::Cell::new(0);
        let count = |_: &Path| {
            counted.set(counted.get() + 1);
            7
        };
        let mut seen = Seen::default();
        seen.refresh(None, Some(&root), count);
        std::fs::create_dir_all(root.join("2026-11-18")).unwrap();
        seen.refresh(None, Some(&root), count);
        let names: Vec<_> = seen.backups.iter().map(|b| (b.name.as_str(), b.entries)).collect();
        assert_eq!(names, [("2026-11-18", 7), ("2026-11-17", 7)], "newest first");
        assert_eq!(counted.get(), 2, "the older one wasn't read again");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_the_newest_backups_stay() {
        let root = dir("backups");
        for day in 1..=20 {
            std::fs::create_dir_all(root.join(format!("2026-10-{day:02}"))).unwrap();
        }
        prune_backups(&root);
        let kept = backups(&root);
        assert_eq!(kept.len(), BACKUPS);
        assert_eq!(kept[0].0, "2026-10-20");
        assert_eq!(kept.last().unwrap().0, "2026-10-07");
        let _ = std::fs::remove_dir_all(&root);
    }
}
