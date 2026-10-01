//! The stored Deezer session (`arl` cookie), kept in the OS keychain.

const SERVICE: &str = "dust";
const ACCOUNT: &str = "arl";

fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT).ok()
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
