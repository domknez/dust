//! Tiny `key=value` settings file in the platform config directory.

use std::collections::BTreeMap;
use std::path::PathBuf;

fn path() -> Option<PathBuf> {
    let env = |k| std::env::var_os(k).map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else if cfg!(windows) {
        env("APPDATA")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    }?;
    Some(base.join("dust").join("settings.conf"))
}

pub fn load() -> BTreeMap<String, String> {
    let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) else { return BTreeMap::new() };
    text.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
}

pub fn set(key: &str, value: &str) {
    let Some(p) = path() else { return };
    let mut all = load();
    all.insert(key.to_string(), value.to_string());
    let text: String = all.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&p, text) {
        eprintln!("dust: could not save settings to {}: {e}", p.display());
    }
}
