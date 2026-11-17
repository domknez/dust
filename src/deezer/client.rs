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
    /// Held while the CSRF token is fetched again, so calls that found it stale
    /// at once fetch one token, not one each (each new one rotates the cookies the
    /// others were given).
    pub refreshing: Mutex<()>,
    pub license_token: String,
    pub user_id: u64,
    pub loved_id: u64,
    pub name: String,
    /// The family members the account can listen as (empty unless it is a family
    /// plan's owner), and the one in use.
    pub profiles: Vec<Profile>,
    pub profile: Option<u64>,
    /// The id of the account the login belongs to (the family plan's owner).
    pub owner_id: u64,
    /// The pick (or the stored one) couldn't be confirmed: logged in as the owner.
    pub unconfirmed: bool,
}

/// A member of a Deezer family plan.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub id: u64,
    pub name: String,
}

/// Whether `user` (getUserData's `USER`) is the owner of a family plan: the only
/// account that can act as the others.
fn is_family_owner(user: &Value) -> bool {
    let multi = &user["MULTI_ACCOUNT"];
    multi["ENABLED"].as_bool().unwrap_or(false) && !multi["IS_SUB_ACCOUNT"].as_bool().unwrap_or(false)
}

/// The members of `getChildAccounts` that can be listened as. The owner is among
/// them; a member marked private can't be.
fn profiles_from(children: &Value) -> Vec<Profile> {
    let loggable = |c: &Value| c["EXTRA_FAMILY"]["IS_LOGGABLE_AS"].as_bool().unwrap_or(false);
    let members = children.as_array().into_iter().flatten().filter(|c| loggable(c));
    members
        .filter_map(|c| {
            let id = number(&c["USER_ID"]);
            (id != 0).then(|| Profile { id, name: text(&c["BLOG_NAME"]) })
        })
        .collect()
}

/// Logged-in Deezer account. Cheap to clone and share between threads.
#[derive(Clone)]
pub struct Deezer(pub(super) Arc<Inner>);

impl Deezer {
    /// Log in with an `arl` cookie value taken from a browser session, as the
    /// account itself. Use [`Deezer::profiles`] to see whether a family member can be
    /// picked, then [`Deezer::login_as`].
    pub fn login(arl: &str) -> Result<Deezer> {
        Self::login_as(arl, None)
    }

