//! A stand-in for Deezer's gw-light, for tests: a family plan whose owner (user 1)
//! can act as its members through the `account_id` cookie, as deezer.com does, with
//! CSRF tokens that can be made stale and a switch for Deezer no longer taking
//! a member.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// The session cookie the fake takes; any other is logged out.
pub const ARL: &str = "good-arl";
/// The plan's owner; 2 can be listened as, 3 made their profile private.
pub const OWNER: u64 = 1;

pub struct Request {
    pub method: String,
    /// Who Deezer took the caller for.
    pub user: u64,
    pub cookies: String,
    /// Answered, rather than refused for a stale token.
    pub served: bool,
}

pub struct State {
    /// Whether the `account_id` cookie of a member is taken (else it's the owner).
    pub honour_members: bool,
    /// Bumped to make every CSRF token handed out so far stale.
    pub token_round: u32,
    /// What came in, in order.
    pub requests: Vec<Request>,
}

/// The gateway, serving on a local port while this is kept; gw-light calls on this
/// thread go to it.
pub struct Fake {
    state: Arc<Mutex<State>>,
}

impl Fake {
    pub fn start() -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
        let url = format!("http://{}/ajax/gw-light.php", listener.local_addr().expect("its address"));
        let state = Arc::new(Mutex::new(State { honour_members: true, token_round: 0, requests: Vec::new() }));
        let shared = state.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let state = shared.clone();
                std::thread::spawn(move || serve(stream, &state));
            }
        });
        super::session::TEST_GATEWAY.with(|g| *g.borrow_mut() = Some(url));
        Fake { state }
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The users the calls of `method` were served as, in order (refused ones left
    /// out).
    pub fn served(&self, method: &str) -> Vec<u64> {
        self.state().requests.iter().filter(|r| r.method == method && r.served).map(|r| r.user).collect()
    }

    /// The cookies of the last call.
    pub fn last_cookies(&self) -> String {
        self.state().requests.last().map(|r| r.cookies.clone()).unwrap_or_default()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        super::session::TEST_GATEWAY.with(|g| *g.borrow_mut() = None);
    }
}

/// Requests on one connection (kept alive) until it closes.
fn serve(stream: TcpStream, state: &Mutex<State>) {
    let Ok(read) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read);
    let mut writer = stream;
    loop {
        let mut first = String::new();
        if reader.read_line(&mut first).unwrap_or(0) == 0 {
            return;
        }
        let mut headers = HashMap::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
        let length = headers.get("content-length").and_then(|l| l.parse().ok()).unwrap_or(0);
        let mut body = vec![0; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let target = first.split_whitespace().nth(1).unwrap_or("");
        let query: HashMap<&str, &str> =
            target.split_once('?').map_or("", |(_, q)| q).split('&').filter_map(|kv| kv.split_once('=')).collect();
        let cookies = headers.get("cookie").cloned().unwrap_or_default();
        let (reply, set_cookie) = answer(
            &mut state.lock().unwrap_or_else(PoisonError::into_inner),
            query.get("method").copied().unwrap_or(""),
            query.get("api_token").copied().unwrap_or(""),
            &cookies,
        );
        let body = reply.to_string();
        let cookie_line = set_cookie.map(|c| format!("Set-Cookie: {c}\r\n")).unwrap_or_default();
        let response =
            format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{cookie_line}Content-Length: {}\r\n\r\n{body}", body.len());
        if writer.write_all(response.as_bytes()).is_err() {
            return;
        }
    }
}

fn cookie<'a>(cookies: &'a str, name: &str) -> Option<&'a str> {
    cookies.split(';').filter_map(|c| c.trim().split_once('=')).find(|(k, _)| *k == name).map(|(_, v)| v)
}

/// One gw-light call: its reply, and a cookie to set.
fn answer(state: &mut State, method: &str, token: &str, cookies: &str) -> (Value, Option<String>) {
    let logged_in = cookie(cookies, "arl") == Some(ARL);
    let asked: Option<u64> = cookie(cookies, "account_id").and_then(|id| id.parse().ok());
    // The plan's members that can be listened as; anyone else is the owner.
    let user = match asked {
        Some(2) if state.honour_members => 2,
        _ => OWNER,
    };
    let user = if logged_in { user } else { 0 };
    let valid_token = format!("tok-{user}-{}", state.token_round);
    let served = method == "deezer.getUserData" || token == valid_token;
    state.requests.push(Request { method: method.into(), user, cookies: cookies.into(), served });
    let name = ["", "Parent", "Kid"].get(user as usize).copied().unwrap_or("");
    match method {
        "deezer.getUserData" => {
            let reply = json!({"results": {
                "USER": {
                    "USER_ID": user.to_string(),
                    "BLOG_NAME": name,
                    "LOVEDTRACKS_ID": (100 + user).to_string(),
                    "OPTIONS": {"license_token": if logged_in { "licence" } else { "" }},
                    "MULTI_ACCOUNT": {"ENABLED": true, "IS_SUB_ACCOUNT": user != OWNER},
                },
                "checkForm": if logged_in { valid_token } else { String::new() },
            }});
            // As deezer.com does: the account's own id, whatever was asked for.
            (reply, logged_in.then(|| format!("account_id={OWNER}; path=/")))
        }
        _ if token != valid_token => (json!({"error": {"VALID_TOKEN_REQUIRED": "Invalid CSRF token"}, "results": {}}), None),
        "deezer.getChildAccounts" => (
            json!({"results": [
                {"USER_ID": "1", "BLOG_NAME": "Parent", "EXTRA_FAMILY": {"IS_LOGGABLE_AS": true}},
                {"USER_ID": "2", "BLOG_NAME": "Kid", "EXTRA_FAMILY": {"IS_LOGGABLE_AS": true}},
                {"USER_ID": "3", "BLOG_NAME": "Teen", "EXTRA_FAMILY": {"IS_LOGGABLE_AS": false}},
            ]}),
            None,
        ),
        _ => (json!({"results": {"USER_ID": user}}), None),
    }
}
