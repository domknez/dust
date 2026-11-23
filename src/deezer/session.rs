//! gw-light transport: cookie jar, CSRF (`api_token`) handling and the raw call.

use super::{Error, Result, SERVICE, http};
use serde_json::Value;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

const GATEWAY: &str = "https://www.deezer.com/ajax/gw-light.php";
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
/// api_token value gw-light expects before we have a real one.
const NO_TOKEN: &str = "null";

/// Per-call limit for API requests. Streams opt out (see `open_stream`).
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Where gw-light calls go: Deezer's, or a test's stand-in on this thread (see
/// `deezer::fake`).
fn gateway() -> std::borrow::Cow<'static, str> {
    #[cfg(test)]
    if let Some(url) = TEST_GATEWAY.with(|g| g.borrow().clone()) {
        return url.into();
    }
    GATEWAY.into()
}

#[cfg(test)]
thread_local! {
    pub static TEST_GATEWAY: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

pub fn http_agent() -> ureq::Agent {
    crate::http::agent(USER_AGENT, Some(CALL_TIMEOUT))
}

/// CSRF token plus every cookie the server set. The token is bound to the whole
/// cookie set (sid, dzr_uniq_id, bot-protection cookies), not just `sid`.
pub struct Session {
    pub api_token: String,
    cookies: Vec<(String, String)>,
    /// The family profile acted as: sent as the `account_id` cookie, which is how
    /// deezer.com itself picks the member (library, favourites, history).
    profile: Option<u64>,
}

impl Session {
    pub fn new() -> Self {
        Self { api_token: NO_TOKEN.into(), cookies: Vec::new(), profile: None }
    }

    /// Act as this family profile from now on (the account owner's own when none).
    pub fn set_profile(&mut self, profile: Option<u64>) {
        self.profile = profile;
    }

    pub fn reset_token(&mut self) {
        self.api_token = NO_TOKEN.into();
    }

    fn cookie_header(&self, arl: &str) -> String {
        let arl = (!arl.is_empty()).then(|| format!("arl={arl}"));
        // A chosen profile overrides whatever `account_id` the server last set.
        let pinned = self.profile.map(|id| format!("account_id={id}"));
        let is_pinned = pinned.is_some();
        let others = self.cookies.iter().filter(|(k, _)| k != "arl" && !(is_pinned && k == "account_id")).map(|(k, v)| format!("{k}={v}"));
        arl.into_iter().chain(others).chain(pinned).collect::<Vec<_>>().join("; ")
    }

    fn store(&mut self, set_cookie: &str) {
        let Some((name, rest)) = set_cookie.split_once('=') else { return };
        let name = name.trim();
        let value = rest.split(';').next().unwrap_or("").trim();
        let expired = value == "deleted" || expires_now(rest);
        self.cookies.retain(|(k, _)| k != name);
        if !expired {
            self.cookies.push((name.to_string(), value.to_string()));
        }
    }
}

/// Whether a cookie's attributes (`Max-Age=0`, an `Expires` date gone by) say
/// to drop it.
fn expires_now(attributes: &str) -> bool {
    attributes.split(';').skip(1).filter_map(|a| a.split_once('=')).any(|(name, value)| match name.trim().to_ascii_lowercase().as_str() {
        "max-age" => value.trim().parse::<i64>().is_ok_and(|secs| secs <= 0),
        "expires" => jiff::fmt::rfc2822::parse(value.trim()).is_ok_and(|at| at.timestamp() <= jiff::Timestamp::now()),
        _ => false,
    })
}

/// One gw-light call. `extra` adds URL query parameters (e.g. `gateway_input`).
/// The session is locked only to read the token and cookies and to keep the new
/// cookies, never across the request: calls run side by side.
pub fn call(agent: &ureq::Agent, arl: &str, session: &Mutex<Session>, method: &str, body: Value, extra: &[(&str, &str)]) -> Result<Value> {
    let (token, cookies) = {
        let s = lock(session);
        (s.api_token.clone(), s.cookie_header(arl))
    };
    let resp = agent
        .post(gateway().as_ref())
        .query("method", method)
        .query("input", "3")
        .query("api_version", "1.0")
        .query("api_token", &token)
        .query_pairs(extra.iter().copied())
        .header("Cookie", &cookies)
        .send_json(body)
        .map_err(http)?;
    {
        let mut s = lock(session);
        for c in resp.headers().get_all("set-cookie") {
            if let Ok(c) = c.to_str() {
                s.store(c);
            }
        }
    }
    results(method, resp.into_body().read_json().map_err(http)?)
}

/// The session, also after a thread panicked holding it (it's only ever left
/// whole: each change is one assignment).
pub fn lock(session: &Mutex<Session>) -> MutexGuard<'_, Session> {
    session.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The `results` of a gw-light reply, or its `error` object as an [`Error`].
fn results(method: &str, reply: Value) -> Result<Value> {
    match &reply["error"] {
        Value::Object(o) if o.contains_key("VALID_TOKEN_REQUIRED") => Err(Error::InvalidToken),
        Value::Object(o) if !o.is_empty() => {
            // Deezer names errors by the keys of its error object, and sometimes in
            // its values ({"FAVORITE": "ERROR_DATA_EXISTS"}).
            let codes = o.iter().flat_map(|(k, v)| std::iter::once(k.clone()).chain(v.as_str().map(str::to_string))).collect();
            Err(SERVICE.api_codes(format!("{method}: {}", Value::Object(o.clone())), codes))
        }
        _ => Ok(reply["results"].clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deezer::parse;
    use serde_json::json;

    #[test]
    fn cookie_jar_replaces_and_expires() {
        let mut s = Session::new();
        s.store("sid=abc; path=/");
        s.store("dzr_uniq_id=x; path=/");
        s.store("sid=def; path=/");
        s.store("account_id=deleted; Max-Age=0");
        s.store("dzr_uniq_id=x; Max-Age=03600");
        s.store("old=1; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/");
        s.store("later=1; expires=Fri, 01 Jan 2100 00:00:00 GMT");
        assert_eq!(s.cookie_header("A"), "arl=A; sid=def; dzr_uniq_id=x; later=1", "an hour's Max-Age and a future date keep");
        assert_eq!(Session::new().cookie_header(""), "");
    }

    #[test]
    fn chosen_profile_overrides_the_account_id_cookie() {
        let mut s = Session::new();
        s.set_profile(Some(42));
        s.store("account_id=7; path=/");
        s.store("sid=abc; path=/");
        assert_eq!(s.cookie_header("A"), "arl=A; sid=abc; account_id=42");
        let mut owner = Session::new();
        owner.store("account_id=7; path=/");
        assert_eq!(owner.cookie_header("A"), "arl=A; account_id=7", "no profile chosen: the server's cookie stands");
    }

    #[test]
    fn error_replies() {
        assert!(matches!(results("m", json!({"error": {"VALID_TOKEN_REQUIRED": "Invalid CSRF token"}})), Err(Error::InvalidToken)));
        let e = results("m", json!({"error": {"DATA_ERROR": "x"}})).unwrap_err();
        assert!(e.has_code("DATA_ERROR") && !e.has_code("ERROR_DATA_EXISTS"));
        assert!(results("m", json!({"error": {"FAVORITE": "ERROR_DATA_EXISTS"}})).unwrap_err().has_code("ERROR_DATA_EXISTS"));
        assert_eq!(results("m", json!({"error": [], "results": {"a": 1}})).unwrap(), json!({"a": 1}));
    }

    /// Network test: CSRF token from getUserData must be accepted on the next call.
    #[test]
    #[ignore]
    fn session_csrf_roundtrip() {
        let agent = http_agent();
        let session = Mutex::new(Session::new());
        let data = call(&agent, "", &session, "deezer.getUserData", json!({}), &[]).unwrap();
        lock(&session).api_token = parse::text(&data["checkForm"]);
        let r = call(&agent, "", &session, "deezer.pageSearch", json!({"query": "daft punk", "start": 0, "nb": 5}), &[]).unwrap();
        assert!(!parse::tracks(&r["TRACK"]["data"]).is_empty(), "{r}");
    }
}
