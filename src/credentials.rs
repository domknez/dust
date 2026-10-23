//! Stored logins, kept in the OS keychain: one secret per music source (the Deezer
//! session's `arl` cookie, a Navidrome / Subsonic server's address, user name and
//! password), under the source's [`SourceId::login_key`].

use crate::source::SourceId;
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

const SERVICE: &str = "dust";
/// One keychain item holds every login, so macOS asks for access once, not once
/// per login.
const ACCOUNT: &str = "logins";
/// Where 0.18 and earlier kept them, one item each: the keychain account and the
/// source whose login it was.
const OLD_ITEMS: [(&str, SourceId); 2] = [("arl", crate::deezer::ID), ("navidrome", crate::subsonic::ID)];

/// The platform's secure store, installed as keyring-core's default once. Tests get
/// an in-memory one: they never touch (or prompt for) the real keychain.
static STORE: LazyLock<Result<(), keyring_core::Error>> = LazyLock::new(|| {
    #[cfg(test)]
    let store = keyring_core::mock::Store::new()?;
    #[cfg(all(not(test), target_os = "macos"))]
    let store = apple_native_keyring_store::keychain::Store::new()?;
    #[cfg(all(not(test), windows))]
    let store = windows_native_keyring_store::Store::new()?;
    #[cfg(all(not(test), unix, not(target_os = "macos")))]
    let store = dbus_secret_service_keyring_store::Store::new()?;
    keyring_core::set_default_store(store);
    Ok(())
});

/// The stored logins, read from the keychain on first use and kept here after.
static LOGINS: Mutex<Option<Logins>> = Mutex::new(None);

/// Each source's secret by its login key, kept as one JSON object
/// (`{"arl": "…", "server": "{…}"}`).
#[derive(Default, Clone, PartialEq, Debug)]
struct Logins(BTreeMap<String, String>);

impl Logins {
    fn to_json(&self) -> String {
        serde_json::json!(self.0).to_string()
    }

    /// Every login in the item; fields that aren't text (`null` for a login 0.19
    /// didn't have) are none.
    fn from_json(text: &str) -> Logins {
        let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
        let logins = v.as_object().into_iter().flatten().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())));
        Logins(logins.collect())
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
    let logins = Logins(OLD_ITEMS.iter().filter_map(|(account, source)| Some((source.login_key.to_string(), take(account)?))).collect());
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

/// The stored login of `source`, if any.
pub fn load(source: SourceId) -> Option<String> {
    with_logins(|l| l.0.get(source.login_key).cloned())
}

pub fn save(source: SourceId, secret: &str) {
    update(|l| {
        l.0.insert(source.login_key.to_string(), secret.to_string());
    });
}

pub fn clear(source: SourceId) {
    update(|l| {
        l.0.remove(source.login_key);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logins(entries: &[(&str, &str)]) -> Logins {
        Logins(entries.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    #[test]
    fn logins_round_trip() {
        let both = logins(&[("arl", "abc"), ("server", r#"{"url":"http://nas"}"#)]);
        assert_eq!(Logins::from_json(&both.to_json()), both);
        assert_eq!(Logins::from_json("not json"), Logins::default());
    }

    #[test]
    fn reads_the_item_0_19_wrote() {
        let saved = r#"{"arl":"abc","server":"{\"url\":\"http://nas\",\"user\":\"me\",\"password\":\"pw\"}"}"#;
        let read = Logins::from_json(saved);
        assert_eq!(read.0.get(crate::deezer::ID.login_key).map(String::as_str), Some("abc"));
        let server = read.0.get(crate::subsonic::ID.login_key).and_then(|t| crate::subsonic::Server::from_json(t)).unwrap();
        assert_eq!((server.url.as_str(), server.user.as_str()), ("http://nas", "me"));
        assert_eq!(Logins::from_json(r#"{"arl":"abc","server":null}"#), logins(&[("arl", "abc")]), "a login 0.19 didn't have");
        assert_eq!(Logins::from_json(&read.to_json()), read, "and writes it back the same");
    }
}
