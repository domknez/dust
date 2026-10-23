//! Navidrome and other Subsonic / OpenSubsonic servers: a second music source next to
//! Deezer, filling the same models ([`crate::model::Track`] and friends) so the UI
//! and player work the same with either.
//!
//! - [`Subsonic`]: connecting (token authentication), calls and id bookkeeping
//! - [`catalog`]: home shelves, albums, artists, playlists, search, likes, lyrics
//! - [`parse`]: models from the API's JSON
//! - [`stream`]: the song files, with seeking and server-side conversion
//!
//! Ids: the server's ids are strings, dust's track and playlist ids are numbers. A
//! song's or playlist's number is a hash of its server id; the client remembers
//! which string each number stands for.

mod catalog;
#[cfg(test)]
mod live_tests;
mod parse;
mod stream;

use crate::model::{Art, Error, Origin, Result, Service, Track};
use md5::{Digest, Md5};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// API version we speak (Navidrome supports it, plus the OpenSubsonic extensions).
const API_VERSION: &str = "1.16.1";
const CLIENT: &str = "dust";
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// A server to connect to, as entered by the listener.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
    pub url: String,
    pub user: String,
    pub password: String,
}

impl Server {
    /// Saved form (for the keychain): one JSON object.
    pub fn to_json(&self) -> String {
        serde_json::json!({"url": self.url, "user": self.user, "password": self.password}).to_string()
    }

    pub fn from_json(text: &str) -> Option<Server> {
        let v: Value = serde_json::from_str(text).ok()?;
        let s = |k: &str| v[k].as_str().map(str::to_string);
        Some(Server { url: s("url")?, user: s("user")?, password: s("password")? })
    }
}

struct Inner {
    agent: ureq::Agent,
    /// Server address without a trailing slash, e.g. `http://192.168.1.10:4533`.
    base: String,
    user: String,
    /// `md5(password + salt)`, sent instead of the password.
    token: String,
    salt: String,
    /// The server supports OpenSubsonic's synced lyrics (`getLyricsBySongId`).
    song_lyrics: AtomicBool,
    /// Server ids behind dust's numeric track and playlist ids.
    ids: Mutex<HashMap<u64, String>>,
}

/// A connected server. Cheap to clone and share between threads.
#[derive(Clone)]
pub struct Subsonic(Arc<Inner>);

/// dust's number for a server id: a stable 64-bit FNV-1a hash (never 0).
pub fn local_id(id: &str) -> u64 {
    let hash = id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    hash.max(1)
}

impl Subsonic {
    /// Connect and check the credentials (`ping`).
    pub fn connect(server: &Server) -> Result<Subsonic> {
        let base =
            normalize_url(&server.url).ok_or_else(|| Error::Other("Enter the server's address, e.g. http://192.168.1.10:4533".into()))?;
        if server.user.trim().is_empty() {
            return Err(Error::Other("Enter your user name".into()));
        }
        let mut salt = [0u8; 8];
        getrandom::fill(&mut salt).expect("OS RNG");
        let salt: String = salt.iter().map(|b| format!("{b:02x}")).collect();
        let token = hex(&Md5::digest(format!("{}{salt}", server.password).as_bytes()));
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent(format!("dust/{}", env!("CARGO_PKG_VERSION")))
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(CALL_TIMEOUT))
            .build()
            .into();
        let user = server.user.trim().to_string();
        let client =
            Subsonic(Arc::new(Inner { agent, base, user, token, salt, song_lyrics: AtomicBool::new(false), ids: Mutex::default() }));
        client.call("ping", &[]).map_err(|e| connect_error(client.host(), e))?;
        // OpenSubsonic servers list their extensions; plain Subsonic ones don't have
        // the call.
        if let Ok(r) = client.call("getOpenSubsonicExtensions", &[]) {
            let lyrics = r["openSubsonicExtensions"].as_array().into_iter().flatten().any(|e| e["name"] == "songLyrics");
            client.0.song_lyrics.store(lyrics, Ordering::Relaxed);
        }
        Ok(client)
    }

    /// A server that was never reached, for tests that don't call it.
    #[cfg(test)]
    pub fn offline(url: &str, user: &str) -> Subsonic {
        Subsonic(Arc::new(Inner {
            agent: ureq::Agent::new_with_defaults(),
            base: normalize_url(url).expect("a server address"),
            user: user.into(),
            token: String::new(),
            salt: String::new(),
            song_lyrics: AtomicBool::new(false),
            ids: Mutex::default(),
        }))
    }

    /// The user name, shown as the account.
    pub fn name(&self) -> &str {
        &self.0.user
    }

    /// The server's address without the scheme, e.g. `music.example.com`.
    pub fn host(&self) -> &str {
        self.0.base.split_once("://").map_or(&self.0.base, |(_, rest)| rest)
    }

    /// The URL of a cover picture (its [`Art`] is the server's cover art id), `size`
    /// px square.
    pub fn art_url(&self, art: &Art, size: u32) -> String {
        self.url("getCoverArt", &[("id", art.as_str()), ("size", &size.to_string())])
    }

    /// Full URL of an API method, with authentication.
    fn url(&self, method: &str, params: &[(&str, &str)]) -> String {
        let i = &self.0;
        let mut url = format!("{}/rest/{method}?u={}&t={}&s={}&v={API_VERSION}&c={CLIENT}&f=json", i.base, enc(&i.user), i.token, i.salt);
        for (k, v) in params {
            url.push('&');
            url.push_str(k);
            url.push('=');
            url.push_str(&enc(v));
        }
        url
    }

    /// Call `method`; the reply's `subsonic-response` object, or the server's error.
    fn call(&self, method: &str, params: &[(&str, &str)]) -> Result<Value> {
        let resp = self.0.agent.get(&self.url(method, params)).call().map_err(http)?;
        let body: Value = resp.into_body().read_json().map_err(|e| Error::Other(format!("Not a Subsonic reply: {e}")))?;
        parse::response(body)
    }

    /// The server id behind one of dust's numeric ids.
    fn server_id(&self, id: u64) -> Result<String> {
        self.0.ids.lock().unwrap().get(&id).cloned().ok_or_else(|| Error::Other("This item is no longer known; open it again".into()))
    }

    /// dust's number for a server id, remembered for the way back.
    fn remember(&self, id: &str) -> u64 {
        let local = local_id(id);
        self.0.ids.lock().unwrap().entry(local).or_insert_with(|| id.to_string());
        local
    }

    /// Learn the server ids of tracks that came from elsewhere (a restored queue).
    pub fn remember_tracks(&self, tracks: &[Track]) {
        for track in tracks {
            if let Origin::Server(song) = &track.origin {
                self.remember(&song.id);
            }
        }
    }
}

