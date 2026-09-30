//! Listen reports. Deezer's own apps send these; Deezer uses them for history,
//! Flow and — if the account is linked — Last.fm scrobbling.

use super::models::Listen;
use super::{Deezer, Result};
use serde_json::json;

/// Client/device identifiers Deezer's web player sends with reports.
const DEVICE_VERSION: &str = "10020230525142740";

impl Deezer {
    /// A track started (feeds history and "Recently played").
    pub fn report_listen_start(&self, song_id: u64) -> Result<()> {
        self.call("log.listen", json!({"next_media": {"media": {"id": song_id, "type": "song"}}})).map(drop)
    }

    /// A track finished or was skipped.
    pub fn report_listen(&self, listen: &Listen) -> Result<()> {
        let params = json!({
            "media": {"id": listen.song_id, "type": "song", "format": listen.format.api_name()},
            "type": 1,
            "stat": {"seek": u8::from(listen.skipped), "pause": 0, "sync": 0, "next": listen.skipped},
            "lt": listen.listened_secs,
            "ctxt": {"t": "search_page", "id": listen.song_id},
            "dev": {"v": DEVICE_VERSION, "t": 0},
            "ls": [],
            "ts_listen": listen.started_unix,
            "is_shuffle": false,
            "stream_id": listen.stream_id,
        });
        self.call("log.listen", json!({"params": params})).map(drop)
    }
}
