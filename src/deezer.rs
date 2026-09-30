//! Minimal Deezer client: session via ARL cookie, gw-light API for catalog,
//! media.deezer.com for stream URLs, and on-the-fly stripe decryption.

use blowfish::Blowfish;
use blowfish::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
use md5::{Digest, Md5};
use serde_json::{Value, json};
use std::io::{self, Read};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const GW: &str = "https://www.deezer.com/ajax/gw-light.php";
const MEDIA: &str = "https://media.deezer.com/v1/get_url";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
const STRIPE_SECRET: &[u8; 16] = b"g4el58wc0zvf9na1";

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug)]
pub struct Track {
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: u32,
    pub token: String,
}

#[derive(Clone, Debug)]
pub struct Playlist {
    pub id: u64,
    pub title: String,
    pub count: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    Mp3_128,
    Mp3_320,
    Flac,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Mp3_128, Quality::Mp3_320, Quality::Flac];

    pub fn label(self) -> &'static str {
        match self {
            Quality::Mp3_128 => "MP3 128",
            Quality::Mp3_320 => "MP3 320",
            Quality::Flac => "FLAC",
        }
    }

    /// Requested format and every lower one, best first.
    fn formats(self) -> &'static [&'static str] {
        match self {
            Quality::Flac => &["FLAC", "MP3_320", "MP3_128"],
            Quality::Mp3_320 => &["MP3_320", "MP3_128"],
            Quality::Mp3_128 => &["MP3_128"],
        }
    }
}

struct Session {
    api_token: String,
    sid: Option<String>,
}

struct Inner {
    agent: ureq::Agent,
    arl: String,
    session: Mutex<Session>,
    license_token: String,
    pub user_id: u64,
    pub loved_id: u64,
    pub name: String,
}

