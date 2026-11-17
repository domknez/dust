//! New release alerts: every 12 hours dust looks through the discographies of the
//! artists followed and the ones listened to most lately, and puts what came out
//! since (an album, single or EP) in the Crate as new, with a toast. What was found
//! is remembered in `releases-<source>.json` next to the settings, so nothing comes
//! twice.

use crate::model::{ItemKind, Release, Result};
use crate::source::Shared;
use crate::ui::load::{Jobs, Load};
use jiff::ToSpan;
use jiff::civil::Date;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How often to look, seconds.
pub const EVERY: u64 = 12 * 60 * 60;
/// The first look goes this many days back; later ones a month, for releases
/// Deezer lists late.
const FIRST_LOOK_DAYS: i64 = 14;
const LOOK_DAYS: i64 = 30;
/// Most artists looked through each time.
const MOST_ARTISTS: usize = 300;
/// The artists listened to most in the last 90 days, besides the followed ones.
pub const LISTENED: usize = 40;
/// Album ids remembered.
const SEEN: usize = 5000;
/// Between two artists, not to hammer the service.
const PAUSE: Duration = Duration::from_millis(250);

/// A release found, and whose it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub artist: String,
    pub release: Release,
}

/// What a look is given: the artists listened to, (id, name).
pub type Artists = Vec<(String, String)>;

#[derive(Default)]
pub struct Watch {
    file: Option<PathBuf>,
    /// When the last look finished, Unix seconds; 0 before the first.
    checked: u64,
    /// Album ids found before, oldest first.
    seen: Vec<String>,
    running: Load<Vec<Found>>,
}

impl Watch {
    /// The watch of `source`, as remembered in `dir`.
    pub fn load(dir: Option<&Path>, source: &str) -> Self {
        let file = dir.map(|d| d.join(format!("releases-{source}.json")));
        let saved: Value =
            file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let seen = saved["seen"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
        Watch { file, checked: saved["checked"].as_u64().unwrap_or(0), seen, running: Load::Idle }
    }

    fn save(&self) {
        let Some(file) = &self.file else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file, json!({ "checked": self.checked, "seen": self.seen }).to_string());
    }

    /// Time for another look.
    pub fn due(&self, now: u64) -> bool {
        !self.running.is_loading() && now.saturating_sub(self.checked) >= EVERY
    }

    /// Look in the background: the followed artists, then `listened`.
    pub fn start(&mut self, jobs: &Jobs, source: Shared, listened: Artists, today: Date) {
        let days = if self.checked == 0 { FIRST_LOOK_DAYS } else { LOOK_DAYS };
        let since = today.checked_sub(days.days()).unwrap_or(today);
        let seen: HashSet<String> = self.seen.iter().cloned().collect();
        self.running.start(jobs, move || look(&*source, listened, since, today, &seen));
    }

    /// The releases a finished look found (none when it failed; it's tried again
    /// in 12 hours).
    pub fn poll(&mut self, now: u64) -> Option<Vec<Found>> {
        let result = self.running.poll_take()?;
        self.checked = now;
        let found = result.unwrap_or_default();
        self.seen.extend(found.iter().map(|f| f.release.album.id.clone()));
        if self.seen.len() > SEEN {
            self.seen.drain(..self.seen.len() - SEEN);
        }
        self.save();
        Some(found)
    }
}

/// Go through the artists (followed first) and keep their releases from `since`
/// to `today` not seen before.
fn look(source: &dyn crate::source::Source, listened: Artists, since: Date, today: Date, seen: &HashSet<String>) -> Result<Vec<Found>> {
    let followed = source.favorite_artists();
    let followed_ok = followed.is_ok();
    let mut artists: Artists =
        followed.unwrap_or_default().into_iter().filter(|i| i.kind == ItemKind::Artist).map(|i| (i.id, i.title)).collect();
    artists.extend(listened);
    let mut ids = HashSet::new();
    artists.retain(|(id, _)| !id.is_empty() && ids.insert(id.clone()));
    artists.truncate(MOST_ARTISTS);
    let mut found = Vec::new();
    let mut failed = 0;
    for (id, name) in &artists {
        match source.releases(id) {
            Ok(releases) => {
                for release in fresh(releases, since, today, seen) {
                    // An album of two artists watched comes once.
                    if !found.iter().any(|f: &Found| f.release.album.id == release.album.id) {
                        found.push(Found { artist: name.clone(), release });
                    }
                }
            }
            Err(e) => {
                failed += 1;
                // Offline, or logged out: no use going on.
                if failed >= 5 && found.is_empty() {
                    return Err(e);
                }
            }
        }
        std::thread::sleep(PAUSE);
    }
    if !followed_ok && artists.is_empty() {
        return Err(crate::model::Error::Other("couldn't load the artists followed".into()));
    }
    log_info!("new release check: {} artists, {} new", artists.len(), found.len());
    Ok(found)
}

