//! The stored Deezer session (`arl` cookie), kept in the OS keychain.

use std::sync::LazyLock;

const SERVICE: &str = "dust";
const ACCOUNT: &str = "arl";

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

fn entry() -> Option<keyring_core::Entry> {
    if let Err(e) = &*STORE {
        log_warn!("no keychain available: {e}");
        return None;
    }
    keyring_core::Entry::new(SERVICE, ACCOUNT).ok()
}

pub fn load() -> Option<String> {
    entry().and_then(|e| e.get_password().ok())
}

pub fn save(arl: &str) {
    if let Some(e) = entry()
        && let Err(err) = e.set_password(arl)
    {
        log_warn!("could not save session to the keychain: {err}");
    }
}

pub fn clear() {
    if let Some(e) = entry() {
        let _ = e.delete_credential();
    }
}
