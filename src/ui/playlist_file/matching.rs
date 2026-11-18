//! Telling which of a source's tracks a file means: names folded for comparing
//! (case, accents, "feat." and version notes aside), then a score per candidate.

use super::read::Wanted;
use crate::model::Track;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Letters that don't come apart into a base letter and an accent, as the
/// letters they stand for.
fn base_letters(c: char) -> Option<&'static str> {
    Some(match c {
        'ß' => "ss",
        'æ' => "ae",
        'œ' => "oe",
        'ø' => "o",
        'ł' => "l",
        'đ' | 'ð' => "d",
        'þ' => "th",
        'ı' => "i",
        _ => return None,
    })
}

/// A name folded for comparing: lower case, no accents (however they're encoded:
/// macOS file names keep them apart from their letter) or punctuation, "&" as "and".
fn fold_name(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.nfd().filter(|&c| !is_combining_mark(c)).flat_map(char::to_lowercase) {
        match base_letters(c) {
            Some(base) => out.push_str(base),
            None if c.is_alphanumeric() => out.push(c),
            None if c == '&' => out.push_str(" and "),
            None => out.push(' '),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An artist's name folded, without a leading "the" ("The Beatles", "Beatles").
fn fold_artist(name: &str) -> String {
    let name = fold_name(name);
    name.strip_prefix("the ").map_or_else(|| name.clone(), str::to_string)
}

/// A title folded for comparing, without what versions add: "(Remastered 2011)",
/// "[Live]", "- Radio Edit", "feat. Someone".
fn fold_title(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let lower = out.to_lowercase();
    let cut = [" - ", " – ", " feat. ", " feat ", " ft. ", " featuring "].iter().filter_map(|m| lower.find(m)).min();
    match fold_name(cut.map_or(&lower[..], |at| &lower[..at])) {
        // All of it in brackets ("(Nice Dream)"): the brackets are the title.
        folded if folded.is_empty() => fold_name(text),
        folded => folded,
    }
}

/// The artists a file's artist field lists, folded: "Daft Punk, Pharrell
/// Williams" or "Daft Punk feat. Pharrell Williams" → both. Split only where
/// lists are, not at "&", "and" or "/", which band names hold ("Simon &
/// Garfunkel", "AC/DC").
fn listed(text: &str) -> Vec<String> {
    let mut text = text.to_lowercase();
    for featuring in [" feat. ", " feat ", " ft. ", " featuring "] {
        text = text.replace(featuring, ",");
    }
    text.split([',', ';']).map(fold_artist).filter(|n| !n.is_empty()).collect()
}

/// What each kind of agreement between a file's track and a found one is worth.
/// A title and (when the file names one) an artist must agree, and the track
/// must be playable (only those go on a playlist); the rest only ranks the
/// candidates that do.
mod points {
    pub const SAME_TITLE: u32 = 6;
    /// One title is the other plus words ("Love" and "Love Story").
    pub const TITLE_WORDS: u32 = 3;
    pub const ARTIST: u32 = 4;
    pub const SAME_LENGTH: u32 = 2;
    pub const NEAR_LENGTH: u32 = 1;
    pub const ALBUM: u32 = 2;
}

/// Lengths within this many seconds are the same recording.
const SAME_LENGTH_SECS: u32 = 3;
/// Within this, likely a different master or edit of it.
const NEAR_LENGTH_SECS: u32 = 10;
/// Past this, another version: live, extended, a different song of the same name.
const OTHER_VERSION_SECS: u32 = 45;

/// How well `track` fits what the file asks for; None when it isn't the one.
fn fit(want: &Wanted, track: &Track) -> Option<u32> {
    if !track.available {
        return None;
    }
    let title = title_points(&fold_title(&want.title), &fold_title(&track.title))?;
    let lengths_known = want.secs > 0 && track.duration > 0;
    // Only part of the title alike ("Love", "Love Story"): the same song only if
    // the lengths say so.
    if title == points::TITLE_WORDS && !lengths_known {
        return None;
    }
    let mut score = title;
    if !want.artist.is_empty() {
        if !same_artist(&want.artist, track) {
            return None;
        }
        score += points::ARTIST;
    }
    if lengths_known {
        score += match want.secs.abs_diff(track.duration) {
            d if d <= SAME_LENGTH_SECS => points::SAME_LENGTH,
            d if d <= NEAR_LENGTH_SECS => points::NEAR_LENGTH,
            d if d <= OTHER_VERSION_SECS => 0,
            _ => return None,
        };
    }
    if !want.album.is_empty() && fold_title(&want.album) == fold_title(&track.album) {
        score += points::ALBUM;
    }
    Some(score)
}

/// Points for two folded titles; None when they name different songs.
fn title_points(ours: &str, theirs: &str) -> Option<u32> {
    if ours.is_empty() || theirs.is_empty() {
        None
    } else if ours == theirs {
        Some(points::SAME_TITLE)
    } else if starts_with_words(ours, theirs) || starts_with_words(theirs, ours) {
        Some(points::TITLE_WORDS)
    } else {
        None
    }
}

/// Whether the file's `artist` is the track's: the whole name the same as the
/// track's artist or one of its credited artists, or (a file listing several)
/// one of those listed the same as one of the track's. Names are compared whole:
/// "Simon" isn't "Paul Simon", "Prince" isn't "Prince Royce".
fn same_artist(artist: &str, track: &Track) -> bool {
    // The credited artists, or else the displayed name split where it joins
    // several (a name of one's own can hold a comma: "Earth, Wind & Fire").
    let credited: Vec<&str> =
        if track.artists.is_empty() { track.artist.split(", ").collect() } else { track.artists.iter().map(|a| a.name.as_str()).collect() };
    let theirs: Vec<String> = credited.into_iter().chain([track.artist.as_str()]).map(fold_artist).filter(|n| !n.is_empty()).collect();
    theirs.contains(&fold_artist(artist)) || listed(artist).iter().any(|n| theirs.contains(n))
}

/// Whether `whole` begins with all the words of `start` ("love story" with "love",
/// not "lovely" with "love").
fn starts_with_words(whole: &str, start: &str) -> bool {
    whole.strip_prefix(start).is_some_and(|rest| rest.starts_with(' '))
}

/// The track among `found` that fits best.
pub fn best<'a>(want: &Wanted, found: &'a [Track]) -> Option<&'a Track> {
    found
        .iter()
        .filter_map(|t| fit(want, t).map(|score| (score, t)))
        .fold(None, |best: Option<(u32, &Track)>, (score, t)| match best {
            Some((top, _)) if top >= score => best,
            _ => Some((score, t)),
        })
        .map(|(_, t)| t)
}

/// What to search for: the artist as the file gives it (all of it: "Earth, Wind
/// & Fire" is one) and the title without its extras.
pub fn query(want: &Wanted) -> String {
    let title = fold_title(&want.title);
    match fold_artist(&want.artist) {
        artist if artist.is_empty() => title,
        artist => format!("{artist} {title}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ArtistRef;
    use crate::ui::playlist_file::fixtures::{song, want};

    #[test]
    fn names_fold_case_accents_and_ampersands() {
        assert_eq!(fold_name("Beyoncé & JAY-Z"), "beyonce and jay z");
        assert_eq!(fold_name("Sigur Rós"), fold_name("sigur ros"));
        assert_eq!(fold_title("Teardrop (2019 Remaster) - Radio Edit"), "teardrop");
        assert_eq!(fold_title("Get Lucky feat. Pharrell Williams"), "get lucky");
        assert_eq!(fold_name("Sigur Ro\u{301}s"), "sigur ros", "an accent kept apart, as in a macOS file name");
        assert_eq!(fold_name("Mø, Łódź, Straße"), "mo lodz strasse");
        assert_eq!(listed("A, B & C; D feat. E"), ["a", "b and c", "d", "e"]);
        assert_eq!(listed("AC/DC"), ["ac dc"]);
        assert_eq!(fold_title("(Nice Dream)"), "nice dream");
        assert_eq!(fold_artist("The Beatles"), fold_artist("Beatles"));
    }

    #[test]
    fn the_right_version_of_the_right_song_is_picked() {
        let found = [
            song("1", "Teardrop (Live)", "Massive Attack", "Live", 420, true),
            song("2", "Teardrop", "Some Cover Band", "Covers", 330, true),
            song("3", "Teardrop - 2019 Remaster", "Massive Attack", "Mezzanine", 331, true),
        ];
        let w = want("Massive Attack", "Teardrop", 330);
        assert_eq!(best(&w, &found).map(|t| t.id.as_str()), Some("3"));
        assert!(fit(&want("Massive Attack", "Angel", 0), &found[2]).is_none());
        assert!(fit(&want("Beyonce", "Halo", 0), &song("4", "Halo", "Beyoncé", "", 0, true)).is_some());
        assert!(fit(&want("Simon and Garfunkel", "The Boxer", 0), &song("5", "The Boxer", "Simon & Garfunkel", "", 0, true)).is_some());
        // A featured artist listed first in the file still counts.
        assert!(
            fit(
                &want("Daft Punk, Pharrell Williams", "Get Lucky (feat. Pharrell Williams)", 369),
                &song("6", "Get Lucky", "Daft Punk", "", 368, true)
            )
            .is_some()
        );
        assert_eq!(
            query(&want("Daft Punk, Pharrell Williams", "Get Lucky (feat. Pharrell Williams)", 0)),
            "daft punk pharrell williams get lucky"
        );
        assert_eq!(query(&want("Simon & Garfunkel", "The Boxer", 0)), "simon and garfunkel the boxer");
        assert_eq!(query(&want("AC/DC", "T.N.T.", 0)), "ac dc t n t");
    }

    #[test]
    fn artists_are_compared_by_whole_names() {
        let fits = |artist: &str, theirs: &str| fit(&want(artist, "Kiss", 0), &song("1", "Kiss", theirs, "", 0, true)).is_some();
        assert!(!fits("Simon & Garfunkel", "Paul Simon"));
        assert!(!fits("Prince", "Prince Royce"));
        assert!(!fits("Queen", "Queen Latifah"));
        assert!(fits("Prince", "Prince"));
        assert!(fits("The Beatles", "Beatles"));
        assert!(fits("Daft Punk feat. Pharrell Williams", "Daft Punk"), "one of several listed");
        assert!(fits("Daft Punk", "Daft Punk, Pharrell Williams"), "one of several credited");
        let mut band = song("2", "Kiss", "Earth, Wind & Fire", "", 0, true);
        band.artists = vec![ArtistRef { id: "9".into(), name: "Earth, Wind & Fire".into() }];
        assert!(fit(&want("Earth", "Kiss", 0), &band).is_none(), "a comma inside one name");
        assert!(fit(&want("Earth, Wind & Fire", "Kiss", 0), &band).is_some());
    }

    #[test]
    fn only_playable_tracks_fit() {
        let mut gone = song("1", "Teardrop", "Massive Attack", "", 330, true);
        gone.available = false;
        assert!(fit(&want("Massive Attack", "Teardrop", 330), &gone).is_none());
    }

    #[test]
    fn a_longer_title_fits_only_by_whole_words() {
        let story = song("1", "Love Story", "Taylor Swift", "", 235, true);
        assert_eq!(fit(&want("Taylor Swift", "Love", 235), &story), Some(points::TITLE_WORDS + points::ARTIST + points::SAME_LENGTH));
        assert!(fit(&want("Taylor Swift", "Love", 0), &story).is_none(), "no length to tell them apart");
        assert!(fit(&want("Taylor Swift", "Lov", 235), &story).is_none());
        assert!(fit(&want("Adele", "Hello", 235), &song("2", "Hellothere", "Adele", "", 235, true)).is_none());
    }

    #[test]
    fn another_length_means_another_version() {
        let radio = song("1", "Song", "Band", "", 200, true);
        assert!(fit(&want("Band", "Song", 600), &radio).is_none(), "the extended mix");
        assert!(fit(&want("Band", "Song", 0), &radio).is_some(), "the file doesn't say");
        assert!(fit(&want("Band", "Song", 202), &radio) > fit(&want("Band", "Song", 230), &radio));
    }
}
