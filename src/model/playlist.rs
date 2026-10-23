//! The account's playlists.

use super::{Art, PlaylistId};

#[derive(Clone, Debug, PartialEq)]
pub struct Playlist {
    pub id: PlaylistId,
    pub title: String,
    pub count: u32,
    /// Made by this account (editable), not one it follows.
    pub owned: bool,
    pub private: bool,
    pub picture: Option<Art>,
}
