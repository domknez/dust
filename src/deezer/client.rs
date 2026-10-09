//! The [`Deezer`] handle: login and authenticated gw-light calls.

use super::parse::{number, text};
use super::session::{self, Session};
use super::{Error, Result};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

pub(super) struct Inner {
    pub agent: ureq::Agent,
    pub arl: String,
    pub session: Mutex<Session>,
    pub license_token: String,
    pub user_id: u64,
    pub loved_id: u64,
    pub name: String,
}

/// Logged-in Deezer account. Cheap to clone and share between threads.
#[derive(Clone)]
pub struct Deezer(pub(super) Arc<Inner>);

impl Deezer {
    /// Log in with an `arl` cookie value taken from a browser session.
    pub fn login(arl: &str) -> Result<Deezer> {
        let arl = arl.trim().to_string();
        let agent = session::http_agent();
        let mut session = Session::new();
        let data = session::call(&agent, &arl, &mut session, "deezer.getUserData", json!({}), &[])?;
        let user = &data["USER"];
        let user_id = number(&user["USER_ID"]);
        if user_id == 0 {
            return Err(Error::SessionExpired);
        }
        session.api_token = text(&data["checkForm"]);
        let license_token = text(&user["OPTIONS"]["license_token"]);
        if license_token.is_empty() {
            return Err(Error::NoStreamingLicense);
        }
        Ok(Deezer(Arc::new(Inner {
            agent,
            arl,
            session: Mutex::new(session),
            license_token,
            user_id,
            loved_id: number(&user["LOVEDTRACKS_ID"]),
            name: text(&user["BLOG_NAME"]),
        })))
    }

    /// The session cookie, for storing in the keychain.
    pub fn arl(&self) -> &str {
        &self.0.arl
    }

    /// Display name of the account.
    pub fn name(&self) -> &str {
        &self.0.name
    }

    pub(super) fn call(&self, method: &str, body: Value) -> Result<Value> {
        self.call_with(method, body, &[])
    }

    /// Any gw-light method, for diagnostics (`--debug-api`). A `gateway_input` key in
    /// `body` is sent as that query parameter instead (as `page.get` expects).
    pub fn call_raw(&self, method: &str, mut body: Value) -> Result<Value> {
        let gateway = body.as_object_mut().and_then(|o| o.remove("gateway_input")).map(|g| g.to_string());
        match gateway {
            Some(input) => self.call_with(method, body, &[("gateway_input", &input)]),
            None => self.call(method, body),
        }
    }

    /// Authenticated call; refreshes an expired CSRF token once and retries.
    pub(super) fn call_with(&self, method: &str, body: Value, extra: &[(&str, &str)]) -> Result<Value> {
        let inner = &self.0;
        let mut session = inner.session.lock().unwrap();
        match session::call(&inner.agent, &inner.arl, &mut session, method, body.clone(), extra) {
            Err(Error::InvalidToken) => {
                session.reset_token();
                let data = session::call(&inner.agent, &inner.arl, &mut session, "deezer.getUserData", json!({}), &[])?;
                // A rejected token with no logged-in user behind it: the session is gone.
                if number(&data["USER"]["USER_ID"]) == 0 {
                    return Err(Error::SessionExpired);
                }
                session.api_token = text(&data["checkForm"]);
                session::call(&inner.agent, &inner.arl, &mut session, method, body, extra)
            }
            result => result,
        }
    }
}
