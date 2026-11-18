//! The HTTP clients dust makes: one shape, each with its own name and deadline.

use std::time::Duration;

/// How long to wait for a server to accept a connection.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// "dust/0.31.1", for servers that like to know who's asking.
pub const USER_AGENT: &str = concat!("dust/", env!("CARGO_PKG_VERSION"));

/// An HTTP client sending `user_agent`, each request given at most `deadline`
/// (None: as long as it takes, for streams and downloads that set their own).
pub fn agent(user_agent: &str, deadline: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder().user_agent(user_agent).timeout_connect(Some(CONNECT_TIMEOUT)).timeout_global(deadline).build().into()
}
