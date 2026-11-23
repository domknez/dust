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
    /// See [`Deezer::data_key`].
    pub data_key: String,
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

/// What a member's local data is kept under: the owner's (and a plain account's)
/// keeps the source's key, so nothing saved before family profiles moves.
fn data_key(profile: Option<u64>, owner_id: u64) -> String {
    match profile.filter(|&p| p != owner_id) {
        Some(p) => format!("{}:{p}", super::ID.key),
        None => super::ID.key.to_string(),
    }
}

/// Ask Deezer who the session is (getUserData) and take the CSRF token it gives.
/// `expected`: the user it must be, else [`Error::ProfileLost`] (and the token
/// isn't taken, so nothing more is done as someone else). Its data and user id.
fn who(agent: &ureq::Agent, arl: &str, session: &Mutex<Session>, expected: Option<u64>) -> Result<(Value, u64)> {
    let data = session::call(agent, arl, session, "deezer.getUserData", json!({}), &[])?;
    let user = number(&data["USER"]["USER_ID"]);
    // A rejected token with no logged-in user behind it: the session is gone.
    if user == 0 {
        return Err(Error::SessionExpired);
    }
    if expected.is_some_and(|id| id != user) {
        return Err(Error::ProfileLost);
    }
    session::lock(session).api_token = text(&data["checkForm"]);
    Ok((data, user))
}

/// Have `session` act as the family member `id`, and check that Deezer does:
/// getUserData must answer as them. Their getUserData on success.
fn act_as(agent: &ureq::Agent, arl: &str, session: &Mutex<Session>, id: u64) -> Result<Value> {
    {
        let mut s = session::lock(session);
        s.set_profile(Some(id));
        s.reset_token();
    }
    who(agent, arl, session, Some(id)).map(|(data, _)| data)
}

/// Logged-in Deezer account. Cheap to clone and share between threads.
#[derive(Clone)]
pub struct Deezer(pub(super) Arc<Inner>);

