//! Resolving stream URLs and opening decrypted streams.

use super::crypto::{CHUNK, StripeReader};
use super::models::{Format, Quality, Track};
use super::parse::text;
use super::prefetch::Prefetch;
use super::session::CALL_TIMEOUT;
use super::{Deezer, Error, Result};
use serde_json::{Value, json};
use std::io::Read;

const MEDIA_API: &str = "https://media.deezer.com/v1/get_url";

/// A resolved stream: short-lived CDN URL, delivered format, and the song id whose
/// key decrypts it (differs from the track's id on regional fallbacks).
#[derive(Clone)]
pub struct StreamSource {
    pub url: String,
    pub format: Format,
    pub song_id: u64,
}

impl Deezer {
    /// Best available stream for `track`: its own token, a refreshed token, then
    /// Deezer's regional fallback version.
    pub fn stream_source(&self, track: &Track, quality: Quality) -> Result<StreamSource> {
        let with_id = |(url, format): (String, Format), song_id| StreamSource { url, format, song_id };
        let first_error = match self.media_url(&track.token, quality) {
            Ok(found) => return Ok(with_id(found, track.id)),
            Err(e) => e,
        };
        if let Ok(found) = self.refresh_token(track.id).and_then(|t| self.media_url(&t, quality)) {
            return Ok(with_id(found, track.id));
        }
        match &track.fallback {
            Some((id, token)) => self.media_url(token, quality).map(|found| with_id(found, *id)).map_err(|_| first_error),
            None => Err(first_error),
        }
    }

    /// Open a decrypted byte stream, starting at `offset` rounded down to a stripe
    /// chunk. Returns the reader and the offset actually used.
    pub fn open_stream(&self, source: &StreamSource, offset: u64) -> Result<(Box<dyn Read + Send + Sync>, u64)> {
        let offset = offset - offset % CHUNK as u64;
        // No overall deadline: a track streams for minutes and may sit paused.
        // Prefetch bounds each wait for data instead.
        let mut req = self.0.agent.get(&source.url).config().timeout_global(None).timeout_recv_response(Some(CALL_TIMEOUT)).build();
        if offset > 0 {
            req = req.header("Range", &format!("bytes={offset}-"));
        }
        let body = Prefetch::new(req.call()?.into_body().into_reader(), CALL_TIMEOUT);
        let reader = StripeReader::new(body, source.song_id, offset / CHUNK as u64);
        Ok((Box::new(reader), offset))
    }

    /// Fresh track token, used when a cached one was rejected.
    fn refresh_token(&self, id: u64) -> Result<String> {
        let r = self.call("song.getListData", json!({"sng_ids": [id]}))?;
        r["data"][0]["TRACK_TOKEN"].as_str().map(str::to_string).ok_or(Error::NotAvailable)
    }

    fn media_url(&self, token: &str, quality: Quality) -> Result<(String, Format)> {
        let formats: Vec<Value> = quality.api_formats().iter().map(|f| json!({"cipher": "BF_CBC_STRIPE", "format": f})).collect();
        let body = json!({
            "license_token": self.0.license_token,
            "media": [{"type": "FULL", "formats": formats}],
            "track_tokens": [token],
        });
        let r: Value = self.0.agent.post(MEDIA_API).send_json(body)?.into_body().read_json()?;
        let item = &r["data"][0];
        if let Some(err) = item["errors"].get(0) {
            // 2002: "Track token has no sufficient rights on requested media".
            return Err(match err["code"].as_u64() {
                Some(2002) => Error::NotAvailable,
                _ => Error::Api(text(&err["message"])),
            });
        }
        let media = &item["media"][0];
        let url = media["sources"][0]["url"].as_str().ok_or(Error::NotAvailable)?;
        Ok((url.to_string(), Format::from_api(media["format"].as_str())))
    }
}
