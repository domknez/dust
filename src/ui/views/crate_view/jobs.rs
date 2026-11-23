//! The Crate's background work: putting its new music together to play, and
//! looking up what its albums are.

use super::*;

/// Albums looked up at a time, and the pause between two.
const ABOUT_BATCH: usize = 40;
const ABOUT_PAUSE: std::time::Duration = std::time::Duration::from_millis(200);

/// What an album is, from its page: the page's type and year where it says, else
/// what was known.
fn about_from(known: About, tracks: usize, info: crate::model::AlbumInfo) -> About {
    About {
        kind: if info.kind.is_empty() { known.kind } else { info.kind },
        year: if info.year.is_empty() { known.year } else { info.year },
        tracks: tracks as u32,
    }
}

impl App {
    /// New tracks, albums and artists of the source in use, oldest saved first.
    fn new_to_play(&self) -> Vec<Thing> {
        let source = self.data_key();
        let mut fresh: Vec<&Entry> =
            self.crates.store.entries().iter().filter(|e| !e.heard && e.source == source && e.thing != Thing::Note).collect();
        fresh.sort_by_key(|e| e.saved);
        fresh.into_iter().map(|e| e.thing.clone()).collect()
    }

    /// Play the new ones: each track, and the first tracks of each album and the
    /// popular ones of each artist (fetched in the background).
    pub(in crate::ui) fn play_crate(&mut self) {
        let Some(client) = self.client.clone() else { return };
        let things = self.new_to_play();
        self.crates.radio.start(&self.jobs, move || {
            let mut tracks: Vec<Track> = Vec::new();
            for thing in things {
                let more = match thing {
                    Thing::Track(track) => vec![track],
                    Thing::Album { id, .. } => {
                        client.album(&id).unwrap_or_default().into_iter().filter(|t| t.available).take(EACH).collect()
                    }
                    Thing::Artist { id, .. } => {
                        client.artist_top(&id).unwrap_or_default().into_iter().filter(|t| t.available).take(EACH).collect()
                    }
                    Thing::Note => Vec::new(),
                };
                for track in more {
                    if track.available && !tracks.iter().any(|t| t.id == track.id) {
                        tracks.push(track);
                    }
                }
            }
            if tracks.is_empty() { Err(crate::model::Error::Other("nothing in your Crate can be played".into())) } else { Ok(tracks) }
        });
    }

    /// Start what [`App::play_crate`] put together, once it's ready.
    pub(in crate::ui) fn crate_radio_tick(&mut self, ctx: &egui::Context) {
        match self.crates.radio.poll_take() {
            Some(Ok(tracks)) => {
                let n = tracks.len();
                self.player.send(Cmd::Play(tracks, 0));
                self.toasts.info(ctx, format!("Playing {n} tracks new in your Crate"));
            }
            Some(Err(e)) => self.toasts.info(ctx, format!("Couldn't play your Crate: {e}")),
            None => {}
        }
    }

    /// Look up what the Crate's albums saved without it are (an EP? how many
    /// tracks?) in the background, a batch at a time, and keep it.
    pub(in crate::ui) fn crate_about_tick(&mut self) {
        if let Some(found) = self.crates.about.poll_take() {
            for (id, about) in found.unwrap_or_default() {
                self.crates.store.set_about(&id, about);
            }
        }
        if !self.prefs.crate_on || self.crates.about.is_loading() {
            return;
        }
        let Some(client) = self.client.clone() else { return };
        let source = client.data_key();
        let wanted: Vec<(String, String, About)> = self
            .crates
            .store
            .entries()
            .iter()
            .filter(|e| e.source == source && !self.crates.about_tried.contains(&e.id))
            .filter_map(|e| match &e.thing {
                Thing::Album { id, about, .. } if about.unknown() => Some((e.id.clone(), id.clone(), about.clone())),
                _ => None,
            })
            .take(ABOUT_BATCH)
            .collect();
        if wanted.is_empty() {
            return;
        }
        self.crates.about_tried.extend(wanted.iter().map(|(entry, ..)| entry.clone()));
        self.crates.about.start(&self.jobs, move || {
            let mut found = Vec::new();
            for (k, (entry, album, known)) in wanted.into_iter().enumerate() {
                if k > 0 {
                    std::thread::sleep(ABOUT_PAUSE);
                }
                if let Ok((tracks, info)) = client.album_page(&album) {
                    found.push((entry, about_from(known, tracks.len(), info)));
                }
            }
            Ok(found)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app::testing::*;

    fn album(id: &str, about: About) -> Thing {
        Thing::Album { id: id.into(), title: format!("Album {id}"), artist: "A".into(), picture: None, about }
    }

    #[test]
    fn albums_saved_without_what_they_are_get_looked_up_once() {
        let mut t = TestApp::new("");
        t.app.source_ready(deezer());
        let known = About { kind: "EP".into(), year: "2026".into(), tracks: 5 };
        t.app.crates.store.add("deezer", album("1", known), "", "");
        let new_release = About { kind: "Single".into(), year: "2026".into(), tracks: 0 };
        let wanted = t.app.crates.store.add("deezer", album("2", new_release), "New release · 6 Nov", "");
        t.app.crates.store.add("navidrome", album("3", About::default()), "", "");
        t.app.crates.store.add("deezer", Thing::Note, "", "a note");
        let waiting = t.jobs.waiting();

        t.app.crate_about_tick();
        assert_eq!(t.jobs.waiting(), waiting + 1, "one look for the batch");
        assert_eq!(t.app.crates.about_tried.iter().collect::<Vec<_>>(), [&wanted], "only the unknown album of this source");
        t.app.crates.about = Default::default();
        t.app.crate_about_tick();
        assert_eq!(t.jobs.waiting(), waiting + 1, "not looked up again this run");
    }

    #[test]
    fn an_albums_page_says_what_it_is() {
        let known = About { kind: "Single".into(), year: "2026".into(), tracks: 0 };
        let info = crate::model::AlbumInfo { kind: "EP".into(), ..Default::default() };
        assert_eq!(about_from(known.clone(), 4, info), About { kind: "EP".into(), year: "2026".into(), tracks: 4 });
        let silent = crate::model::AlbumInfo { year: "2025".into(), ..Default::default() };
        assert_eq!(about_from(known, 1, silent), About { kind: "Single".into(), year: "2025".into(), tracks: 1 });
    }
}
