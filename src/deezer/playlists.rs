//! Changing the account's own playlists: create (optionally from tracks), add and
//! remove tracks, rename, delete.

use super::{Deezer, Result, api, parse};
use crate::model::Playlist;
use serde_json::{Value, json};

/// Deezer's playlist visibility (`STATUS`).
const PUBLIC: u64 = 0;
const PRIVATE: u64 = 1;

fn songs(ids: &[u64]) -> Value {
    Value::Array(ids.iter().map(|id| json!([id.to_string(), 0])).collect())
}

impl Deezer {
    /// A new playlist holding `tracks` (in order); returns its id.
    pub fn create_playlist(&self, title: &str, private: bool, tracks: &[u64]) -> Result<u64> {
        let status = if private { PRIVATE } else { PUBLIC };
        let r = self.call("playlist.create", json!({"title": title, "description": "", "status": status, "songs": songs(tracks)}))?;
        Some(parse::number(&r)).filter(|&id| id != 0).ok_or_else(|| api("playlist.create: no id in reply"))
    }

    /// Append the tracks that aren't in the playlist yet (Deezer rejects a batch
    /// with any duplicate). Returns how many were added.
    pub fn add_to_playlist(&self, playlist: u64, tracks: &[u64]) -> Result<usize> {
        let present: std::collections::HashSet<u64> = self.playlist(playlist)?.iter().map(|t| t.id).collect();
        let mut seen = std::collections::HashSet::new();
        let new: Vec<u64> = tracks.iter().copied().filter(|id| !present.contains(id) && seen.insert(*id)).collect();
        if !new.is_empty() {
            self.call("playlist.addSongs", json!({"playlist_id": playlist, "songs": songs(&new), "offset": -1}))?;
        }
        Ok(new.len())
    }

    pub fn remove_from_playlist(&self, playlist: u64, tracks: &[u64]) -> Result<()> {
        self.call("playlist.deleteSongs", json!({"playlist_id": playlist, "songs": songs(tracks)})).map(drop)
    }

    /// Rename, keeping its visibility.
    pub fn rename_playlist(&self, playlist: &Playlist, title: &str) -> Result<()> {
        let status = if playlist.private { PRIVATE } else { PUBLIC };
        self.call("playlist.update", json!({"playlist_id": playlist.id, "title": title, "description": "", "status": status})).map(drop)
    }

    pub fn delete_playlist(&self, playlist: u64) -> Result<()> {
        self.call("playlist.delete", json!({"playlist_id": playlist})).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Network test on the stored account (`cargo test -- --ignored playlist_round_trip`):
    /// creates a private playlist, edits it, deletes it.
    #[test]
    #[ignore]
    fn playlist_round_trip() {
        let client = Deezer::login(&crate::credentials::load().expect("stored session")).expect("login");
        let id = client.create_playlist("dust test (delete me)", true, &[3135556]).unwrap();
        let ids = |c: &Deezer| c.playlist(id).unwrap().iter().map(|t| t.id).collect::<Vec<_>>();
        let result = || {
            assert_eq!(ids(&client), [3135556]);
            // The duplicate is skipped instead of failing the batch.
            assert_eq!(client.add_to_playlist(id, &[3135556, 3135561, 3135561]).unwrap(), 1);
            assert_eq!(ids(&client), [3135556, 3135561]);
            client.remove_from_playlist(id, &[3135556]).unwrap();
            assert_eq!(ids(&client), [3135561]);
            let playlist = client.playlists().unwrap().into_iter().find(|p| p.id == id).expect("listed");
            assert!(playlist.owned && playlist.private);
            client.rename_playlist(&playlist, "dust test renamed").unwrap();
            assert!(client.playlists().unwrap().iter().any(|p| p.id == id && p.title == "dust test renamed"));
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(result));
        client.delete_playlist(id).unwrap();
        assert!(!client.playlists().unwrap().iter().any(|p| p.id == id), "deleted");
        outcome.unwrap();
    }

    #[test]
    fn songs_payload() {
        assert_eq!(songs(&[3135556, 7]), json!([["3135556", 0], ["7", 0]]));
    }
}
