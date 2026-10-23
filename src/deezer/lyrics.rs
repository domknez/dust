//! Song lyrics from Deezer: time-synced lines when it has them, plain text otherwise.

use super::{Deezer, Error, Result, parse};
use crate::model::{LyricLine, Lyrics, TrackId};
use serde_json::{Value, json};

impl Deezer {
    /// The lyrics of a track, or None if Deezer has none.
    pub fn lyrics(&self, song_id: &TrackId) -> Result<Option<Lyrics>> {
        match self.call("song.getLyrics", json!({"sng_id": song_id.as_str()})) {
            Ok(r) => Ok(lyrics(&r)),
            // Deezer answers "no lyrics" with a DATA_ERROR.
            Err(Error::Api { message: e, .. }) if e.contains("DATA_ERROR") => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn lyrics(r: &Value) -> Option<Lyrics> {
    let synced: Vec<LyricLine> = r["LYRICS_SYNC_JSON"]
        .as_array()
        .into_iter()
        .flatten()
        // Entries without a time are verse breaks; the plain text keeps those.
        .filter(|l| !l["milliseconds"].is_null())
        .map(|l| LyricLine { at_ms: parse::number(&l["milliseconds"]) as u32, text: parse::text(&l["line"]) })
        .collect();
    let text = parse::text(&r["LYRICS_TEXT"]);
    let plain: Vec<String> = if text.trim().is_empty() {
        synced.iter().map(|l| l.text.clone()).collect()
    } else {
        text.lines().map(|l| l.trim_end().to_string()).collect()
    };
    if plain.iter().all(|l| l.trim().is_empty()) {
        return None;
    }
    Some(Lyrics { synced, plain, writers: parse::text(&r["LYRICS_WRITERS"]), copyright: parse::text(&r["LYRICS_COPYRIGHTS"]) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synced_lines_and_plain_text() {
        let r = json!({
            "LYRICS_SYNC_JSON": [
                {"line": "Work it", "milliseconds": "51480", "duration": "1160"},
                {"line": ""},
                {"line": "Harder", "milliseconds": "59710", "duration": "1190"}
            ],
            "LYRICS_TEXT": "Work it\r\n\r\nHarder",
            "LYRICS_WRITERS": "Thomas Bangalter",
            "LYRICS_COPYRIGHTS": "Because"
        });
        let l = lyrics(&r).unwrap();
        assert_eq!(l.synced, vec![LyricLine { at_ms: 51_480, text: "Work it".into() }, LyricLine { at_ms: 59_710, text: "Harder".into() }]);
        assert_eq!(l.plain, vec!["Work it", "", "Harder"]);
        assert_eq!((l.writers.as_str(), l.copyright.as_str()), ("Thomas Bangalter", "Because"));
        assert_eq!(l.line_at(0), None);
        assert_eq!(l.line_at(51_480), Some(0));
        assert_eq!(l.line_at(59_000), Some(0));
        assert_eq!(l.line_at(600_000), Some(1));
    }

    #[test]
    fn plain_only_and_empty() {
        let l = lyrics(&json!({"LYRICS_TEXT": "La la\nLa"})).unwrap();
        assert!(l.synced.is_empty());
        assert_eq!(l.plain, vec!["La la", "La"]);
        assert_eq!(lyrics(&json!({"LYRICS_TEXT": " "})), None);
        assert_eq!(lyrics(&json!({})), None);
    }
}
