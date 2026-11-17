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
        let v: serde_json::Value = serde_json::from_str(text).unwrap_or_else(|e| {
            log_warn!("the stored logins aren't readable ({e}); starting without them");
            serde_json::Value::Null
        });
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

/// Run `f` on the logins, reading them from the keychain the first time. None
/// while the keychain can't be read: nothing is cached then, so the next call
/// tries again, and nothing can be written over logins that weren't seen. (An
/// item that's there but unreadable counts as empty: its logins are lost anyway.)
fn with_logins<R>(f: impl FnOnce(&mut Logins) -> R) -> Option<R> {
    let mut slot = LOGINS.lock().unwrap_or_else(|e| e.into_inner());
    cached(&mut slot, read).map(f)
}

/// The logins in `slot`, filled by `read` when empty; a failed read leaves it empty.
fn cached(slot: &mut Option<Logins>, read: impl FnOnce() -> Option<Logins>) -> Option<&mut Logins> {
    if slot.is_none() {
        *slot = read();
    }
    slot.as_mut()
}

/// The logins in the keychain, or None when it can't be read.
fn read() -> Option<Logins> {
    let entry = entry_for(ACCOUNT)?;
    match entry.get_password() {
        Ok(text) => Some(Logins::from_json(&text)),
        Err(keyring_core::Error::NoEntry) => Some(migrate()),
        Err(e) => {
            log_warn!("could not read logins from the keychain: {e}");
            None
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
        if let Err(e) = write(&logins) {
            log_warn!("{e}");
        }
    }
    logins
}

fn write(logins: &Logins) -> Result<(), String> {
    let entry = entry_for(ACCOUNT).ok_or("there's no keychain to keep it in")?;
    let result = if *logins == Logins::default() { entry.delete_credential() } else { entry.set_password(&logins.to_json()) };
    match result {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("the keychain refused it ({e})")),
    }
}

/// Change the logins and save them (only if they changed: every write to the
/// keychain may ask for access again).
fn update(f: impl FnOnce(&mut Logins)) -> Result<(), String> {
    let saved = with_logins(|logins| {
        let before = logins.clone();
        f(logins);
        if *logins != before { write(logins) } else { Ok(()) }
    });
    let saved = saved.unwrap_or_else(|| Err("the keychain couldn't be read (writing now would drop the other logins)".into()));
    if let Err(e) = &saved {
        log_warn!("logins not saved: {e}");
    }
    saved
}

/// The stored login of `source`, if any.
pub fn load(source: SourceId) -> Option<String> {
    with_logins(|l| l.0.get(source.login_key).cloned()).flatten()
}

/// Keep `secret` as the login of `source`; Err says why it couldn't be.
pub fn save(source: SourceId, secret: &str) -> Result<(), String> {
    update(|l| {
        l.0.insert(source.login_key.to_string(), secret.to_string());
    })
}

/// Where a source keeps which member of the account was picked (a Deezer family plan).
fn profile_key(source: SourceId) -> String {
    format!("{}.profile", source.login_key)
}

/// The family member picked for `source`, if any.
pub fn load_profile(source: SourceId) -> Option<u64> {
    with_logins(|l| l.0.get(&profile_key(source))?.parse().ok()).flatten()
}

/// Remember the member picked for `source` (none: the account itself).
pub fn save_profile(source: SourceId, profile: Option<u64>) -> Result<(), String> {
    update(|l| match profile {
        Some(id) => {
            l.0.insert(profile_key(source), id.to_string());
        }
        None => {
            l.0.remove(&profile_key(source));
        }
    })
}

/// Forget the login of `source`; Err says why it couldn't be.
pub fn clear(source: SourceId) -> Result<(), String> {
    update(|l| {
        l.0.remove(source.login_key);
        l.0.remove(&profile_key(source));
    })
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
    fn a_failed_read_is_not_cached() {
        let mut slot = None;
        assert!(cached(&mut slot, || None).is_none(), "nothing to work on");
        assert!(slot.is_none(), "so a save can't write over logins it never saw");
        let read = cached(&mut slot, || Some(logins(&[("arl", "abc")]))).cloned();
        assert_eq!(read, Some(logins(&[("arl", "abc")])), "the next call reads again");
        assert_eq!(cached(&mut slot, || panic!("read once")).cloned(), read, "and keeps it");
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

    /// A source of its own: the other tests log Deezer and Navidrome in and out in
    /// the shared in-memory keychain while this runs.
    #[test]
    fn the_picked_profile_is_kept_with_the_login_and_cleared_with_it() {
        let own = SourceId { key: "profile-test", name: "Test", queue_file: "profile-test.json", login_key: "profile-test" };
        assert_eq!(load_profile(own), None);
        save(own, "secret").unwrap();
        save_profile(own, Some(42)).unwrap();
        assert_eq!(load_profile(own), Some(42));
        save_profile(own, None).unwrap();
        assert_eq!(load_profile(own), None);
        save_profile(own, Some(7)).unwrap();
        clear(own).unwrap();
        assert_eq!((load(own), load_profile(own)), (None, None), "logging out forgets the pick too");
    }
}
