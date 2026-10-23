//! The account's playlists.

#[derive(Clone, Debug, PartialEq)]
pub struct Playlist {
    pub id: u64,
    pub title: String,
    pub count: u32,
    /// Made by this account (editable), not one it follows.
    pub owned: bool,
    pub private: bool,
    /// (`cover`|`playlist`|..., md5) on Deezer's image CDN.
    pub picture: Option<(String, String)>,
}
