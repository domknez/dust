//! The [`Library`]: what the source in use holds for the listener (playlists, likes,
//! the home page, followed artists, liked albums and podcasts) and the loads that
//! fill it.

use super::likes::LikeState;
use super::load::Load;
use super::playlist_edit::PlaylistEditing;
use crate::model::{Error, Item, Likes, Playlist, Section};

/// One source's data. Switching source or logging out replaces it wholesale
/// (`Library::default()`), which also drops its loads in flight: a result that
/// arrives after that has no receiver and is never applied.
#[derive(Default)]
pub struct Library {
    pub playlists: Vec<Playlist>,
    /// (Re)loading the playlists; the list on screen stays until the new one is in.
    pub playlists_load: Load<Vec<Playlist>>,
    pub likes: LikeState,
    pub likes_load: Load<Likes>,
    /// Cached; the refresh button resets it to force a reload.
    pub home: Load<Vec<Section>>,
    /// Followed artists, as cards (for `View::Artists`).
    pub followed: Load<Vec<Item>>,
    /// Liked albums, as cards (for `View::Albums`).
    pub favorite_albums: Load<Vec<Item>>,
    /// Followed podcasts, as cards (for `View::Podcasts`).
    pub favorite_shows: Load<Vec<Item>>,
    /// Playlist changes in flight and their dialog.
    pub editing: PlaylistEditing,
}

impl Library {
    /// Collect finished loads. Returns their errors (each also kept with its load)
    /// for the caller to act on; likes that didn't load are only logged.
    pub fn poll(&mut self) -> Vec<Error> {
        let mut failed = Vec::new();
        match self.playlists_load.poll_take() {
            Some(Ok(playlists)) => self.playlists = playlists,
            Some(Err(e)) => failed.push(e),
            None => {}
        }
        match self.likes_load.poll_take() {
            Some(Ok(likes)) => self.likes.likes = likes,
            Some(Err(e)) => log_warn!("loading likes: {e}"),
            None => {}
        }
        failed.extend(
            [self.home.poll(), self.followed.poll(), self.favorite_albums.poll(), self.favorite_shows.poll()].into_iter().flatten(),
        );
        failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn playlist(id: u64) -> Playlist {
        Playlist { id, title: format!("P{id}"), count: 1, owned: true, private: false, picture: None }
    }

    #[test]
    fn results_land_in_the_library() {
        let mut library = Library::default();
        let (playlists, home) = (channel(), channel());
        library.playlists_load = Load::Loading(playlists.1);
        library.home = Load::Loading(home.1);
        playlists.0.send(Ok(vec![playlist(1)])).unwrap();
        home.0.send(Err(Error::Other("home is down".into()))).unwrap();
        let failed = library.poll();
        assert_eq!(library.playlists.iter().map(|p| p.id).collect::<Vec<_>>(), [1]);
        assert_eq!(failed.len(), 1);
        assert_eq!(library.home.error(), Some("home is down"));
        assert!(library.playlists_load.error().is_none(), "the home error stays with home");
    }

    #[test]
    fn a_replaced_library_never_gets_old_results() {
        let mut library = Library::default();
        let (playlists, followed) = (channel(), channel());
        library.playlists = vec![playlist(1)];
        library.playlists_load = Load::Loading(playlists.1);
        library.followed = Load::Loading(followed.1);
        assert!(library.playlists_load.is_loading() && library.followed.is_loading());
        // Logging out, or switching to the other source.
        library = Library::default();
        assert!(playlists.0.send(Ok(vec![playlist(2)])).is_err(), "nobody is listening any more");
        assert!(followed.0.send(Ok(Vec::new())).is_err());
        assert!(library.poll().is_empty());
        assert!(library.playlists.is_empty());
        assert!(!library.playlists_load.is_loading() && library.followed.ready().is_none());
    }
}