/// The releases dated from `since` to `today` (whole dates only) not in `seen`,
/// newest first.
pub fn fresh(releases: Vec<Release>, since: Date, today: Date, seen: &HashSet<String>) -> Vec<Release> {
    let mut new: Vec<Release> = releases
        .into_iter()
        .filter(|r| r.date.parse::<Date>().is_ok_and(|d| d >= since && d <= today))
        .filter(|r| !seen.contains(&r.album.id))
        .collect();
    new.sort_by(|a, b| b.date.cmp(&a.date));
    new
}

impl crate::ui::app::App {
    /// Each frame: take in what a look found, and start one when it's time.
    pub(in crate::ui) fn release_tick(&mut self, ctx: &eframe::egui::Context) {
        let Some(client) = self.client.clone() else { return };
        let now = jiff::Timestamp::now().as_second().max(0) as u64;
        if let Some(found) = self.releases.poll(now)
            && self.prefs.crate_on
        {
            self.add_releases(ctx, client.id().key, found);
        }
        if !client.features().releases || !self.prefs.release_alerts || !self.prefs.crate_on || !self.releases.due(now) {
            return;
        }
        // With the history off, only the artists followed.
        let listened = if self.prefs.history_on { self.listened_artists(client.id().key, now) } else { Vec::new() };
        let today = crate::ui::plays::when::today(&crate::ui::plays::when::zone());
        self.releases.start(&self.jobs, client, listened, today);
    }

    /// The artists heard most in the last 90 days, by plays heard through.
    pub(in crate::ui) fn listened_artists(&self, source: &str, now: u64) -> Artists {
        let since = now.saturating_sub(90 * 24 * 60 * 60);
        let mut count: std::collections::HashMap<(String, String), u32> = std::collections::HashMap::new();
        for play in self.plays.oldest_first(source).into_iter().filter(|p| p.at >= since && !p.skipped) {
            if let Some(artist) = play.track.artists.first().filter(|a| !a.id.is_empty()) {
                *count.entry((artist.id.clone(), artist.name.clone())).or_default() += 1;
            }
        }
        let mut ranked: Vec<_> = count.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        ranked.into_iter().take(LISTENED).map(|(artist, _)| artist).collect()
    }

    /// Put the releases found in the Crate as new (those not in it already) and say
    /// so.
    fn add_releases(&mut self, ctx: &eframe::egui::Context, source: &str, found: Vec<Found>) {
        use crate::ui::crate_store::{About, Thing};
        let mut added = Vec::new();
        for f in found {
            let album = f.release.album;
            // The card's subtitle is "2026 · EP"; how many tracks is looked up later.
            let kind = album.subtitle.rsplit(" · ").next().unwrap_or_default().to_string();
            let about = About { kind, year: f.release.date.get(..4).unwrap_or_default().to_string(), tracks: 0 };
            let thing = Thing::Album { id: album.id, title: album.title, artist: f.artist.clone(), picture: album.picture, about };
            if self.crate_box.find(source, &thing).is_some() {
                continue;
            }
            let day = f.release.date.parse::<Date>().map(|d| d.strftime("%-d %b").to_string()).unwrap_or_default();
            let title = match &thing {
                Thing::Album { title, .. } => title.clone(),
                _ => String::new(),
            };
            self.crate_box.add(source, thing, &format!("New release · {day}"), "");
            added.push((f.artist, title));
        }
        let message = match added.as_slice() {
            [] => return,
            [(artist, title)] => format!("New from {artist}: {title}. It's in your Crate"),
            more => format!("{} new releases from artists you like are in your Crate", more.len()),
        };
        self.toasts.info(ctx, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Item;
    use jiff::civil::date;

    fn release(id: &str, date: &str) -> Release {
        Release {
            album: Item { kind: ItemKind::Album, id: id.into(), title: format!("Album {id}"), subtitle: String::new(), picture: None },
            date: date.into(),
        }
    }

    #[test]
    fn only_recent_unseen_releases_are_new() {
        let today = date(2026, 11, 16);
        let seen: HashSet<String> = ["3".to_string()].into();
        let all = vec![
            release("1", "2026-11-14"),
            release("2", "2026-10-01"),
            release("3", "2026-11-15"),
            release("4", "2026-11-20"),
            release("5", "2026"),
            release("6", "2026-11-10"),
        ];
        let new = fresh(all, date(2026, 11, 2), today, &seen);
        assert_eq!(
            new.iter().map(|r| r.album.id.as_str()).collect::<Vec<_>>(),
            ["1", "6"],
            "newest first; old, seen, future and undated left out"
        );
    }

    #[test]
    fn what_was_found_is_remembered() {
        let dir = std::env::temp_dir().join(format!("dust-releases-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut watch = Watch::load(Some(&dir), "deezer");
        assert!(watch.due(1_000_000));
        let jobs = Jobs::held();
        let found = vec![Found { artist: "A".into(), release: release("9", "2026-11-14") }];
        let sent = found.clone();
        watch.running.start(&jobs, move || Ok(sent));
        jobs.run_held();
        assert_eq!(watch.poll(1_000_000), Some(found));
        assert!(!watch.due(1_000_000 + 60));
        let again = Watch::load(Some(&dir), "deezer");
        assert_eq!((again.checked, again.seen.as_slice()), (1_000_000, ["9".to_string()].as_slice()));
        assert!(again.due(1_000_000 + EVERY));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
