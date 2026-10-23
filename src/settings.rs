//! Tiny `key=value` settings file in the platform config directory, read and
//! written as typed [`Prefs`].

use crate::deezer::Quality;
use crate::player::Repeat;
use crate::ui::Appearance;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// dust's folder in the platform config directory.
pub fn dir() -> Option<PathBuf> {
    path().and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

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

/// The music source used last time (`source`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourcePref {
    Deezer,
    /// A Navidrome or other Subsonic server.
    Navidrome,
}

impl SourcePref {
    pub fn key(self) -> &'static str {
        match self {
            SourcePref::Deezer => "deezer",
            SourcePref::Navidrome => "navidrome",
        }
    }
}

/// The listener's preferences, as kept in the settings file. Change them with
/// [`Prefs::update`], which saves what changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub source: SourcePref,
    pub quality: Quality,
    pub theme: Appearance,
    /// Send listen reports (Deezer history, Flow, Last.fm; server scrobbles).
    pub report_listens: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// AirPlay speaker (by name) to play on; None for this computer.
    pub output: Option<String>,
    pub volume: f32,
    pub auto_update: bool,
    /// The sidebar's playlist list is shown.
    pub sidebar_playlists: bool,
    /// The settings file changes are saved to; None keeps them in memory.
    file: Option<PathBuf>,
}

impl Prefs {
    /// The saved preferences (defaults for anything missing or unreadable).
    pub fn load() -> Prefs {
        let file = path();
        Prefs { file: file.clone(), ..Prefs::parse(&file.as_deref().map(read).unwrap_or_default()) }
    }

    /// Preferences from a settings file's text, not tied to the file: changes stay
    /// in memory.
    pub fn parse(text: &str) -> Prefs {
        let saved = entries(text);
        let get = |k: &str| saved.get(k).map(String::as_str);
        Prefs {
            source: if get("source") == Some("navidrome") { SourcePref::Navidrome } else { SourcePref::Deezer },
            quality: get("quality").and_then(Quality::from_key).unwrap_or(Quality::Mp3_320),
            theme: get("theme").and_then(Appearance::from_key).unwrap_or(Appearance::System),
            // Switches that are on unless turned off.
            report_listens: get("report_listens") != Some("false"),
            shuffle: get("shuffle") == Some("true"),
            repeat: get("repeat").map_or(Repeat::Off, Repeat::from_key),
            output: get("output").filter(|o| !o.is_empty()).map(str::to_string),
            volume: get("volume").and_then(|v| v.parse::<f32>().ok()).map_or(0.5, |v| v.clamp(0.0, 1.0)),
            auto_update: get("auto_update") != Some("false"),
            sidebar_playlists: get("sidebar_playlists") != Some("closed"),
            file: None,
        }
    }

    /// Change preferences and save the ones that changed. Keys this version
    /// doesn't know, and ones left unchanged, stay in the file as they were.
    pub fn update(&mut self, change: impl FnOnce(&mut Prefs)) {
        let before = self.values();
        change(self);
        let changed: Vec<_> = self.values().into_iter().filter(|v| !before.contains(v)).collect();
        if changed.is_empty() {
            return;
        }
        let Some(p) = &self.file else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(p, merge(&read(p), &changed)) {
            log_warn!("could not save settings to {}: {e}", p.display());
        }
    }

    /// Every preference as its key and saved value.
    fn values(&self) -> [(&'static str, String); 10] {
        let flag = |on: bool| if on { "true" } else { "false" }.to_string();
        [
            ("source", self.source.key().into()),
            ("quality", self.quality.key().into()),
            ("theme", self.theme.key().into()),
            ("report_listens", flag(self.report_listens)),
            ("shuffle", flag(self.shuffle)),
            ("repeat", self.repeat.key().into()),
            ("output", self.output.clone().unwrap_or_default()),
            ("volume", format!("{:.3}", self.volume)),
            ("auto_update", flag(self.auto_update)),
            ("sidebar_playlists", if self.sidebar_playlists { "open" } else { "closed" }.into()),
        ]
    }
}

/// A settings file's text; empty when there is none.
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The `key=value` lines of a settings file.
fn entries(text: &str) -> BTreeMap<String, String> {
    text.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
}

/// A settings file's text with `changes` written into it.
fn merge(text: &str, changes: &[(&str, String)]) -> String {
    let mut all = entries(text);
    for (k, v) in changes {
        all.insert(k.to_string(), v.clone());
    }
    all.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file as dust 0.19 writes it.
    const SAVED: &str = "auto_update=false\noutput=Kitchen\nquality=flac\nrepeat=all\nreport_listens=false\nshuffle=true\n\
                         sidebar_playlists=closed\nsource=navidrome\ntheme=dark\nvolume=0.250\n";

    #[test]
    fn reads_a_saved_file() {
        let p = Prefs::parse(SAVED);
        assert_eq!(p.source, SourcePref::Navidrome);
        assert_eq!(p.quality, Quality::Flac);
        assert_eq!(p.theme, Appearance::Dark);
        assert!(!p.report_listens && p.shuffle && !p.auto_update && !p.sidebar_playlists);
        assert_eq!(p.repeat, Repeat::All);
        assert_eq!(p.output.as_deref(), Some("Kitchen"));
        assert_eq!(p.volume, 0.25);
    }

    #[test]
    fn missing_or_odd_values_get_defaults() {
        let p = Prefs::parse("quality=ogg\nvolume=7\noutput=\nsomething else\n");
        assert_eq!(p.source, SourcePref::Deezer);
        assert_eq!(p.quality, Quality::Mp3_320);
        assert_eq!(p.theme, Appearance::System);
        assert!(p.report_listens && !p.shuffle && p.auto_update && p.sidebar_playlists);
        assert_eq!(p.repeat, Repeat::Off);
        assert_eq!(p.output, None);
        assert_eq!(p.volume, 1.0);
    }

    #[test]
    fn round_trips_through_the_file() {
        let text = merge("", &Prefs::parse(SAVED).values());
        assert_eq!(text, SAVED);
        assert_eq!(Prefs::parse(&text), Prefs::parse(SAVED));
    }

    #[test]
    fn saving_keeps_other_lines() {
        let text = merge("future=1\nquality=flac\n", &[("quality", "mp3_128".into())]);
        assert_eq!(text, "future=1\nquality=mp3_128\n");
    }
}
