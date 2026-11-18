//! Finding a file's tracks at the source in the background: the tracks it links
//! are fetched together first, the rest searched for one by one.

use crate::model::{Error, Track, TrackId};
use crate::source::Source;
use crate::ui::playlist_file::{self, Wanted};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Between searches, so a long playlist doesn't hammer the service.
const PAUSE: Duration = Duration::from_millis(150);
/// Linked tracks fetched per call (a stop takes effect between calls).
const LINKS_PER_FETCH: usize = 100;

/// What became of one wanted track.
#[derive(Debug)]
pub enum Outcome {
    Found(Box<Track>),
    /// Looked for, and the source has nothing playable that fits.
    Missing,
    /// Couldn't be looked for: the source failed.
    Failed,
    /// Not looked for: the search was stopped first.
    Skipped,
}

/// What became of a file's tracks, one outcome each, in its order.
#[derive(Debug)]
pub struct Report {
    pub outcomes: Vec<Outcome>,
    /// Why the source failed (the first such error), when it did.
    pub error: Option<Error>,
    /// The tracks found, each once, in the file's order: the playlist.
    pub tracks: Vec<Track>,
    /// The names of the file's tracks the playlist won't have.
    pub left_out: Vec<String>,
}

impl Report {
    fn new(wanted: &[Wanted], outcomes: Vec<Outcome>, error: Option<Error>) -> Report {
        let mut seen = HashSet::new();
        let tracks = outcomes
            .iter()
            .filter_map(|o| match o {
                Outcome::Found(t) if seen.insert(t.id.clone()) => Some(Track::clone(t)),
                _ => None,
            })
            .collect();
        let left_out = wanted.iter().zip(&outcomes).filter(|(_, o)| !matches!(o, Outcome::Found(_))).map(|(w, _)| w.name()).collect();
        Report { outcomes, error, tracks, left_out }
    }

    /// How many of the file's tracks were found (a track listed twice counts twice).
    pub fn found(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Found(_)))
    }

    pub fn failed(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Failed))
    }

    pub fn skipped(&self) -> usize {
        self.count(|o| matches!(o, Outcome::Skipped))
    }

    fn count(&self, like: impl Fn(&Outcome) -> bool) -> usize {
        self.outcomes.iter().filter(|o| like(o)).count()
    }
}

/// Where tracks are looked up: a source, or a stand-in in tests.
pub trait Lookup {
    /// The track a link names, told without the network.
    fn track_id(&self, link: &str) -> Option<TrackId>;
    /// The playable tracks among `ids`.
    fn tracks(&mut self, ids: &[TrackId]) -> Result<Vec<Track>, Error>;
    fn search(&mut self, query: &str) -> Result<Vec<Track>, Error>;
}

/// A source, searched no faster than one search per [`PAUSE`].
pub struct Paced<'a> {
    client: &'a dyn Source,
    last: Option<Instant>,
}

impl<'a> Paced<'a> {
    pub fn new(client: &'a dyn Source) -> Self {
        Paced { client, last: None }
    }
}

impl Lookup for Paced<'_> {
    fn track_id(&self, link: &str) -> Option<TrackId> {
        self.client.track_id(link)
    }

    fn tracks(&mut self, ids: &[TrackId]) -> Result<Vec<Track>, Error> {
        self.client.tracks(ids)
    }

    fn search(&mut self, query: &str) -> Result<Vec<Track>, Error> {
        if let Some(last) = self.last {
            std::thread::sleep(PAUSE.saturating_sub(last.elapsed()));
        }
        let found = self.client.search(query).map(|r| r.tracks);
        self.last = Some(Instant::now());
        found
    }
}