impl Deezer {
    /// Log in with an `arl` cookie value taken from a browser session, acting as the
    /// family member `profile` (none: the account itself). See [`Deezer::profiles`]
    /// for whether there are members to pick, and [`Deezer::profile_unconfirmed`] for
    /// a pick Deezer didn't take.
    pub fn login_as(arl: &str, profile: Option<u64>) -> Result<Deezer> {
        let arl = arl.trim().to_string();
        let agent = session::http_agent();
        let session = Mutex::new(Session::new());
        let (mut data, owner_id) = who(&agent, &arl, &session, None)?;
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
        // Acting as another member is only taken for granted once Deezer says so: with
        // the member's cookie, getUserData must answer as that member. Otherwise likes,
        // playlist edits and listen reports would reach the wrong account, so it's the
        // owner, marked `unconfirmed`. One the plan doesn't have (any more) is too.
        let picked = profile.filter(|id| profiles.iter().any(|p| p.id == *id));
        let mut confirmed = picked.filter(|&id| id == owner_id);
        if let Some(id) = picked.filter(|&id| id != owner_id) {
            match act_as(&agent, &arl, &session, id) {
                Ok(member) => {
                    data = member;
                    confirmed = Some(id);
                }
                Err(e) => {
                    log_warn!("family profile not taken: {e}");
                    // Whatever cookies the attempt left could still point at the member.
                    *session::lock(&session) = Session::new();
                    data = who(&agent, &arl, &session, Some(owner_id))?.0;
                }
            }
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
            unconfirmed: profile.is_some() && confirmed.is_none(),
            data_key: data_key(confirmed, owner_id),
            profiles,
            profile: confirmed,
        })))
    }

    /// An account that never logged in, for tests that don't call Deezer.
    #[cfg(test)]
    pub fn offline(name: &str) -> Deezer {
        Self::offline_family(name, &[], None)
    }

    /// A family plan's owner (id 1) that never logged in, as `profile` (none: not
    /// picked yet).
    #[cfg(test)]
    pub fn offline_family(name: &str, members: &[(u64, &str)], profile: Option<u64>) -> Deezer {
        const OWNER: u64 = 1;
        Deezer(Arc::new(Inner {
            agent: session::http_agent(),
            arl: String::new(),
            session: Mutex::new(Session::new()),
            refreshing: Mutex::new(()),
            license_token: String::new(),
            user_id: profile.unwrap_or(OWNER),
            loved_id: 0,
            name: name.into(),
            profiles: members.iter().map(|&(id, n)| Profile { id, name: n.into() }).collect(),
            profile,
            data_key: data_key(profile, OWNER),
            unconfirmed: false,
        }))
    }

    /// The owner, after Deezer didn't confirm the member asked for.
    #[cfg(test)]
    pub fn offline_unconfirmed(name: &str, members: &[(u64, &str)]) -> Deezer {
        let owner = Self::offline_family(name, members, None);
        Deezer(Arc::new(Inner { unconfirmed: true, ..Arc::into_inner(owner.0).expect("just made") }))
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

    /// The Deezer user acted as: the member picked, or the account itself.
    pub fn user_id(&self) -> u64 {
        self.0.user_id
    }

    /// The family member asked for (the stored pick, or the picker's) couldn't be
    /// confirmed by Deezer: this is the account's owner instead.
    pub fn profile_unconfirmed(&self) -> bool {
        self.0.unconfirmed
    }

    /// What the local data (history, Crate, saved queue, release alerts) is kept under,
    /// so family members don't share it: the owner's keeps the source's plain key.
    pub fn data_key(&self) -> &str {
        &self.0.data_key
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
        // Someone else answering (the plan's owner, when Deezer stopped taking the
        // member's cookie) would mean liking, editing and reporting as them.
        who(&inner.agent, &inner.arl, &inner.session, Some(inner.user_id)).map(drop)
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
        let key = |profile| Deezer::offline_family("P", &[(1, "Parent"), (2, "Kid")], profile).data_key().to_string();
        assert_eq!(key(None), "deezer", "as before profiles");
        assert_eq!(key(Some(1)), "deezer", "the owner keeps what was saved before");
        assert_eq!(key(Some(2)), "deezer:2");
        let file = |profile| crate::source::Source::queue_file(&Deezer::offline_family("P", &[(1, "Parent"), (2, "Kid")], profile));
        assert_eq!(file(Some(1)), "queue.json", "the owner's saved queue stays where it was");
        assert_eq!(file(Some(2)), "queue-deezer-2.json");
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

    // ---------------------------------------------------- against a fake Deezer

    use super::super::fake::{self, Fake};

    /// What the next authenticated call is answered as.
    fn acting_as(client: &Deezer) -> u64 {
        number(&client.call_raw("user.whoami", json!({})).expect("answered")["USER_ID"])
    }

    #[test]
    fn the_owner_logs_in_and_sees_the_members_that_can_be_picked() {
        let _deezer = Fake::start();
        let client = Deezer::login_as(fake::ARL, None).unwrap();
        let ids: Vec<u64> = client.profiles().iter().map(|p| p.id).collect();
        assert_eq!(ids, [1, 2], "the private member is left out");
        assert_eq!((client.profile(), client.user_id(), client.data_key()), (None, 1, "deezer"));
        assert!(!client.profile_unconfirmed());
        assert_eq!(acting_as(&client), 1);
    }

    #[test]
    fn a_member_is_acted_as_once_deezer_confirms_it() {
        let deezer = Fake::start();
        let client = Deezer::login_as(fake::ARL, Some(2)).unwrap();
        assert_eq!((client.profile(), client.user_id(), client.data_key(), client.name()), (Some(2), 2, "deezer:2", "Kid"));
        assert!(!client.profile_unconfirmed());
        // Deezer sets the owner's account_id again with every getUserData: the
        // member's stays what's sent.
        assert_eq!(acting_as(&client), 2);
        assert_eq!(acting_as(&client), 2);
        let sent = deezer.last_cookies();
        assert!(sent.contains("account_id=2") && !sent.contains("account_id=1"), "{sent}");
    }

    #[test]
    fn the_owner_picked_is_the_owner() {
        let _deezer = Fake::start();
        let client = Deezer::login_as(fake::ARL, Some(1)).unwrap();
        assert_eq!((client.profile(), client.data_key()), (Some(1), "deezer"), "keeps what was saved before profiles");
        assert!(!client.profile_unconfirmed());
        assert_eq!(acting_as(&client), 1);
    }

    #[test]
    fn a_member_deezer_doesnt_take_leaves_the_owner_and_no_trace_of_the_member() {
        let deezer = Fake::start();
        deezer.state().honour_members = false;
        let client = Deezer::login_as(fake::ARL, Some(2)).unwrap();
        assert!(client.profile_unconfirmed());
        assert_eq!((client.profile(), client.user_id(), client.data_key(), client.name()), (None, 1, "deezer", "Parent"));
        assert_eq!(acting_as(&client), 1);
        let sent = deezer.last_cookies();
        assert!(!sent.contains("account_id=2"), "the member's cookie is gone: {sent}");
    }

    #[test]
    fn a_member_not_on_the_plan_is_not_tried() {
        let deezer = Fake::start();
        for gone in [3, 99] {
            let client = Deezer::login_as(fake::ARL, Some(gone)).unwrap();
            assert!(client.profile_unconfirmed(), "{gone}");
            assert_eq!((client.profile(), client.data_key()), (None, "deezer"));
        }
        assert!(deezer.state().requests.iter().all(|r| !r.cookies.contains("account_id=3") && !r.cookies.contains("account_id=99")));
    }

    #[test]
    fn a_session_deezer_doesnt_know_is_expired() {
        let _deezer = Fake::start();
        assert!(matches!(Deezer::login_as("stale-arl", None), Err(Error::SessionExpired)));
        assert!(matches!(Deezer::login_as("stale-arl", Some(2)), Err(Error::SessionExpired)));
    }

    #[test]
    fn a_stale_token_is_renewed_as_the_same_member() {
        let deezer = Fake::start();
        let client = Deezer::login_as(fake::ARL, Some(2)).unwrap();
        deezer.state().token_round += 1;
        assert_eq!(acting_as(&client), 2, "renewed, still the kid");
        assert_eq!(deezer.served("user.whoami"), [2], "refused once, then served as the kid");
    }

    #[test]
    fn a_member_deezer_stops_taking_is_never_acted_as_the_owner() {
        let deezer = Fake::start();
        let client = Deezer::login_as(fake::ARL, Some(2)).unwrap();
        {
            let mut state = deezer.state();
            state.honour_members = false;
            state.token_round += 1;
        }
        assert!(matches!(client.call_raw("user.whoami", json!({})), Err(Error::ProfileLost)));
        // The owner's token wasn't taken: nothing goes through as them afterwards.
        assert!(client.call_raw("user.whoami", json!({})).is_err());
        assert!(deezer.served("user.whoami").is_empty(), "nothing done as the owner");
    }
}
