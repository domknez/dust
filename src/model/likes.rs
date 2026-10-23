//! What the listener likes: tracks, albums and artists.

use std::collections::HashSet;

/// Something the listener can like: favourite tracks, albums and followed artists on
/// Deezer, starred ones on a music server.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Likeable {
    Track(u64),
    Album(String),
    Artist(String),
}

/// Everything the account likes, by id.
#[derive(Clone, Debug, Default)]
pub struct Likes {
    pub tracks: HashSet<u64>,
    pub albums: HashSet<String>,
    pub artists: HashSet<String>,
}

impl Likes {
    pub fn contains(&self, what: &Likeable) -> bool {
        match what {
            Likeable::Track(id) => self.tracks.contains(id),
            Likeable::Album(id) => self.albums.contains(id),
            Likeable::Artist(id) => self.artists.contains(id),
        }
    }

    /// Record a like or unlike locally.
    pub fn set(&mut self, what: &Likeable, liked: bool) {
        fn apply<T: Eq + std::hash::Hash + Clone>(set: &mut HashSet<T>, id: &T, liked: bool) {
            if liked {
                set.insert(id.clone());
            } else {
                set.remove(id);
            }
        }
        match what {
            Likeable::Track(id) => apply(&mut self.tracks, id, liked),
            Likeable::Album(id) => apply(&mut self.albums, id, liked),
            Likeable::Artist(id) => apply(&mut self.artists, id, liked),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn likes_track_toggles() {
        let mut likes = Likes::default();
        let (song, album, artist) = (Likeable::Track(7), Likeable::Album("302127".into()), Likeable::Artist("27".into()));
        assert!(!likes.contains(&song));
        likes.set(&song, true);
        likes.set(&album, true);
        likes.set(&artist, true);
        assert!(likes.contains(&song) && likes.contains(&album) && likes.contains(&artist));
        likes.set(&album, false);
        assert!(!likes.contains(&album) && likes.contains(&song));
    }
}
