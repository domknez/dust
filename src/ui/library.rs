//! The [`Library`]: what the source in use holds for the listener (playlists, likes,
//! the home page, followed artists, liked albums and podcasts) and the loads that
//! fill it.

use super::likes::LikeState;
use super::playlist_edit::PlaylistEditing;
use super::tasks::Task;
use crate::deezer::{Item, Likes, Playlist, Section};

/// One source's data. Switching source or logging out replaces it wholesale
/// (`Library::default()`), which also drops its loads in flight: a result that
/// arrives after that has no receiver and is never applied.
#[derive(Default)]
pub struct Library {
    pub playlists: Vec<Playlist>,
    pub playlists_task: Task<Vec<Playlist>>,
    pub likes: LikeState,
    pub likes_task: Task<Likes>,
    pub home: Vec<Section>,
    pub home_task: Task<Vec<Section>>,
    /// Followed artists, as cards (for `View::Artists`).
    pub followed: Vec<Item>,
    pub followed_task: Task<Vec<Item>>,
    /// Liked albums, as cards (for `View::Albums`).
    pub favorite_albums: Vec<Item>,
    pub favorite_albums_task: Task<Vec<Item>>,
    /// Followed podcasts, as cards (for `View::Podcasts`).
    pub favorite_shows: Vec<Item>,
    pub favorite_shows_task: Task<Vec<Item>>,
    /// Playlist changes in flight and their dialog.
    pub editing: PlaylistEditing,
}