/// Every wanted track looked up, counting them off in `done`; what's left once
/// `stop` is set is skipped. The tracks the file links are fetched first,
/// together; one the source didn't give (gone, or the fetch failed) is searched
/// for by the name the file gives. Fails when the source failed every time, so a
/// dead connection reads as one error rather than a list of misses, and when the
/// session expired (the rest is for after logging in again).
pub fn find_all(lookup: &mut impl Lookup, wanted: &[Wanted], done: &AtomicUsize, stop: &AtomicBool) -> Result<Report, Error> {
    let mut outcomes: Vec<Option<Outcome>> = wanted.iter().map(|_| None).collect();
    let mut error = None;
    let linked: Vec<(usize, TrackId)> =
        wanted.iter().enumerate().filter_map(|(k, w)| w.link.as_deref().and_then(|link| lookup.track_id(link)).map(|id| (k, id))).collect();
    for batch in linked.chunks(LINKS_PER_FETCH) {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let ids: Vec<TrackId> = batch.iter().map(|(_, id)| id.clone()).collect();
        let fetched = lookup.tracks(&ids);
        for (k, id) in batch {
            outcomes[*k] = match &fetched {
                Ok(tracks) => tracks.iter().find(|t| &t.id == id && t.available).map(|t| Outcome::Found(Box::new(t.clone()))),
                // Without a name to search by, there's nothing more to try.
                Err(_) if wanted[*k].title.is_empty() => Some(Outcome::Failed),
                Err(_) => None,
            };
        }
        if let Err(e) = fetched {
            error.get_or_insert(e);
        }
        done.fetch_add(batch.iter().filter(|(k, _)| outcomes[*k].is_some()).count(), Ordering::Relaxed);
    }
    for (want, outcome) in wanted.iter().zip(&mut outcomes).filter(|(_, o)| o.is_none()) {
        *outcome = Some(if stop.load(Ordering::Relaxed) {
            Outcome::Skipped
        } else {
            let found = search(lookup, want);
            done.fetch_add(1, Ordering::Relaxed);
            match found {
                Ok(Some(track)) => Outcome::Found(Box::new(track)),
                Ok(None) => Outcome::Missing,
                Err(e) => {
                    error.get_or_insert(e);
                    Outcome::Failed
                }
            }
        });
    }
    let outcomes: Vec<Outcome> = outcomes.into_iter().map(|o| o.unwrap_or(Outcome::Skipped)).collect();
    let looked_up = outcomes.iter().filter(|o| !matches!(o, Outcome::Skipped)).count();
    let failed = outcomes.iter().filter(|o| matches!(o, Outcome::Failed)).count();
    match error {
        Some(e) if e.is_session_expired() || (looked_up > 0 && failed == looked_up) => Err(e),
        error => Ok(Report::new(wanted, outcomes, error)),
    }
}

