//! Files the user hands dust or gets from it: paths typed with "~", and lists
//! saved into Downloads.

use std::io::Write;
use std::path::{Path, PathBuf};

/// The user's home folder.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// A typed path, "~/Music" as the home folder's Music; None when empty.
pub fn expand_home(path: &str) -> Option<PathBuf> {
    if path.is_empty() {
        return None;
    }
    match path.strip_prefix("~/") {
        Some(rest) => Some(home()?.join(rest)),
        None => Some(PathBuf::from(path)),
    }
}

/// The Downloads folder: on Linux the one the desktop names (in its language,
/// in `~/.config/user-dirs.dirs`), else `~/Downloads`.
fn downloads() -> Option<PathBuf> {
    let home = home()?;
    let named = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config")).join("user-dirs.dirs");
    let named = std::fs::read_to_string(named).ok().and_then(|dirs| download_dir(&dirs, &home));
    Some(named.unwrap_or_else(|| home.join("Downloads")))
}

/// `XDG_DOWNLOAD_DIR` in a `user-dirs.dirs` file: `"$HOME/Preuzimanja"` or an
/// absolute path.
fn download_dir(dirs: &str, home: &Path) -> Option<PathBuf> {
    let value = dirs.lines().find_map(|line| line.trim().strip_prefix("XDG_DOWNLOAD_DIR="))?.trim().trim_matches('"');
    match value.strip_prefix("$HOME") {
        Some(rest) => Some(home.join(rest.trim_start_matches('/'))),
        None => Path::new(value).is_absolute().then(|| PathBuf::from(value)),
    }
}

/// Save `text` as `name` in the Downloads folder; the file it became.
pub fn save_to_downloads(name: &str, text: &str) -> std::io::Result<PathBuf> {
    let downloads = downloads().ok_or_else(|| std::io::Error::other("no home folder"))?;
    save_new(&downloads, name, text)
}

/// Save `text` as a new file `name` in `dir`, never over one already there: the
/// second "list.csv" of a day becomes "list-2.csv".
fn save_new(dir: &Path, name: &str, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = Path::new(name);
    let stem = name.file_stem().unwrap_or_default().to_string_lossy();
    let ext = name.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for k in 1u32.. {
        let file = if k == 1 { dir.join(name) } else { dir.join(format!("{stem}-{k}{ext}")) };
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&file) {
            Ok(mut out) => {
                return match out.write_all(text.as_bytes()) {
                    Ok(()) => Ok(file),
                    Err(e) => {
                        // Half a file is no use: take it away again.
                        let _ = std::fs::remove_file(&file);
                        Err(e)
                    }
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!("a free name is found long before u32 runs out")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_paths_expand_the_home_folder() {
        assert_eq!(expand_home("/tmp"), Some(PathBuf::from("/tmp")));
        assert_eq!(expand_home(""), None);
        assert_eq!(expand_home("~/Music"), home().map(|h| h.join("Music")));
    }

    #[test]
    fn the_desktop_names_its_downloads_folder() {
        let home = Path::new("/home/me");
        let dirs = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Radna površina\"\nXDG_DOWNLOAD_DIR=\"$HOME/Preuzimanja\"\n";
        assert_eq!(download_dir(dirs, home), Some(PathBuf::from("/home/me/Preuzimanja")));
        assert_eq!(download_dir("XDG_DOWNLOAD_DIR=\"/data/dl\"", home), Some(PathBuf::from("/data/dl")));
        assert_eq!(download_dir("XDG_MUSIC_DIR=\"$HOME/Music\"", home), None);
    }

    #[test]
    fn a_saved_file_never_replaces_one_already_there() {
        let root = std::env::temp_dir().join(format!("dust-files-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let first = save_new(&root, "list.csv", "one").unwrap();
        let second = save_new(&root, "list.csv", "two").unwrap();
        assert_eq!((first.file_name().unwrap(), second.file_name().unwrap()), ("list.csv".as_ref(), "list-2.csv".as_ref()));
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "two");
        let _ = std::fs::remove_dir_all(&root);
    }
}
