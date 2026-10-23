//! Artwork: pictures named the way their source names them, turned into URLs by
//! that source.

use std::fmt;

/// A picture (album cover, artist photo, playlist or podcast artwork), as its
/// source names it. The name means something only to that source: Deezer's is the
/// picture's path on its image CDN, a music server's its cover art id. The source
/// turns it into a URL ([`ArtResolver`]).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Art(String);

impl Art {
    /// The picture `name`d by its source; None for an empty name.
    pub fn new(name: impl Into<String>) -> Option<Art> {
        let name = name.into();
        (!name.is_empty()).then_some(Art(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Art {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Art({:?})", self.0)
    }
}

/// Turns the source's pictures into URLs of square images.
pub trait ArtResolver: Send + Sync {
    /// The URL of `art`, about `size` px square.
    fn art_url(&self, art: &Art, size: u32) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_names_are_no_picture() {
        assert_eq!(Art::new(""), None);
        assert_eq!(Art::new("cover/abc").map(|a| a.as_str().to_string()).as_deref(), Some("cover/abc"));
    }
}