/// Why a first connection failed, in words for the connect form.
fn connect_error(host: &str, e: Error) -> Error {
    let message = match e {
        // A refused login says why ("Wrong username or password").
        Error::Api { message, .. } => message,
        Error::Http { status, .. } => format!("{host} answered with HTTP {status}. Is this the address of your Navidrome server?"),
        Error::Network { message: m, .. } => format!("Couldn't reach {host}. Check the address and that the server is running. ({m})"),
        Error::Other(_) => format!("{host} doesn't look like a Navidrome or Subsonic server."),
        e => e.to_string(),
    };
    Error::Other(message)
}

/// A failed request to the server, as an error naming it.
fn http(e: ureq::Error) -> Error {
    Service::Subsonic.http(e)
}

/// The server answered with an error.
fn api(message: impl Into<String>) -> Error {
    Service::Subsonic.api(message)
}

/// `http://host:port` from what was typed: scheme added if missing, trailing
/// slashes and a pasted `/app` or `/rest` path removed.
fn normalize_url(input: &str) -> Option<String> {
    let mut url = input.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return None;
    }
    if !url.contains("://") {
        url = format!("http://{url}");
    }
    for suffix in ["/app", "/rest"] {
        if let Some(stripped) = url.strip_suffix(suffix) {
            url = stripped.to_string();
        }
    }
    Some(url)
}

/// Percent-encode a query value.
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_normalized() {
        assert_eq!(normalize_url("192.168.1.10:4533/").as_deref(), Some("http://192.168.1.10:4533"));
        assert_eq!(normalize_url(" https://music.example.com/app/ ").as_deref(), Some("https://music.example.com"));
        assert_eq!(normalize_url("http://nas:4533/rest").as_deref(), Some("http://nas:4533"));
        assert_eq!(normalize_url("  "), None);
    }

    #[test]
    fn connect_errors_say_what_to_check() {
        let say = |e| connect_error("nas:4533", e).to_string();
        assert_eq!(say(api("Wrong username or password")), "Wrong username or password");
        assert!(say(http(ureq::Error::StatusCode(404))).starts_with("nas:4533 answered with HTTP 404."));
        assert!(say(Service::Subsonic.network("Connection refused")).starts_with("Couldn't reach nas:4533."));
        assert!(say(Error::Other("Not a Subsonic reply: eof".into())).contains("doesn't look like"));
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(enc("AC/DC & co"), "AC%2FDC%20%26%20co");
        assert_eq!(enc("a-b_c.d~"), "a-b_c.d~");
    }

    #[test]
    fn token_is_md5_of_password_and_salt() {
        // The example from the Subsonic API documentation.
        assert_eq!(hex(&Md5::digest(b"sesamec19b2d")), "26719a1196d2a940705a59634eb18eab");
    }

    #[test]
    fn cover_art_comes_from_the_server() {
        let server = Subsonic::offline("nas:4533", "me");
        let url = server.art_url(&Art::new("al-5F81DCwcfjVByziJAH9maf_0").unwrap(), 300);
        assert!(url.starts_with("http://nas:4533/rest/getCoverArt?u=me&"), "{url}");
        assert!(url.ends_with("&id=al-5F81DCwcfjVByziJAH9maf_0&size=300"), "{url}");
    }

    #[test]
    fn local_ids_are_stable_and_non_zero() {
        assert_eq!(local_id("qPyR7XLJOiu93qWw1n06xy"), local_id("qPyR7XLJOiu93qWw1n06xy"));
        assert_ne!(local_id("a"), local_id("b"));
        assert_ne!(local_id(""), 0);
    }

    #[test]
    fn servers_round_trip_through_the_keychain_form() {
        let s = Server { url: "http://nas:4533".into(), user: "me".into(), password: "p\"w".into() };
        assert_eq!(Server::from_json(&s.to_json()), Some(s));
        assert_eq!(Server::from_json("{}"), None);
    }
}
