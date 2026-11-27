//! An artist's genres from MusicBrainz, the open music encyclopedia: neither
//! Deezer nor a music server gives them for an artist. No key is needed, but at
//! most one request a second, and a User-Agent saying who's asking.

use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

const API: &str = "https://musicbrainz.org/ws/2";
const USER_AGENT: &str = concat!("dust/", env!("CARGO_PKG_VERSION"), " ( https://github.com/domknez/dust )");
/// Between two requests: MusicBrainz turns away more than one a second.
const GAP: Duration = Duration::from_millis(1100);
/// How sure the search must be of an artist (0–100) for it to count.
const MIN_SCORE: u64 = 90;
/// Genres kept, the most agreed on first.
pub const GENRES: usize = 4;

/// When the last request went out, to keep them [`GAP`] apart.
static LAST: Mutex<Option<Instant>> = Mutex::new(None);

/// The genres of the artist named `name`, the most agreed on first; none when
/// MusicBrainz doesn't know them (or anyone by that name). Before each request
/// `wanted` says whether they're still wanted: if not, nothing more is asked and
/// the answer is None.
pub fn artist_genres(name: &str, wanted: &dyn Fn() -> bool) -> Result<Option<Vec<String>>, String> {
    let agent = crate::http::agent(USER_AGENT, Some(Duration::from_secs(15)));
    let phrase = format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""));
    let query = format!("artist:{phrase} OR alias:{phrase}");
    let search = agent.get(format!("{API}/artist")).query("query", &query).query("limit", "5").query("fmt", "json");
    let Some(found) = get(search, wanted)? else { return Ok(None) };
    let Some(id) = best_match(&found, name) else { return Ok(Some(Vec::new())) };
    let lookup = agent.get(format!("{API}/artist/{id}")).query("inc", "genres").query("fmt", "json");
    Ok(get(lookup, wanted)?.map(|artist| genres(&artist)))
}

/// Send `request` once [`GAP`] has passed since the last one, and read its JSON;
/// None, unsent, when it's no longer `wanted` by then.
fn get(request: ureq::RequestBuilder<ureq::typestate::WithoutBody>, wanted: &dyn Fn() -> bool) -> Result<Option<Value>, String> {
    {
        let mut last = LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(wait) = last.and_then(|at| GAP.checked_sub(at.elapsed())) {
            std::thread::sleep(wait);
        }
        if !wanted() {
            return Ok(None);
        }
        *last = Some(Instant::now());
    }
    let mut reply = request.call().map_err(|e| format!("MusicBrainz: {e}"))?;
    reply.body_mut().read_json().map(Some).map_err(|e| format!("MusicBrainz: {e}"))
}

/// The id of the artist in search results `found` who is called `name` (or has it
/// as an alias), the likeliest first.
fn best_match<'a>(found: &'a Value, name: &str) -> Option<&'a str> {
    let wanted = fold(name);
    let artists = found["artists"].as_array()?;
    artists
        .iter()
        .filter(|a| a["score"].as_u64().is_some_and(|s| s >= MIN_SCORE))
        .find(|a| {
            let aliases = a["aliases"].as_array().into_iter().flatten().filter_map(|al| al["name"].as_str());
            a["name"].as_str().into_iter().chain(aliases).any(|n| fold(n) == wanted)
        })
        .and_then(|a| a["id"].as_str())
}

/// An artist's genres from its lookup, the most votes first.
fn genres(artist: &Value) -> Vec<String> {
    let mut all: Vec<(u64, &str)> = artist["genres"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| Some((g["count"].as_u64()?, g["name"].as_str()?)))
        .filter(|&(count, name)| count > 0 && !name.is_empty())
        .collect();
    all.sort_by_key(|&(count, _)| std::cmp::Reverse(count));
    all.into_iter().take(GENRES).map(|(_, name)| name.to_string()).collect()
}

/// A name for comparing: lowercase, without accents.
fn fold(name: &str) -> String {
    name.trim().nfd().filter(|&c| !is_combining_mark(c)).flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn found() -> Value {
        json!({"artists": [
            {"id": "dj", "name": "DJ Sigur Ros", "score": 100},
            {"id": "sr", "name": "Sigur Rós", "score": 98},
            {"id": "other", "name": "Sigur Ros", "score": 40},
        ]})
    }

    #[test]
    fn the_artist_by_that_name_is_picked() {
        assert_eq!(best_match(&found(), "sigur ros"), Some("sr"), "accents and case aside");
        assert_eq!(best_match(&found(), "Sigur"), None, "a part of a name isn't the name");
        assert_eq!(best_match(&json!({}), "x"), None);
    }

    #[test]
    fn an_alias_counts_and_a_weak_hit_does_not() {
        let found = json!({"artists": [
            {"id": "weak", "name": "Bjork", "score": 50},
            {"id": "b", "name": "Björk Guðmundsdóttir", "score": 95, "aliases": [{"name": "Bjork"}]},
        ]});
        assert_eq!(best_match(&found, "Björk"), Some("b"));
    }

    #[test]
    fn genres_come_most_voted_first() {
        let artist = json!({"genres": [
            {"name": "rock", "count": 19},
            {"name": "alternative rock", "count": 43},
            {"name": "unvoted", "count": 0},
            {"name": "art rock", "count": 30},
            {"name": "electronic", "count": 14},
            {"name": "art pop", "count": 3},
        ]});
        assert_eq!(genres(&artist), ["alternative rock", "art rock", "rock", "electronic"]);
        assert!(genres(&json!({"name": "x"})).is_empty());
    }
}
