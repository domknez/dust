//! Shuffled order that feels random to a listener: tracks not heard for a while
//! (or ever) tend to come first, and the same artist doesn't play twice in a row
//! where the list allows it. Pure, so it is unit-testable with a seeded generator.

use super::LastHeard;
use crate::model::Track;
use std::collections::{HashMap, VecDeque};

/// From this long ago on, a track counts as much as one never heard.
const FORGOTTEN_SECS: u64 = 90 * 24 * 60 * 60;
/// How much likelier a forgotten (or never heard) track is to come early than one
/// heard a moment ago.
const FORGOTTEN_WEIGHT: f64 = 4.0;

/// Put `tracks` in shuffled order. `after` is the track playing before them (the
/// current one), whose artist the first of them avoids; `heard` says when each
/// track was last heard, as of `now` (Unix seconds).
pub fn shuffle(tracks: &mut Vec<Track>, after: Option<&Track>, heard: Option<&LastHeard>, now: u64, rng: &mut fastrand::Rng) {
    let mut keyed: Vec<(f64, Track)> = tracks.drain(..).map(|t| (key(weight(&t, heard, now), rng), t)).collect();
    keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
    let ordered: Vec<Track> = keyed.into_iter().map(|(_, t)| t).collect();
    *tracks = spread_artists(ordered, after);
}

/// How strongly `track` wants to come early: 1 when just heard, rising evenly to
/// [`FORGOTTEN_WEIGHT`] once [`FORGOTTEN_SECS`] have passed or when never heard.
fn weight(track: &Track, heard: Option<&LastHeard>, now: u64) -> f64 {
    let Some(&at) = heard.and_then(|h| h.get(&track.id)) else { return FORGOTTEN_WEIGHT };
    let ago = now.saturating_sub(at).min(FORGOTTEN_SECS) as f64 / FORGOTTEN_SECS as f64;
    1.0 + (FORGOTTEN_WEIGHT - 1.0) * ago
}

/// A random sort key for weighted shuffling (Efraimidis–Spirakis): sorting by it,
/// highest first, puts each track first with a chance proportional to its weight.
fn key(weight: f64, rng: &mut fastrand::Rng) -> f64 {
    rng.f64().powf(1.0 / weight)
}

/// `ordered` again with no two neighbours by the same artist (nor the first by
/// `after`'s) where that can be done: each next track is the first of the rest by
/// another artist than the one before, unless an artist holds more than half of
/// what's left: then theirs comes now, or the others couldn't keep them apart.
fn spread_artists(ordered: Vec<Track>, after: Option<&Track>) -> Vec<Track> {
    let mut left: HashMap<Artist, usize> = HashMap::new();
    for artist in ordered.iter().filter_map(artist) {
        *left.entry(artist).or_default() += 1;
    }
    // Tracks by no known artist, which can go between any two.
    let mut unknown = ordered.len() - left.values().sum::<usize>();
    // Taken mostly from the front, so a deque keeps it linear.
    let mut rest = VecDeque::from(ordered);
    let mut out = Vec::with_capacity(rest.len());
    let mut last = after.and_then(artist);
    while left.len() > 1 || (left.len() == 1 && unknown > 0) {
        let crowded = left.iter().find(|&(a, &n)| 2 * n > rest.len() && last.as_ref() != Some(a)).map(|(a, _)| a.clone());
        let fits = |t: &Track| {
            let who = artist(t);
            match &crowded {
                Some(_) => who == crowded,
                None => who.is_none() || who != last,
            }
        };
        let Some(track) = rest.iter().position(fits).and_then(|next| rest.remove(next)) else { break };
        last = artist(&track);
        match &last {
            Some(who) => {
                if let Some(n) = left.get_mut(who) {
                    *n -= 1;
                    if *n == 0 {
                        left.remove(who);
                    }
                }
            }
            None => unknown -= 1,
        }
        out.push(track);
    }
    // One artist left with nothing to put between them, or none: kept as they are.
    out.extend(rest);
    out
}

