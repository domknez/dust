//! Artists' genres for their pages, looked up in the background (see
//! [`crate::musicbrainz`]) and kept by artist name for the session. Only the
//! artist on screen is wanted: a lookup for one left before its answer stops
//! before its next request, and starts over if the page is opened again.

use super::load::Jobs;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

type Fetch = fn(&str, &dyn Fn() -> bool) -> Result<Option<Vec<String>>, String>;

#[derive(Default)]
struct Shared {
    /// Genres by artist name; empty when none were found, or the lookup failed
    /// (not tried again before dust restarts, so MusicBrainz isn't pestered).
    known: HashMap<String, Vec<String>>,
    /// Artists being looked up.
    asking: HashSet<String>,
    /// The artist whose page is on screen.
    wanted: String,
}

pub struct ArtistGenres {
    shared: Arc<Mutex<Shared>>,
    fetch: Fetch,
}

impl Default for ArtistGenres {
    fn default() -> Self {
        ArtistGenres::with(crate::musicbrainz::artist_genres)
    }
}

impl ArtistGenres {
    fn with(fetch: Fetch) -> Self {
        ArtistGenres { shared: Arc::default(), fetch }
    }

    /// The genres of the artist named `name`, whose page is on screen; empty until
    /// known. The first call asks for them.
    pub fn of(&self, jobs: &Jobs, name: &str) -> Vec<String> {
        if name.is_empty() {
            return Vec::new();
        }
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(genres) = shared.known.get(name) {
            return genres.clone();
        }
        if shared.wanted != name {
            shared.wanted = name.to_string();
        }
        if shared.asking.insert(name.to_string()) {
            let (fetch, all, who) = (self.fetch, self.shared.clone(), name.to_string());
            jobs.run(move || {
                let wanted = || all.lock().unwrap_or_else(PoisonError::into_inner).wanted == who;
                let result = fetch(&who, &wanted);
                let mut shared = all.lock().unwrap_or_else(PoisonError::into_inner);
                shared.asking.remove(&who);
                let genres = match result {
                    Ok(Some(genres)) => genres,
                    Ok(None) => return,
                    Err(e) => {
                        log_warn!("genres of {who}: {e}");
                        Vec::new()
                    }
                };
                shared.known.insert(who, genres);
            });
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Like MusicBrainz: asks whether still wanted before answering.
    fn found(name: &str, wanted: &dyn Fn() -> bool) -> Result<Option<Vec<String>>, String> {
        if !wanted() {
            return Ok(None);
        }
        match name {
            "Radiohead" => Ok(Some(vec!["alternative rock".into(), "art rock".into()])),
            "Nobody" => Ok(Some(Vec::new())),
            _ => Err("offline".into()),
        }
    }

    #[test]
    fn each_artist_is_asked_for_once() {
        let jobs = Jobs::held();
        let genres = ArtistGenres::with(found);
        assert!(genres.of(&jobs, "Radiohead").is_empty(), "not known yet");
        assert!(genres.of(&jobs, "Radiohead").is_empty());
        assert_eq!(jobs.waiting(), 1, "asked once while on its way");
        jobs.run_held();
        assert_eq!(genres.of(&jobs, "Radiohead"), ["alternative rock", "art rock"]);
        assert_eq!(jobs.waiting(), 0, "known: not asked again");
        assert!(genres.of(&jobs, "").is_empty());
        assert_eq!(jobs.waiting(), 0, "no name, nothing to ask");
    }

    #[test]
    fn a_page_left_stops_its_lookup_and_failures_are_not_retried() {
        let jobs = Jobs::held();
        let genres = ArtistGenres::with(found);
        genres.of(&jobs, "Radiohead");
        genres.of(&jobs, "Offline");
        jobs.run_held();
        assert!(genres.of(&jobs, "Offline").is_empty());
        assert_eq!(jobs.waiting(), 0, "a failed lookup isn't tried again");
        genres.of(&jobs, "Radiohead");
        assert_eq!(jobs.waiting(), 1, "the one left gave up, so it's asked for again");
        jobs.run_held();
        assert_eq!(genres.of(&jobs, "Radiohead").len(), 2);
    }
}
