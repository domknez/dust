//! Errors from the music sources, worded for showing to the listener. Network and
//! API errors name the [`Service`] they came from, so a dropped connection to a
//! Navidrome server doesn't read as a Deezer problem.

use std::fmt;

/// A service dust talks to, as its errors name it. Each source has its own
/// (`deezer::SERVICE`, `subsonic::SERVICE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Service {
    /// Short name, as the start of an error ("Navidrome: ...").
    pub name: &'static str,
    /// What couldn't be reached ("Couldn't reach your Navidrome server").
    place: &'static str,
}

impl Service {
    pub const fn new(name: &'static str, place: &'static str) -> Service {
        Service { name, place }
    }

    /// The service answered with an error.
    pub fn api(self, message: impl Into<String>) -> Error {
        Error::Api { service: self, message: message.into() }
    }

    /// The service couldn't be reached.
    pub fn network(self, message: impl Into<String>) -> Error {
        Error::Network { service: self, message: message.into() }
    }

    /// A failed HTTP request: an error status is the service answering, anything
    /// else (DNS, refused, timeout, a cut-off reply) is the network.
    pub fn http(self, e: ureq::Error) -> Error {
        match e {
            ureq::Error::StatusCode(status) => Error::Http { service: self, status },
            e => self.network(e.to_string()),
        }
    }
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

/// What went wrong talking to a music source.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The stored Deezer session (`arl`) is no longer valid.
    #[error("Your Deezer session has expired. Please log in again.")]
    SessionExpired,
    /// The Deezer account exists but can't stream full tracks.
    #[error("This Deezer account can't stream music (Deezer Premium required).")]
    NoStreamingLicense,
    /// The track isn't licensed for this account or region (and has no fallback).
    #[error("This track isn't available in your region.")]
    NotAvailable,
    /// Couldn't reach the service.
    #[error("Couldn't reach {}: {message}", service.place)]
    Network { service: Service, message: String },
    /// The service answered with an error.
    #[error("{service}: {message}")]
    Api { service: Service, message: String },
    /// The service answered with an HTTP error status (kept as a number so callers
    /// can tell "not found" from "try again later").
    #[error("{service}: HTTP {status}")]
    Http { service: Service, status: u16 },
    /// Deezer rejected the CSRF token (handled internally by refreshing it).
    #[error("Deezer rejected the request token.")]
    InvalidToken,
    /// Something this source doesn't have ("Podcasts").
    #[error("{0} aren't available from this music source")]
    Unsupported(String),
    /// Something outside the API itself (e.g. the login window was closed).
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn is_session_expired(&self) -> bool {
        matches!(self, Error::SessionExpired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEEZER: Service = Service::new("Deezer", "Deezer");
    const SERVER: Service = Service::new("Navidrome", "your Navidrome server");

    #[test]
    fn network_errors_name_their_service() {
        assert_eq!(DEEZER.network("timed out").to_string(), "Couldn't reach Deezer: timed out");
        assert_eq!(SERVER.network("timed out").to_string(), "Couldn't reach your Navidrome server: timed out");
    }

    #[test]
    fn api_errors_name_their_service() {
        assert_eq!(DEEZER.api("DATA_ERROR").to_string(), "Deezer: DATA_ERROR");
        assert_eq!(SERVER.api("Wrong username or password").to_string(), "Navidrome: Wrong username or password");
    }

    #[test]
    fn http_status_is_the_service_answering() {
        let e = SERVER.http(ureq::Error::StatusCode(404));
        assert!(matches!(e, Error::Http { service, status: 404 } if service == SERVER));
        assert_eq!(e.to_string(), "Navidrome: HTTP 404");
        let e = DEEZER.http(ureq::Error::ConnectionFailed);
        assert!(matches!(e, Error::Network { service, .. } if service == DEEZER));
    }

    #[test]
    fn unsupported_is_neutral() {
        assert_eq!(Error::Unsupported("Podcasts".into()).to_string(), "Podcasts aren't available from this music source");
    }
}