/// Who a track is by, for telling artists apart: the main artist's id, or the name
/// where the source gave no ids. None when unknown: then it's no one's neighbour.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Artist {
    Id(String),
    Name(String),
}

fn artist(track: &Track) -> Option<Artist> {
    match track.artists.first() {
        Some(a) if !a.id.is_empty() => Some(Artist::Id(a.id.clone())),
        _ if !track.artist.is_empty() => Some(Artist::Name(track.artist.to_lowercase())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ArtistRef;

    fn by(id: u64, artist: &str) -> Track {
        Track { artist: artist.into(), artists: vec![ArtistRef { id: artist.into(), name: artist.into() }], ..Track::test(id) }
    }

    fn number(t: &Track) -> u64 {
        t.id.as_str().parse().unwrap()
    }

    /// Each track's artist name, as the tests make them.
    fn artists(tracks: &[Track]) -> Vec<&str> {
        tracks.iter().map(|t| t.artist.as_str()).collect()
    }

    const NOW: u64 = 1_800_000_000;
    const DAY: u64 = 24 * 60 * 60;

    #[test]
    fn no_artist_twice_in_a_row_when_it_can_be_helped() {
        for seed in 0..200 {
            let mut tracks: Vec<Track> = (0..12).map(|i| by(i, ["a", "b", "c"][i as usize % 3])).collect();
            shuffle(&mut tracks, Some(&by(99, "a")), None, NOW, &mut fastrand::Rng::with_seed(seed));
            let order = artists(&tracks);
            assert_ne!(order[0], "a", "not the playing track's artist first (seed {seed})");
            assert!(order.windows(2).all(|w| w[0] != w[1]), "{order:?} (seed {seed})");
            assert_eq!(tracks.len(), 12);
        }
    }

    #[test]
    fn any_mix_of_artists_is_kept_apart_when_it_can_be() {
        let mut rng = fastrand::Rng::with_seed(1);
        for _ in 0..2000 {
            let len = rng.usize(1..16);
            let artists_n = rng.usize(1..5);
            let mut tracks: Vec<Track> = (0..len as u64).map(|i| by(i, &format!("a{}", rng.usize(..artists_n)))).collect();
            let after = by(99, &format!("a{}", rng.usize(..artists_n)));
            let most = (0..artists_n).map(|a| tracks.iter().filter(|t| t.artist == format!("a{a}")).count()).max().unwrap();
            let of_after = tracks.iter().filter(|t| t.artist == after.artist).count();
            // Possible: the commonest artist fits between the others, and the
            // playing one's too, without coming first.
            let possible = most <= len.div_ceil(2) && of_after <= len / 2;
            shuffle(&mut tracks, Some(&after), None, NOW, &mut rng);
            let clashes =
                std::iter::once(&after).chain(&tracks).collect::<Vec<_>>().windows(2).filter(|w| w[0].artist == w[1].artist).count();
            if possible {
                assert_eq!(clashes, 0, "{:?} after {}", artists(&tracks), after.artist);
            }
            assert_eq!(tracks.len(), len);
        }
    }

    #[test]
    fn one_artist_left_plays_on() {
        let mut tracks: Vec<Track> = [by(1, "a"), by(2, "a"), by(3, "a"), by(4, "b")].into();
        shuffle(&mut tracks, None, None, NOW, &mut fastrand::Rng::with_seed(7));
        let mut ids: Vec<u64> = tracks.iter().map(number).collect();
        ids.sort();
        assert_eq!(ids, [1, 2, 3, 4], "every track kept");
        assert_eq!(artists(&tracks).iter().filter(|a| **a == "a").count(), 3);
        assert_ne!(artists(&tracks)[..2], ["a", "a"], "b comes between the first two of a");
    }

    #[test]
    fn artists_are_told_apart_by_id_then_by_name() {
        let mut no_ids = by(1, "X");
        no_ids.artists.clear();
        assert_eq!(artist(&no_ids), Some(Artist::Name("x".into())));
        assert_ne!(artist(&no_ids), artist(&by(2, "x")), "an id is never a name");
        no_ids.artist.clear();
        assert_eq!(artist(&no_ids), None);

        // Unknown artists sit anywhere, next to each other too.
        let unknown = |id| Track { artist: String::new(), ..Track::test(id) };
        let mut tracks = vec![unknown(1), unknown(2), by(3, "a"), by(4, "a")];
        shuffle(&mut tracks, None, None, NOW, &mut fastrand::Rng::with_seed(5));
        assert_eq!(tracks.len(), 4);
        assert!(!artists(&tracks).windows(2).any(|w| w == ["a", "a"]), "the unknown ones keep the two by a apart: {:?}", artists(&tracks));
    }

    #[test]
    fn long_unheard_tracks_tend_to_come_first() {
        let heard: LastHeard =
            (0..10).map(|i| (by(i, "").id, NOW - DAY / 2)).chain((10..20).map(|i| (by(i, "").id, NOW - 200 * DAY))).collect();
        let (mut fresh_first, runs) = (0, 2000);
        for seed in 0..runs {
            // Every track by another artist, so only the weights decide.
            let mut tracks: Vec<Track> = (0..20).map(|i| by(i, &format!("a{i}"))).collect();
            shuffle(&mut tracks, None, Some(&heard), NOW, &mut fastrand::Rng::with_seed(seed));
            fresh_first += usize::from(number(&tracks[0]) >= 10);
        }
        // Weights about 1.02 against 4: the long-unheard half starts ~80% of the time.
        let share = fresh_first as f64 / runs as f64;
        assert!((0.72..0.88).contains(&share), "{share}");
    }

    #[test]
    fn weight_rises_with_time_since_heard() {
        let t = by(1, "a");
        let heard = |ago: u64| -> LastHeard { [(t.id.clone(), NOW - ago)].into() };
        assert_eq!(weight(&t, None, NOW), FORGOTTEN_WEIGHT, "history off: all alike");
        assert_eq!(weight(&t, Some(&LastHeard::new()), NOW), FORGOTTEN_WEIGHT, "never heard");
        assert_eq!(weight(&t, Some(&heard(0)), NOW), 1.0);
        assert_eq!(weight(&t, Some(&heard(45 * DAY)), NOW), 2.5);
        assert_eq!(weight(&t, Some(&heard(400 * DAY)), NOW), FORGOTTEN_WEIGHT);
        let future: LastHeard = [(t.id.clone(), NOW + 60)].into();
        assert_eq!(weight(&t, Some(&future), NOW), 1.0, "a clock behind another computer's");
    }

    #[test]
    #[ignore = "timing; run with --ignored --release"]
    fn a_big_library_shuffles_quickly() {
        let mut tracks: Vec<Track> = (0..20_000).map(|i| by(i, &format!("a{}", i % 1500))).collect();
        let heard: LastHeard = (0..10_000).map(|i| (by(i, "").id, NOW - i * 600)).collect();
        let started = std::time::Instant::now();
        shuffle(&mut tracks, None, Some(&heard), NOW, &mut fastrand::Rng::with_seed(3));
        assert!(started.elapsed() < std::time::Duration::from_millis(500), "{:?}", started.elapsed());

        // A discography: one artist, nothing to keep apart; and one with a guest.
        for guests in [0, 10] {
            let mut tracks: Vec<Track> = (0..20_000).map(|i| by(i, if i < guests { "guest" } else { "a" })).collect();
            let started = std::time::Instant::now();
            shuffle(&mut tracks, Some(&by(99, "a")), None, NOW, &mut fastrand::Rng::with_seed(4));
            assert!(started.elapsed() < std::time::Duration::from_millis(100), "{guests} guests: {:?}", started.elapsed());
        }
    }
}
