//! Stored logins, kept in the OS keychain: the Deezer session (`arl` cookie) and a
//! Navidrome / Subsonic server (address, user name and password).

use crate::subsonic::Server;
use std::sync::{LazyLock, Mutex};

const SERVICE: &str = "dust";
/// One keychain item holds every login, so macOS asks for access once, not once
/// per login.
const ACCOUNT: &str = "logins";
/// Where 0.18 and earlier kept them, one item each.
const OLD_ARL: &str = "arl";
const OLD_SERVER: &str = "navidrome";

/// The platform's secure store, installed as keyring-core's default once.
static STORE: LazyLock<Result<(), keyring_core::Error>> = LazyLock::new(|| {
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new()?;
    #[cfg(windows)]
    let store = windows_native_keyring_store::Store::new()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    let store = dbus_secret_service_keyring_store::Store::new()?;
    keyring_core::set_default_store(store);
    Ok(())
});

/// The stored logins, read from the keychain on first use and kept here after.
static LOGINS: Mutex<Option<Logins>> = Mutex::new(None);

#[derive(Default, Clone, PartialEq, Debug)]
struct Logins {
    arl: Option<String>,
    /// The server login as JSON ([`Server::to_json`]).
    server: Option<String>,
}

impl Logins {
    fn to_json(&self) -> String {
        serde_json::json!({ "arl": self.arl, "server": self.server }).to_string()
    }

    fn from_json(text: &str) -> Logins {
        let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
        let s = |k: &str| v[k].as_str().map(str::to_string);
        Logins { arl: s("arl"), server: s("server") }
    }
}

fn entry_for(account: &str) -> Option<keyring_core::Entry> {
    if let Err(e) = &*STORE {
        log_warn!("no keychain available: {e}");
        return None;
    }
    keyring_core::Entry::new(SERVICE, account).ok()
}

/// Run `f` on the logins, reading them from the keychain the first time.
fn with_logins<R>(f: impl FnOnce(&mut Logins) -> R) -> R {
    let mut logins = LOGINS.lock().unwrap_or_else(|e| e.into_inner());
    f(logins.get_or_insert_with(read))
}

fn read() -> Logins {
    let Some(entry) = entry_for(ACCOUNT) else { return Logins::default() };
    match entry.get_password() {
        Ok(text) => Logins::from_json(&text),
        Err(keyring_core::Error::NoEntry) => migrate(),
        Err(e) => {
            log_warn!("could not read logins from the keychain: {e}");
            Logins::default()
        }
    }
}

/// Move the logins from their old items into the one item (once).
fn migrate() -> Logins {
    let take = |account: &str| {
        let entry = entry_for(account)?;
        let value = entry.get_password().ok()?;
        let _ = entry.delete_credential();
        Some(value)
    };
    let logins = Logins { arl: take(OLD_ARL), server: take(OLD_SERVER) };
    if logins != Logins::default() {
        log_info!("moved the stored logins into one keychain item");
        write(&logins);
    }
    logins
}

fn write(logins: &Logins) {
    let Some(entry) = entry_for(ACCOUNT) else { return };
    let result = if *logins == Logins::default() { entry.delete_credential() } else { entry.set_password(&logins.to_json()) };
    match result {
        Ok(()) | Err(keyring_core::Error::NoEntry) => {}
        Err(e) => log_warn!("could not save logins to the keychain: {e}"),
    }
}

/// Change the logins and save them (only if they changed: every write to the
/// keychain may ask for access again).
fn update(f: impl FnOnce(&mut Logins)) {
    with_logins(|logins| {
        let before = logins.clone();
        f(logins);
        if *logins != before {
            write(logins);
        }
    });
}

/// The Deezer session (`arl`), if any.
pub fn load() -> Option<String> {
    with_logins(|l| l.arl.clone())
}

pub fn save(arl: &str) {
    update(|l| l.arl = Some(arl.to_string()));
}

pub fn clear() {
    update(|l| l.arl = None);
}

/// The saved music server, if any.
pub fn load_server() -> Option<Server> {
    with_logins(|l| l.server.clone()).and_then(|text| Server::from_json(&text))
}

pub fn save_server(server: &Server) {
    update(|l| l.server = Some(server.to_json()));
}

pub fn clear_server() {
    update(|l| l.server = None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logins_round_trip() {
        let both = Logins { arl: Some("abc".into()), server: Some(r#"{"url":"http://nas"}"#.into()) };
        assert_eq!(Logins::from_json(&both.to_json()), both);
        let deezer_only = Logins { arl: Some("abc".into()), server: None };
        assert_eq!(Logins::from_json(&deezer_only.to_json()), deezer_only);
        assert_eq!(Logins::from_json("not json"), Logins::default());
    }
}
