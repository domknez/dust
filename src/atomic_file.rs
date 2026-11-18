//! Writing a file so that a crash or a full disk mid-write leaves either the old
//! file or the new one, never a cut-off mix: the text goes to a temporary file
//! beside it, which is then renamed over it.

use std::io;
use std::path::Path;

/// Write `contents` to `path`, making its folder if needed.
pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    let written = std::fs::write(&tmp, contents).and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_the_file_and_leaves_nothing_beside_it() {
        let dir = std::env::temp_dir().join(format!("dust-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("deep").join("settings.conf");
        write(&file, "one").unwrap();
        write(&file, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "two");
        assert_eq!(std::fs::read_dir(file.parent().unwrap()).unwrap().count(), 1, "no temporary file left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_keeps_the_old_file() {
        let dir = std::env::temp_dir().join(format!("dust-atomic-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("queue.json");
        write(&file, "old").unwrap();
        std::fs::create_dir_all(dir.join("queue.json.tmp")).unwrap();
        assert!(write(&file, "new").is_err(), "the temporary path is taken");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "old");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
