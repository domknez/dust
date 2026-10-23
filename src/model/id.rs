//! Ids of tracks and playlists: text only their source gives meaning to. Deezer's
//! are its numbers written out, a music server's its own string ids, so each source
//! takes back exactly what it handed out and nothing has to remember a mapping.

use std::fmt;
use std::sync::Arc;

macro_rules! id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Arc<str>);

        impl $name {
            pub fn new(id: impl AsRef<str>) -> Self {
                $name(Arc::from(id.as_ref()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        /// A source that numbers its items (Deezer) writes the number out.
        impl From<u64> for $name {
            fn from(number: u64) -> Self {
                $name::new(number.to_string())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($name), &*self.0)
            }
        }
    };
}

id!(
    /// A track's (song's or podcast episode's) id at its source.
    TrackId
);

id!(
    /// A playlist's id at its source.
    PlaylistId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_out() {
        assert_eq!(TrackId::from(3135553), TrackId::new("3135553"));
        assert_eq!(PlaylistId::from(908622995).as_str(), "908622995");
        assert_eq!(TrackId::new("qPyR7XLJOiu93qWw1n06xy").to_string(), "qPyR7XLJOiu93qWw1n06xy");
    }
}
