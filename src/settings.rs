//! Tiny `key=value` settings file in the platform config directory, read and
//! written as typed [`Prefs`].

use crate::model::{PlaylistId, Quality};
use crate::player::Repeat;
use crate::source::{KINDS, SourceId};
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

/// The listener's preferences, as kept in the settings file. Change them with
/// [`Prefs::update`], which saves what changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    /// The music source used last time.
    pub source: SourceId,
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
    /// Endless Flow (Radio) leaves out the tracks in the play history.
    pub flow_new_only: bool,
    /// New releases from the artists followed and listened to go in the Crate.
    pub release_alerts: bool,
    /// The Crate is used; off hides it everywhere (what's in it is kept).
    pub crate_on: bool,
    /// Plays are recorded; off also hides what's made from them (what was
    /// recorded is kept).
    pub history_on: bool,
    /// The lyrics page and its button.
    pub lyrics: bool,
    /// AirPlay speakers are looked for on the network.
    pub airplay: bool,
    /// Sidebar pages turned off, by [`crate::ui::Page`] key.
    pub hidden_pages: Vec<String>,
    /// Playlists tracks were last added to, the latest first: "Add to playlist"
    /// offers them on top and ⌘D adds to the first. Ids of every source mix here;
    /// each offers only its own.
    pub recent_playlists: Vec<PlaylistId>,
    /// The folder the Crate and play history are synced through; None keeps them
    /// on this computer only.
    pub sync_folder: Option<PathBuf>,
    /// This computer's name in the sync folder's file names; made when sync is
    /// first turned on.
    pub device: String,
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
            source: get("source").and_then(SourceId::from_key).unwrap_or(KINDS[0]),
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
            flow_new_only: get("flow_new_only") == Some("true"),
            release_alerts: get("release_alerts") != Some("false"),
            crate_on: get("crate") != Some("false"),
            history_on: get("history") != Some("false"),
            lyrics: get("lyrics") != Some("false"),
            airplay: get("airplay") != Some("false"),
            hidden_pages: get("hidden_pages")
                .map(|k| k.split(',').filter(|k| !k.is_empty()).map(str::to_string).collect())
                .unwrap_or_default(),
            recent_playlists: get("recent_playlists")
                .map(|ids| ids.split(',').filter(|id| !id.is_empty()).map(PlaylistId::new).collect())
                .unwrap_or_default(),
            sync_folder: get("sync_folder").filter(|f| !f.is_empty()).map(PathBuf::from),
            device: get("device").unwrap_or_default().to_string(),
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

    /// Flow's *new to me only*, offered with the play history on.
    pub fn new_only(&self) -> Option<bool> {
        self.history_on.then_some(self.flow_new_only)
    }

    /// Note that tracks were just added to `playlist`.
    pub fn used_playlist(&mut self, playlist: PlaylistId) {
        self.update(|p| {
            p.recent_playlists.retain(|id| *id != playlist);
            p.recent_playlists.insert(0, playlist);
            p.recent_playlists.truncate(RECENT_PLAYLISTS);
        });
    }

    /// Every preference as its key and saved value.
    fn values(&self) -> [(&'static str, String); 20] {
        let flag = |on: bool| if on { "true" } else { "false" }.to_string();
        [
            ("source", self.source.key.into()),
            ("quality", self.quality.key().into()),
            ("theme", self.theme.key().into()),
            ("report_listens", flag(self.report_listens)),
            ("shuffle", flag(self.shuffle)),
            ("repeat", self.repeat.key().into()),
            ("output", self.output.clone().unwrap_or_default()),
            ("volume", format!("{:.3}", self.volume)),
            ("auto_update", flag(self.auto_update)),
            ("sidebar_playlists", if self.sidebar_playlists { "open" } else { "closed" }.into()),
            ("flow_new_only", flag(self.flow_new_only)),
            ("release_alerts", flag(self.release_alerts)),
            ("crate", flag(self.crate_on)),
            ("history", flag(self.history_on)),
            ("lyrics", flag(self.lyrics)),
            ("airplay", flag(self.airplay)),
            ("hidden_pages", self.hidden_pages.join(",")),
            ("recent_playlists", self.recent_playlists.iter().map(PlaylistId::as_str).collect::<Vec<_>>().join(",")),
            ("sync_folder", self.sync_folder.as_ref().map(|f| f.display().to_string()).unwrap_or_default()),
            ("device", self.device.clone()),
        ]
    }
}

/// Playlists remembered as used last (a few, so some survive a switch of source).
const RECENT_PLAYLISTS: usize = 6;

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

    /// A file as dust writes it.
    const SAVED: &str = "airplay=false\nauto_update=false\ncrate=false\ndevice=Air-3f2a\nflow_new_only=true\nhidden_pages=podcasts,explore\nhistory=false\nlyrics=false\n\
                         output=Kitchen\nquality=flac\nrecent_playlists=12,ab-3\nrelease_alerts=false\nrepeat=all\n\
                         report_listens=false\nshuffle=true\nsidebar_playlists=closed\nsource=navidrome\n\
                         sync_folder=/Users/me/Dropbox\ntheme=dark\nvolume=0.250\n";

    #[test]
    fn reads_a_saved_file() {
        let p = Prefs::parse(SAVED);
        assert_eq!(p.source, crate::subsonic::ID);
        assert_eq!(p.quality, Quality::Flac);
        assert_eq!(p.theme, Appearance::Dark);
        assert!(!p.report_listens && p.shuffle && !p.auto_update && !p.sidebar_playlists && p.flow_new_only && !p.release_alerts);
        assert!(!p.crate_on && !p.history_on && !p.lyrics && !p.airplay);
        assert_eq!(p.hidden_pages, ["podcasts", "explore"]);
        assert_eq!(p.repeat, Repeat::All);
        assert_eq!(p.output.as_deref(), Some("Kitchen"));
        assert_eq!(p.volume, 0.25);
        assert_eq!(p.recent_playlists, [PlaylistId::new("12"), PlaylistId::new("ab-3")]);
        assert_eq!((p.sync_folder.as_deref(), p.device.as_str()), (Some(Path::new("/Users/me/Dropbox")), "Air-3f2a"));
    }

    #[test]
    fn the_playlist_used_last_comes_first() {
        let mut p = Prefs::parse(SAVED);
        p.used_playlist(PlaylistId::new("ab-3"));
        p.used_playlist(PlaylistId::new("7"));
        assert_eq!(p.recent_playlists, [PlaylistId::new("7"), PlaylistId::new("ab-3"), PlaylistId::new("12")]);
        (0..10).for_each(|n| p.used_playlist(PlaylistId::from(n)));
        assert_eq!(p.recent_playlists.len(), RECENT_PLAYLISTS);
    }

    #[test]
    fn missing_or_odd_values_get_defaults() {
        let p = Prefs::parse("quality=ogg\nvolume=7\noutput=\nsomething else\n");
        assert_eq!(p.source, crate::deezer::ID);
        assert_eq!(p.quality, Quality::Mp3_320);
        assert_eq!(p.theme, Appearance::System);
        assert!(p.report_listens && !p.shuffle && p.auto_update && p.sidebar_playlists && !p.flow_new_only && p.release_alerts);
        assert_eq!(p.repeat, Repeat::Off);
        assert_eq!(p.output, None);
        assert_eq!(p.volume, 1.0);
        assert!(p.recent_playlists.is_empty());
        assert!(p.sync_folder.is_none() && p.device.is_empty());
        assert!(p.crate_on && p.history_on && p.lyrics && p.airplay && p.hidden_pages.is_empty(), "every part of dust is on at first");
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