/// The track at the source that `want` names by its title and artist, if any.
fn search(lookup: &mut impl Lookup, want: &Wanted) -> Result<Option<Track>, Error> {
    if want.title.is_empty() {
        return Ok(None);
    }
    let found = lookup.search(&playlist_file::query(want))?;
    if let Some(track) = playlist_file::best(want, &found) {
        return Ok(Some(track.clone()));
    }
    if want.artist.is_empty() {
        return Ok(None);
    }
    // The source may spell the artist differently: search the title alone, still
    // checking the artist.
    let found = lookup.search(&playlist_file::query(&Wanted { artist: String::new(), ..want.clone() }))?;
    Ok(playlist_file::best(want, &found).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Service;
    use crate::ui::playlist_file::fixtures::{song, want};

    /// A source with a few songs, links "link:<id>" to them; its searches and
    /// its fetches by id can be made to fail.
    #[derive(Default)]
    struct Shelf {
        songs: Vec<Track>,
        searches_fail: bool,
        fetches_fail: bool,
        calls: Vec<String>,
    }

    fn offline<T>(fail: bool, then: T) -> Result<T, Error> {
        if fail { Err(Service::new("Shelf", "the shelf").network("offline")) } else { Ok(then) }
    }

    impl Lookup for Shelf {
        fn track_id(&self, link: &str) -> Option<TrackId> {
            link.strip_prefix("link:").map(TrackId::new)
        }

        fn tracks(&mut self, ids: &[TrackId]) -> Result<Vec<Track>, Error> {
            self.calls.push(format!("tracks {}", ids.iter().map(TrackId::as_str).collect::<Vec<_>>().join(",")));
            offline(self.fetches_fail, self.songs.iter().filter(|t| ids.contains(&t.id)).cloned().collect())
        }

        fn search(&mut self, query: &str) -> Result<Vec<Track>, Error> {
            self.calls.push(query.to_string());
            offline(self.searches_fail, self.songs.clone())
        }
    }

    fn linked(id: &str) -> Wanted {
        Wanted { link: Some(format!("link:{id}")), ..Default::default() }
    }

    fn run(shelf: &mut Shelf, wanted: &[Wanted]) -> Result<Report, Error> {
        find_all(shelf, wanted, &AtomicUsize::new(0), &AtomicBool::new(false))
    }

    #[test]
    fn links_are_fetched_together_and_the_files_order_is_kept() {
        let mut shelf = Shelf {
            songs: vec![song("7", "Teardrop", "Massive Attack", "", 330, true), song("42", "Kelly", "Air", "", 230, true)],
            ..Default::default()
        };
        let report =
            run(&mut shelf, &[want("Massive Attack", "Teardrop", 0), linked("42"), want("Nobody", "Nothing", 0), linked("99")]).unwrap();
        assert!(matches!(&report.outcomes[0], Outcome::Found(t) if t.id.as_str() == "7"));
        assert!(matches!(&report.outcomes[1], Outcome::Found(t) if t.title == "Kelly"), "the source's own details");
        assert!(matches!(report.outcomes[2], Outcome::Missing));
        assert!(matches!(report.outcomes[3], Outcome::Missing), "a link to a track that's gone, and no name");
        assert_eq!(shelf.calls, ["tracks 42,99", "massive attack teardrop", "nobody nothing", "nothing"]);
        assert_eq!(report.left_out, ["Nobody - Nothing", "link:99"]);
        assert!(report.error.is_none());
    }

    #[test]
    fn a_track_listed_twice_goes_on_the_playlist_once() {
        let mut shelf = Shelf { songs: vec![song("7", "Teardrop", "Massive Attack", "", 330, true)], ..Default::default() };
        let report = run(&mut shelf, &[want("Massive Attack", "Teardrop", 0), linked("7")]).unwrap();
        assert_eq!(report.found(), 2);
        assert_eq!(report.tracks.len(), 1);
    }

    #[test]
    fn a_dead_or_unplayable_link_is_searched_for_by_name() {
        let mut blocked = song("42", "Kelly", "Air", "", 230, true);
        blocked.available = false;
        let mut shelf = Shelf { songs: vec![blocked, song("43", "Kelly Watch the Stars", "Air", "", 230, true)], ..Default::default() };
        let named = Wanted { link: Some("link:42".into()), ..want("Air", "Kelly Watch the Stars", 230) };
        let report = run(&mut shelf, &[named]).unwrap();
        assert!(matches!(&report.outcomes[0], Outcome::Found(t) if t.id.as_str() == "43"));
        assert_eq!(shelf.calls, ["tracks 42", "air kelly watch the stars"]);
    }

    #[test]
    fn failures_are_told_apart_from_misses() {
        let mut shelf = Shelf { songs: vec![song("42", "Kelly", "Air", "", 230, true)], searches_fail: true, ..Default::default() };
        let report = run(&mut shelf, &[linked("42"), want("Band", "Song", 0)]).unwrap();
        assert_eq!(report.tracks.len(), 1, "what was found is kept");
        assert!(matches!(report.outcomes[1], Outcome::Failed));
        assert!(report.error.is_some(), "and why the rest failed");
    }

    #[test]
    fn a_failed_fetch_of_links_falls_back_to_names() {
        let mut shelf = Shelf { songs: vec![song("43", "Kelly", "Air", "", 230, true)], fetches_fail: true, ..Default::default() };
        let named = Wanted { link: Some("link:42".into()), ..want("Air", "Kelly", 230) };
        let report = run(&mut shelf, &[named, linked("44")]).unwrap();
        assert!(matches!(&report.outcomes[0], Outcome::Found(t) if t.id.as_str() == "43"));
        assert!(matches!(report.outcomes[1], Outcome::Failed), "nothing to search by");
        assert!(report.error.is_some());
    }

    #[test]
    fn nothing_but_failures_is_one_error() {
        let mut shelf = Shelf { searches_fail: true, fetches_fail: true, ..Default::default() };
        assert!(run(&mut shelf, &[linked("42"), want("Band", "Song", 0)]).is_err());
    }

    #[test]
    fn stopping_skips_the_rest_and_done_counts_what_was_looked_up() {
        let mut shelf = Shelf::default();
        let (done, stop) = (AtomicUsize::new(0), AtomicBool::new(true));
        let report = find_all(&mut shelf, &[want("A", "B", 0), want("C", "D", 0)], &done, &stop).unwrap();
        assert!(report.outcomes.iter().all(|o| matches!(o, Outcome::Skipped)));
        assert_eq!(done.load(Ordering::Relaxed), 0);
        assert!(shelf.calls.is_empty());
    }
}
