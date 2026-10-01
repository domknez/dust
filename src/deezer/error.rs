//! Errors from the Deezer client, worded for showing to the listener.

/// What went wrong talking to Deezer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The stored session (`arl`) is no longer valid.
    #[error("Your Deezer session has expired. Please log in again.")]
    SessionExpired,
    /// The account exists but can't stream full tracks.
    #[error("This Deezer account can't stream music (Deezer Premium required).")]
    NoStreamingLicense,
    /// The track isn't licensed for this account or region (and has no fallback).
    #[error("This track isn't available in your region.")]
    NotAvailable,
    /// Couldn't reach Deezer.
    #[error("Couldn't reach Deezer: {0}")]
    Network(String),
    /// The CSRF token was rejected (handled internally by refreshing it).
    #[error("Deezer rejected the request token.")]
    InvalidToken,
    /// Deezer answered with an error.
    #[error("Deezer: {0}")]
    Api(String),
    /// Something outside the API itself (e.g. the login window was closed).
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn is_session_expired(&self) -> bool {
        matches!(self, Error::SessionExpired)
    }
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::StatusCode(code) => Error::Api(format!("HTTP {code}")),
            e => Error::Network(e.to_string()),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Network(e.to_string())
    }
}