#[derive(Clone)]
pub struct Deezer(Arc<Inner>);

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn n(v: &Value) -> u64 {
    match v {
        Value::Number(n) => n.as_u64().unwrap_or(0),
        Value::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn parse_track(v: &Value) -> Option<Track> {
    let id = n(&v["SNG_ID"]);
    let token = s(&v["TRACK_TOKEN"]);
    if id == 0 || token.is_empty() {
        return None; // user uploads (negative ids) or unavailable tracks
    }
    let artist = match v["ARTISTS"].as_array() {
        Some(a) if !a.is_empty() => a.iter().map(|a| s(&a["ART_NAME"])).collect::<Vec<_>>().join(", "),
        _ => s(&v["ART_NAME"]),
    };
    Some(Track {
        id,
        title: match s(&v["VERSION"]) {
            ver if ver.is_empty() => s(&v["SNG_TITLE"]),
            ver => format!("{} {}", s(&v["SNG_TITLE"]), ver),
        },
        artist,
        album: s(&v["ALB_TITLE"]),
        duration: n(&v["DURATION"]) as u32,
        token,
    })
}

fn parse_tracks(v: &Value) -> Vec<Track> {
    v.as_array().map(|a| a.iter().filter_map(parse_track).collect()).unwrap_or_default()
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(UA)
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .build()
}

impl Deezer {
    /// Log in with an `arl` cookie value taken from a browser session.
    pub fn login(arl: &str) -> Result<Deezer> {
        let arl = arl.trim().to_string();
        let agent = agent();
        let mut session = Session { api_token: "null".into(), sid: None };
        let data = gw_call(&agent, &arl, &mut session, "deezer.getUserData", json!({}))?;
        let user = &data["USER"];
        let user_id = n(&user["USER_ID"]);
        if user_id == 0 {
            return Err("Invalid or expired ARL".into());
        }
        session.api_token = s(&data["checkForm"]);
        let license_token = s(&user["OPTIONS"]["license_token"]);
        if license_token.is_empty() {
            return Err("Account has no streaming license".into());
        }
        Ok(Deezer(Arc::new(Inner {
            agent,
            arl,
            session: Mutex::new(session),
            license_token,
            user_id,
            loved_id: n(&user["LOVEDTRACKS_ID"]),
            name: s(&user["BLOG_NAME"]),
        })))
    }

    pub fn name(&self) -> &str {
        &self.0.name
    }

    fn call(&self, method: &str, body: Value) -> Result<Value> {
        let mut session = self.0.session.lock().unwrap();
        match gw_call(&self.0.agent, &self.0.arl, &mut session, method, body.clone()) {
            Err(e) if e.contains("VALID_TOKEN_REQUIRED") => {
                // api_token expired: refresh and retry once.
                session.api_token = "null".into();
                let data = gw_call(&self.0.agent, &self.0.arl, &mut session, "deezer.getUserData", json!({}))?;
                session.api_token = s(&data["checkForm"]);
                gw_call(&self.0.agent, &self.0.arl, &mut session, method, body)
            }
            r => r,
        }
    }

    pub fn search(&self, query: &str) -> Result<Vec<Track>> {
        let r = self.call(
            "deezer.pageSearch",
            json!({"query": query, "start": 0, "nb": 100, "suggest": false, "artist_suggest": false, "top_tracks": false}),
        )?;
        Ok(parse_tracks(&r["TRACK"]["data"]))
    }

    pub fn flow(&self) -> Result<Vec<Track>> {
        let r = self.call("radio.getUserRadio", json!({"user_id": self.0.user_id}))?;
        Ok(parse_tracks(&r["data"]))
    }

    pub fn loved(&self) -> Result<Vec<Track>> {
        self.playlist(self.0.loved_id)
    }

    pub fn playlist(&self, id: u64) -> Result<Vec<Track>> {
        let r = self.call(
            "deezer.pagePlaylist",
            json!({"playlist_id": id, "lang": "en", "nb": 2000, "start": 0, "tab": 0, "tags": true, "header": true}),
        )?;
        Ok(parse_tracks(&r["SONGS"]["data"]))
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let r = self.call("deezer.pageProfile", json!({"user_id": self.0.user_id, "tab": "playlists", "nb": 500}))?;
        let list = r["TAB"]["playlists"]["data"].as_array().cloned().unwrap_or_default();
        Ok(list
            .iter()
            .map(|p| Playlist { id: n(&p["PLAYLIST_ID"]), title: s(&p["TITLE"]), count: n(&p["NB_SONG"]) as u32 })
            .filter(|p| p.id != 0 && p.id != self.0.loved_id)
            .collect())
    }

    /// Fresh track token, used when a cached one was rejected.
    fn refresh_token(&self, id: u64) -> Result<String> {
        let r = self.call("song.getListData", json!({"sng_ids": [id]}))?;
        r["data"][0]["TRACK_TOKEN"].as_str().map(str::to_string).ok_or_else(|| "No track token".into())
    }

    fn media_url(&self, token: &str, quality: Quality) -> Result<(String, Format)> {
        let formats: Vec<Value> =
            quality.formats().iter().map(|f| json!({"cipher": "BF_CBC_STRIPE", "format": f})).collect();
        let body = json!({
            "license_token": self.0.license_token,
            "media": [{"type": "FULL", "formats": formats}],
            "track_tokens": [token],
        });
        let r: Value = self
            .0
            .agent
            .post(MEDIA)
            .send_json(body)
            .map_err(|e| format!("get_url: {e}"))?
            .into_json()
            .map_err(|e| format!("get_url: {e}"))?;
        let item = &r["data"][0];
        if let Some(err) = item["errors"].get(0) {
            return Err(format!("Deezer: {}", s(&err["message"])));
        }
        let media = &item["media"][0];
        let url = media["sources"][0]["url"].as_str().ok_or("No stream source")?;
        let format = match media["format"].as_str() {
            Some("FLAC") => Format::Flac,
            Some("MP3_128") => Format::Mp3(128),
            _ => Format::Mp3(320),
        };
        Ok((url.to_string(), format))
    }

    /// Resolve a (short-lived) CDN URL for a track at the best available format.
    pub fn stream_url(&self, track: &Track, quality: Quality) -> Result<(String, Format)> {
        match self.media_url(&track.token, quality) {
            Ok(r) => Ok(r),
            Err(_) => self.media_url(&self.refresh_token(track.id)?, quality),
        }
    }

    /// Open a decrypted byte stream, starting at `offset` rounded down to a stripe chunk.
    /// Returns the reader and the offset actually used.
    pub fn open(&self, url: &str, track_id: u64, offset: u64) -> Result<(Box<dyn Read + Send + Sync>, u64)> {
        let offset = offset - offset % CHUNK as u64;
        let mut req = self.0.agent.get(url);
        if offset > 0 {
            req = req.set("Range", &format!("bytes={offset}-"));
        }
        let resp = req.call().map_err(|e| format!("stream: {e}"))?;
        let reader = StripeReader::new(resp.into_reader(), track_id, offset / CHUNK as u64);
        Ok((Box::new(reader), offset))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    /// Constant bitrate MP3, kbps.
    Mp3(u32),
    Flac,
}

fn gw_call(agent: &ureq::Agent, arl: &str, session: &mut Session, method: &str, body: Value) -> Result<Value> {
    let mut cookie = format!("arl={arl}");
    if let Some(sid) = &session.sid {
        cookie.push_str("; sid=");
        cookie.push_str(sid);
    }
    let resp = agent
        .post(GW)
        .query("method", method)
        .query("input", "3")
        .query("api_version", "1.0")
        .query("api_token", &session.api_token)
        .set("Cookie", &cookie)
        .send_json(body)
        .map_err(|e| format!("{method}: {e}"))?;
    for c in resp.all("set-cookie") {
        if let Some(v) = c.strip_prefix("sid=") {
            session.sid = v.split(';').next().map(str::to_string);
        }
    }
    let v: Value = resp.into_json().map_err(|e| format!("{method}: {e}"))?;
    match &v["error"] {
        Value::Object(o) if !o.is_empty() => Err(format!("{method}: {}", Value::Object(o.clone()))),
        _ => Ok(v["results"].clone()),
    }
}

const CHUNK: usize = 2048;

fn stripe_key(track_id: u64) -> [u8; 16] {
    let hex: Vec<u8> = Md5::digest(track_id.to_string().as_bytes())
        .iter()
        .flat_map(|b| format!("{b:02x}").into_bytes())
        .collect();
    std::array::from_fn(|i| hex[i] ^ hex[i + 16] ^ STRIPE_SECRET[i])
}

/// Decrypts Deezer's "BF_CBC_STRIPE" layout: every third full 2048-byte chunk
/// is Blowfish-CBC encrypted with a fixed IV, the rest is plaintext.
pub struct StripeReader<R> {
    inner: R,
    cipher: Blowfish,
    chunk_index: u64,
    buf: Box<[u8; CHUNK]>,
    len: usize,
    pos: usize,
}

impl<R: Read> StripeReader<R> {
    pub fn new(inner: R, track_id: u64, start_chunk: u64) -> Self {
        Self {
            inner,
            cipher: Blowfish::new_from_slice(&stripe_key(track_id)).expect("16-byte key"),
            chunk_index: start_chunk,
            buf: Box::new([0; CHUNK]),
            len: 0,
            pos: 0,
        }
    }

    fn fill(&mut self) -> io::Result<()> {
        self.len = 0;
        self.pos = 0;
        while self.len < CHUNK {
            match self.inner.read(&mut self.buf[self.len..]) {
                Ok(0) => break,
                Ok(n) => self.len += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        if self.len == CHUNK && self.chunk_index % 3 == 0 {
            let mut prev = [0u8, 1, 2, 3, 4, 5, 6, 7];
            for block in self.buf.chunks_exact_mut(8) {
                let ct: [u8; 8] = block.try_into().unwrap();
                self.cipher.decrypt_block(GenericArray::from_mut_slice(block));
                block.iter_mut().zip(prev).for_each(|(b, p)| *b ^= p);
                prev = ct;
            }
        }
        self.chunk_index += 1;
        Ok(())
    }
}

impl<R: Read> Read for StripeReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos == self.len {
            self.fill()?;
            if self.len == 0 {
                return Ok(0);
            }
        }
        let n = out.len().min(self.len - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blowfish::cipher::BlockEncrypt;

    #[test]
    fn stripe_roundtrip() {
        let id = 3135556;
        let plain: Vec<u8> = (0..CHUNK * 4 + 100).map(|i| (i * 7) as u8).collect();
        let mut enc = plain.clone();
        let cipher: Blowfish = Blowfish::new_from_slice(&stripe_key(id)).unwrap();
        for (ci, chunk) in enc.chunks_mut(CHUNK).enumerate() {
            if ci % 3 == 0 && chunk.len() == CHUNK {
                let mut prev = [0u8, 1, 2, 3, 4, 5, 6, 7];
                for block in chunk.chunks_exact_mut(8) {
                    block.iter_mut().zip(prev).for_each(|(b, p)| *b ^= p);
                    cipher.encrypt_block(GenericArray::from_mut_slice(block));
                    prev = block.try_into().unwrap();
                }
            }
        }
        assert_ne!(enc, plain);
        let mut out = Vec::new();
        StripeReader::new(&enc[..], id, 0).read_to_end(&mut out).unwrap();
        assert_eq!(out, plain);
    }
}
