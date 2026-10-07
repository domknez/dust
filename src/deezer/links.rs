//! Deezer links pasted into search: `deezer.com/[lang/]album|artist|playlist|track/<id>`
//! and short share links (`link.deezer.com/…`, `deezer.page.link/…`), which redirect
//! to one of those.

use super::parse::text;
use super::{Deezer, Error, Result};
use serde_json::{Value, json};
use std::time::Duration;
use ureq::ResponseExt;

/// What a link points at, with enough to show its page header right away.
#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Album { id: String, title: String, artist: String, picture: Option<String> },
    Artist { id: String },
    Playlist { id: u64, title: String, picture: Option<(String, String)> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Album,
    Artist,
    Playlist,
    Track,
}

/// Whether `text` looks like a Deezer link (then search should resolve it, not query it).
pub fn is_link(text: &str) -> bool {
    let t = text.trim().to_ascii_lowercase();
    let host = t.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.");
    !t.contains(char::is_whitespace)
        && (host.starts_with("deezer.com/") || host.starts_with("link.deezer.com/") || host.starts_with("deezer.page.link/"))
}

/// `(kind, id)` from a full deezer.com URL: the first `album|artist|playlist|track`
/// path segment followed by a numeric id.
fn parse(url: &str) -> Option<(Kind, String)> {
    let path = url.split(['?', '#']).next()?;
    let segments: Vec<&str> = path.split('/').collect();
    segments.windows(2).find_map(|w| {
        let kind = match w[0] {
            "album" => Kind::Album,
            "artist" => Kind::Artist,
            "playlist" => Kind::Playlist,
            "track" => Kind::Track,
            _ => return None,
        };
        (!w[1].is_empty() && w[1].bytes().all(|b| b.is_ascii_digit())).then(|| (kind, w[1].to_string()))
    })
}

impl Deezer {
    /// Resolve a pasted link to what it shows; short links are followed first.
    pub fn resolve_link(&self, pasted: &str) -> Result<Link> {
        let pasted = pasted.trim();
        let url = match parse(pasted) {
            Some(_) => pasted.to_string(),
            None => follow_redirects(pasted)?,
        };
        let (kind, id) = parse(&url).ok_or_else(|| Error::Other("That link isn't an album, artist, playlist or track".into()))?;
        match kind {
            Kind::Artist => Ok(Link::Artist { id }),
            Kind::Album => self.album_link(&id),
            Kind::Track => {
                let song = self.call("song.getData", json!({"sng_id": id}))?;
                self.album_link(&text_or_missing(&song["ALB_ID"])?)
            }
            Kind::Playlist => {
                let id: u64 = id.parse().map_err(|_| Error::NotAvailable)?;
                let page = self
                    .call("deezer.pagePlaylist", json!({"playlist_id": id, "lang": "en", "nb": 0, "start": 0, "tab": 0, "header": true}))?;
                let data = &page["DATA"];
                let picture =
                    Some((text(&data["PICTURE_TYPE"]), text(&data["PLAYLIST_PICTURE"]))).filter(|(k, m)| !k.is_empty() && !m.is_empty());
                Ok(Link::Playlist { id, title: text(&data["TITLE"]), picture })
            }
        }
    }

    fn album_link(&self, id: &str) -> Result<Link> {
        let page = self.call("deezer.pageAlbum", json!({"alb_id": id, "lang": "en", "header": true, "tab": 0}))?;
        let data = &page["DATA"];
        Ok(Link::Album {
            id: id.to_string(),
            title: text(&data["ALB_TITLE"]),
            artist: text(&data["ART_NAME"]),
            picture: Some(text(&data["ALB_PICTURE"])).filter(|p| !p.is_empty()),
        })
    }
}

fn text_or_missing(v: &Value) -> Result<String> {
    Some(text(v)).filter(|s| !s.is_empty() && s != "0").ok_or(Error::NotAvailable)
}

/// Where a short share link ends up after its redirects.
fn follow_redirects(url: &str) -> Result<String> {
    let url = if url.starts_with("http") { url.to_string() } else { format!("https://{url}") };
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).build().into();
    let resp = agent.get(&url).call().map_err(|e| Error::Other(format!("Couldn't open that link ({e})")))?;
    Ok(resp.get_uri().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_links() {
        assert!(is_link("https://www.deezer.com/us/album/1100293142"));
        assert!(is_link("deezer.com/artist/27"));
        assert!(is_link(" https://link.deezer.com/s/31abcd "));
        assert!(is_link("https://deezer.page.link/xyz"));
        assert!(!is_link("daft punk"));
        assert!(!is_link("https://example.com/album/1"));
    }

    #[test]
    fn parses_kind_and_id() {
        assert_eq!(parse("https://www.deezer.com/us/album/1100293142"), Some((Kind::Album, "1100293142".into())));
        assert_eq!(parse("https://www.deezer.com/artist/27?utm_source=x"), Some((Kind::Artist, "27".into())));
        assert_eq!(parse("https://www.deezer.com/hr/playlist/908622995#top"), Some((Kind::Playlist, "908622995".into())));
        assert_eq!(parse("https://www.deezer.com/track/3135556"), Some((Kind::Track, "3135556".into())));
        assert_eq!(parse("https://www.deezer.com/en/profile/123"), None);
        assert_eq!(parse("https://www.deezer.com/album/abc"), None);
    }
}