    /// Log in with an `arl` and act as the family member `profile`. One that the
    /// account doesn't have (any more) is ignored: the account itself.
    pub fn login_as(arl: &str, profile: Option<u64>) -> Result<Deezer> {
        let arl = arl.trim().to_string();
        let agent = session::http_agent();
        let session = Mutex::new(Session::new());
        let mut data = session::call(&agent, &arl, &session, "deezer.getUserData", json!({}), &[])?;
        let owner_id = number(&data["USER"]["USER_ID"]);
        if owner_id == 0 {
            return Err(Error::SessionExpired);
        }
        session::lock(&session).api_token = text(&data["checkForm"]);
        let profiles = if is_family_owner(&data["USER"]) {
            match session::call(&agent, &arl, &session, "deezer.getChildAccounts", json!({"nb": 30, "start": 0}), &[]) {
                Ok(children) => profiles_from(&children),
                Err(e) => {
                    log_warn!("family profiles: {e}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        let member = profile.and_then(|id| profiles.iter().find(|p| p.id == id).cloned());
        // Acting as another member is only taken for granted once Deezer says so: with
        // the member's cookie, getUserData must answer as that member. Otherwise likes,
        // playlist edits and listen reports would reach the wrong account, so it's the
        // owner (see `unconfirmed`).
        let mut confirmed = member.as_ref().map(|m| m.id).filter(|&id| id == owner_id);
        if let Some(m) = member.as_ref().filter(|m| m.id != owner_id) {
            {
                let mut s = session::lock(&session);
                s.set_profile(Some(m.id));
                s.reset_token();
            }
            match session::call(&agent, &arl, &session, "deezer.getUserData", json!({}), &[]) {
                Ok(d) if number(&d["USER"]["USER_ID"]) == m.id => {
                    session::lock(&session).api_token = text(&d["checkForm"]);
                    data = d;
                    confirmed = Some(m.id);
                }
                other => {
                    if let Err(e) = other {
                        log_warn!("family profile {}: {e}", m.id);
                    } else {
                        log_warn!("family profile {}: Deezer answered as another account", m.id);
                    }
                }
            }
        }
        if profile.is_some() && confirmed.is_none() {
            let owner = Self::login_as(&arl, None)?;
            let mut inner = Arc::into_inner(owner.0).expect("just made");
            inner.unconfirmed = true;
            return Ok(Deezer(Arc::new(inner)));
        }
        let user = &data["USER"];
        let license_token = text(&user["OPTIONS"]["license_token"]);
        if license_token.is_empty() {
            return Err(Error::NoStreamingLicense);
        }
        Ok(Deezer(Arc::new(Inner {
            agent,
            arl,
            session,
            refreshing: Mutex::new(()),
            license_token,
            user_id: number(&user["USER_ID"]),
            loved_id: number(&user["LOVEDTRACKS_ID"]),
            name: text(&user["BLOG_NAME"]),
            profiles,
            profile: confirmed,
            owner_id,
            unconfirmed: false,
        })))
    }

    /// An account that never logged in, for tests that don't call Deezer.
    #[cfg(test)]
    pub fn offline(name: &str) -> Deezer {
        Deezer(Arc::new(Inner {
            agent: session::http_agent(),
            arl: String::new(),
            session: Mutex::new(Session::new()),
            refreshing: Mutex::new(()),
            license_token: String::new(),
            user_id: 1,
            loved_id: 0,
            name: name.into(),
            profiles: Vec::new(),
            profile: None,
            owner_id: 1,
            unconfirmed: false,
        }))
    }

    /// A family plan's owner that never logged in, as `profile` (none: not picked yet).
    #[cfg(test)]
    pub fn offline_family(name: &str, members: &[(u64, &str)], profile: Option<u64>) -> Deezer {
        let profiles = members.iter().map(|(id, n)| Profile { id: *id, name: n.to_string() }).collect();
        let inner = Inner { profiles, profile, ..Arc::into_inner(Deezer::offline(name).0).unwrap() };
        Deezer(Arc::new(inner))
    }

    /// The family members that can be listened as: more than one means the listener
    /// should pick (none: not a family plan).
    pub fn profiles(&self) -> &[Profile] {
        &self.0.profiles
    }

    /// The family member in use, if one was picked.
    pub fn profile(&self) -> Option<u64> {
        self.0.profile
    }

    /// The family member asked for (the stored pick, or the picker's) couldn't be
    /// confirmed by Deezer: this is the account's owner instead.
    pub fn profile_unconfirmed(&self) -> bool {
        self.0.unconfirmed
    }

    #[cfg(test)]
    pub fn mark_unconfirmed(&mut self) {
        let inner = Arc::into_inner(std::mem::replace(&mut self.0, Deezer::offline("").0)).unwrap();
        self.0 = Arc::new(Inner { unconfirmed: true, ..inner });
    }

    /// What the local data (history, Crate, saved queue, release alerts) is kept under,
    /// so family members don't share it: the owner's keeps the source's plain key.
    pub fn data_key(&self) -> String {
        match self.0.profile.filter(|&p| p != self.0.owner_id) {
            Some(p) => format!("{}:{p}", super::ID.key),
            None => super::ID.key.to_string(),
        }
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
        let sent = session::lock(&inner.session).api_token.clone();
        match session::call(&inner.agent, &inner.arl, &inner.session, method, body.clone(), extra) {
            Err(Error::InvalidToken) => {
                self.refresh_csrf(&sent)?;
                session::call(&inner.agent, &inner.arl, &inner.session, method, body, extra)
            }
            result => result,
        }
    }

    /// Fetch a new CSRF token in place of `stale`, unless another call already has.
    fn refresh_csrf(&self, stale: &str) -> Result<()> {
        let inner = &self.0;
        let _one_at_a_time = inner.refreshing.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if session::lock(&inner.session).api_token != stale {
            return Ok(());
        }
        session::lock(&inner.session).reset_token();
        let data = session::call(&inner.agent, &inner.arl, &inner.session, "deezer.getUserData", json!({}), &[])?;
        // A rejected token with no logged-in user behind it: the session is gone.
        if number(&data["USER"]["USER_ID"]) == 0 {
            return Err(Error::SessionExpired);
        }
        session::lock(&inner.session).api_token = text(&data["checkForm"]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_owner_of_a_family_plan_can_pick() {
        let owner = json!({"MULTI_ACCOUNT": {"ENABLED": true, "IS_SUB_ACCOUNT": false}});
        let member = json!({"MULTI_ACCOUNT": {"ENABLED": true, "IS_SUB_ACCOUNT": true}});
        let single = json!({"MULTI_ACCOUNT": {"ENABLED": false, "IS_SUB_ACCOUNT": false}});
        assert!(is_family_owner(&owner));
        assert!(!is_family_owner(&member));
        assert!(!is_family_owner(&single));
        assert!(!is_family_owner(&json!({})), "no multi-account info: not a family");
    }

    #[test]
    fn each_member_keeps_local_data_of_their_own() {
        let key = |profile| Deezer::offline_family("P", &[(1, "Parent"), (2, "Kid")], profile).data_key();
        assert_eq!(key(None), "deezer", "as before profiles");
        assert_eq!(key(Some(1)), "deezer", "the owner keeps what was saved before");
        assert_eq!(key(Some(2)), "deezer:2");
    }

    #[test]
    fn private_members_cant_be_picked() {
        let children = json!([
            {"USER_ID": "11", "BLOG_NAME": "Parent", "LOVEDTRACKS_ID": "100", "EXTRA_FAMILY": {"IS_LOGGABLE_AS": true}},
            {"USER_ID": 22, "BLOG_NAME": "Kid", "EXTRA_FAMILY": {"IS_LOGGABLE_AS": true}},
            {"USER_ID": "33", "BLOG_NAME": "Teen", "EXTRA_FAMILY": {"IS_LOGGABLE_AS": false}},
            {"USER_ID": "0", "BLOG_NAME": "Broken", "EXTRA_FAMILY": {"IS_LOGGABLE_AS": true}},
        ]);
        let profiles = profiles_from(&children);
        assert_eq!(profiles, vec![Profile { id: 11, name: "Parent".into() }, Profile { id: 22, name: "Kid".into() }]);
        assert!(profiles_from(&json!(null)).is_empty());
    }
}
