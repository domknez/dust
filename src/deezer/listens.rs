//! Listen reports. Deezer's own apps send these; Deezer uses them for history,
//! Flow and — if the account is linked — Last.fm scrobbling.

use super::{Deezer, Result, parse};
use crate::model::{Format, Listen, TrackId};
use serde_json::json;

/// The device version Deezer's web player sends with reports.
const DEVICE_VERSION: &str = "10020230525142740";

impl Deezer {
    /// A track started (feeds history and "Recently played").
    pub fn report_listen_start(&self, song_id: &TrackId) -> Result<()> {
        let id = parse::id_value(song_id.as_str());
        self.call("log.listen", json!({"next_media": {"media": {"id": id, "type": "song"}}})).map(drop)
    }

    /// A track finished or was skipped.
    pub fn report_listen(&self, listen: &Listen) -> Result<()> {
        let id = parse::id_value(listen.song_id.as_str());
        let params = json!({
            "media": {"id": id, "type": "song", "format": api_name(listen.format)},
            "type": 1,
            "stat": {"seek": u8::from(listen.skipped), "pause": 0, "sync": 0, "next": listen.skipped},
            "lt": listen.listened_secs,
            "ctxt": {"t": "search_page", "id": id},
            "dev": {"v": DEVICE_VERSION, "t": 0},
            "ls": [],
            "ts_listen": listen.started_unix,
            "is_shuffle": false,
            "stream_id": stream_uuid(),
        });
        self.call("log.listen", json!({"params": params})).map(drop)
    }
}

/// A random RFC 4122 v4 id, as Deezer's clients send for each stream.
fn stream_uuid() -> String {
    let h = format!("{:016x}{:016x}", fastrand::u64(..), fastrand::u64(..));
    format!("{}-{}-4{}-a{}-{}", &h[..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32])
}

/// Deezer's name for the format a listen was streamed in.
fn api_name(format: Format) -> &'static str {
    match format {
        Format::Flac => "FLAC",
        Format::Mp3(128) | Format::Podcast | Format::File => "MP3_128",
        Format::Mp3(_) => "MP3_320",
    }
}

#[cfg(test)]
mod tests {
    use super::stream_uuid;

    #[test]
    fn stream_ids_are_version_4_uuids() {
        let id = stream_uuid();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), [8, 4, 4, 4, 12], "{id}");
        assert!(parts[2].starts_with('4') && parts[3].starts_with('a'), "{id}");
        assert!(id.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
        assert_ne!(id, stream_uuid());
    }
}
