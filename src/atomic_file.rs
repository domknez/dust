//! Writing a file so that a crash, a full disk or a power cut mid-write leaves
//! either the old file or the new one, never a cut-off mix: the text goes to a
//! temporary file beside it, is flushed to the disk, then renamed over it.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Write `contents` to `path`, making its folder if needed. A symlink at `path`
/// is written through: the file it points at is replaced, the link kept.
pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = temporary(&path);
    let written = write_synced(&tmp, contents.as_ref()).and_then(|()| std::fs::rename(&tmp, &path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// `path` with `.tmp` added: beside it, so the rename stays on one disk.
fn temporary(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

/// Write `contents` to a new `file` and wait for it to reach the disk.
fn write_synced(file: &Path, contents: &[u8]) -> io::Result<()> {
    let mut f = std::fs::File::create(file)?;
    f.write_all(contents)?;
    f.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dust-atomic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn replaces_the_file_and_leaves_nothing_beside_it() {
        let dir = dir("replace");
        let file = dir.join("deep").join("settings.conf");
        write(&file, "one").unwrap();
        write(&file, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "two");
        assert_eq!(std::fs::read_dir(file.parent().unwrap()).unwrap().count(), 1, "no temporary file left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_keeps_the_old_file() {
        let dir = dir("fail");
        let file = dir.join("queue.json");
        write(&file, "old").unwrap();
        std::fs::create_dir_all(dir.join("queue.json.tmp")).unwrap();
        assert!(write(&file, "new").is_err(), "the temporary path is taken");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_stays_a_link() {
        let dir = dir("link");
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("dotfiles-settings.conf");
        let link = dir.join("settings.conf");
        std::fs::write(&real, "old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write(&link, "new").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "the link is kept");
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "new", "and what it points at is written");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
