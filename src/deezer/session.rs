//! gw-light transport: cookie jar, CSRF (`api_token`) handling and the raw call.

use super::{Error, Result};
use serde_json::Value;
use std::time::Duration;

const GATEWAY: &str = "https://www.deezer.com/ajax/gw-light.php";
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
/// api_token value gw-light expects before we have a real one.
const NO_TOKEN: &str = "null";

/// Per-call limit for API requests. Streams opt out (see `open_stream`).
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);

pub fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(USER_AGENT)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(CALL_TIMEOUT))
        .build()
        .into()
}

/// CSRF token plus every cookie the server set. The token is bound to the whole
/// cookie set (sid, dzr_uniq_id, bot-protection cookies), not just `sid`.
pub struct Session {
    pub api_token: String,
    cookies: Vec<(String, String)>,
}

impl Session {
    pub fn new() -> Self {
        Self { api_token: NO_TOKEN.into(), cookies: Vec::new() }
    }

    pub fn reset_token(&mut self) {
        self.api_token = NO_TOKEN.into();
    }

    fn cookie_header(&self, arl: &str) -> String {
        let arl = (!arl.is_empty()).then(|| format!("arl={arl}"));
        let others = self.cookies.iter().filter(|(k, _)| k != "arl").map(|(k, v)| format!("{k}={v}"));
        arl.into_iter().chain(others).collect::<Vec<_>>().join("; ")
    }

    fn store(&mut self, set_cookie: &str) {
        let Some((name, rest)) = set_cookie.split_once('=') else { return };
        let name = name.trim();
        let value = rest.split(';').next().unwrap_or("").trim();
        let expired = value == "deleted" || set_cookie.to_ascii_lowercase().contains("max-age=0");
        self.cookies.retain(|(k, _)| k != name);
        if !expired {
            self.cookies.push((name.to_string(), value.to_string()));
        }
    }
}

/// One gw-light call. `extra` adds URL query parameters (e.g. `gateway_input`).
pub fn call(agent: &ureq::Agent, arl: &str, session: &mut Session, method: &str, body: Value, extra: &[(&str, &str)]) -> Result<Value> {
    let resp = agent
        .post(GATEWAY)
        .query("method", method)
        .query("input", "3")
        .query("api_version", "1.0")
        .query("api_token", &session.api_token)
        .query_pairs(extra.iter().copied())
        .header("Cookie", &session.cookie_header(arl))
        .send_json(body)?;
    for c in resp.headers().get_all("set-cookie") {
        if let Ok(c) = c.to_str() {
            session.store(c);
        }
    }
    results(method, resp.into_body().read_json()?)
}

/// The `results` of a gw-light reply, or its `error` object as an [`Error`].
fn results(method: &str, reply: Value) -> Result<Value> {
    match &reply["error"] {
        Value::Object(o) if o.contains_key("VALID_TOKEN_REQUIRED") => Err(Error::InvalidToken),
        Value::Object(o) if !o.is_empty() => Err(Error::Api(format!("{method}: {}", Value::Object(o.clone())))),
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
        assert_eq!(s.cookie_header("A"), "arl=A; dzr_uniq_id=x; sid=def");
        assert_eq!(Session::new().cookie_header(""), "");
    }

    #[test]
    fn error_replies() {
        assert!(matches!(results("m", json!({"error": {"VALID_TOKEN_REQUIRED": "Invalid CSRF token"}})), Err(Error::InvalidToken)));
        assert!(matches!(results("m", json!({"error": {"DATA_ERROR": "x"}})), Err(Error::Api(_))));
        assert_eq!(results("m", json!({"error": [], "results": {"a": 1}})).unwrap(), json!({"a": 1}));
    }

    /// Network test: CSRF token from getUserData must be accepted on the next call.
    #[test]
    #[ignore]
    fn session_csrf_roundtrip() {
        let agent = http_agent();
        let mut session = Session::new();
        let data = call(&agent, "", &mut session, "deezer.getUserData", json!({}), &[]).unwrap();
        session.api_token = parse::text(&data["checkForm"]);
        let r = call(&agent, "", &mut session, "deezer.pageSearch", json!({"query": "daft punk", "start": 0, "nb": 5}), &[]).unwrap();
        assert!(!parse::tracks(&r["TRACK"]["data"]).is_empty(), "{r}");
    }
}
