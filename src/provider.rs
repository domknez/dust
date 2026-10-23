//! The music source the app is using: Deezer, or a Navidrome / Subsonic server.
//! [`Provider`] gives the UI and the player one interface to both; what only Deezer
//! has (Flow, podcasts, track mixes, Deezer links) is flagged in [`Features`] so the
//! UI can leave it out.

use crate::deezer::{self, Deezer, Link};
use crate::media::{OpenError, Playback};
use crate::model::{
    AlbumInfo, Art, ArtResolver, ArtistPage, Error, Item, Likeable, Likes, Listen, Lyrics, Playlist, PlaylistId, Quality, Result,
    SearchResults, Section, Track, TrackId,
};
use crate::player::library::Library;
use crate::subsonic::Subsonic;

/// A logged-in music source. Cheap to clone and share between threads.
#[derive(Clone)]
pub enum Provider {
    Deezer(Deezer),
    Subsonic(Subsonic),
}

/// What a source offers beyond browsing and playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    /// Endless Flow and its moods.
    pub flow: bool,
    pub podcasts: bool,
    /// "Mixes inspired by" a track.
    pub track_mix: bool,
    /// Pasting a Deezer link into search opens it.
    pub links: bool,
    /// The streaming quality setting applies.
    pub quality: bool,
    /// Playlists can be private (Deezer) rather than just the owner's.
    pub private_playlists: bool,
}

impl Provider {
    /// Deezer has everything.
    pub const DEEZER_FEATURES: Features =
        Features { flow: true, podcasts: true, track_mix: true, links: true, quality: true, private_playlists: true };

    pub fn features(&self) -> Features {
        match self {
            Provider::Deezer(_) => Self::DEEZER_FEATURES,
            Provider::Subsonic(_) => {
                Features { flow: false, podcasts: false, track_mix: false, links: false, quality: false, private_playlists: false }
            }
        }
    }

    /// Settings value and saved-queue key of the source: empty for Deezer.
    pub fn key(&self) -> &'static str {
        match self {
            Provider::Deezer(_) => "",
            Provider::Subsonic(_) => "navidrome",
        }
    }

    /// Account name shown in the sidebar.
    pub fn name(&self) -> String {
        match self {
            Provider::Deezer(d) => d.name().to_string(),
            Provider::Subsonic(s) => s.name().to_string(),
        }
    }

    /// Where the music comes from, as the account menu says it ("Deezer account",
    /// "Navidrome · music.example.com").
    pub fn label(&self) -> String {
        match self {
            Provider::Deezer(_) => "Deezer account".into(),
            Provider::Subsonic(s) => format!("Navidrome · {}", s.host()),
        }
    }

    /// Short name of the service ("Deezer", "Navidrome").
    pub fn service(&self) -> &'static str {
        match self {
            Provider::Deezer(_) => "Deezer",
            Provider::Subsonic(_) => "Navidrome",
        }
    }

    pub fn home(&self) -> Result<Vec<Section>> {
        match self {
            Provider::Deezer(d) => d.home(),
            Provider::Subsonic(s) => s.home(),
        }
    }

    pub fn search(&self, query: &str) -> Result<SearchResults> {
        match self {
            Provider::Deezer(d) => d.search(query),
            Provider::Subsonic(s) => s.search(query),
        }
    }

    pub fn resolve_link(&self, pasted: &str) -> Result<Link> {
        match self {
            Provider::Deezer(d) => d.resolve_link(pasted),
            Provider::Subsonic(_) => Err(unsupported("Deezer links")),
        }
    }

    pub fn loved(&self) -> Result<Vec<Track>> {
        match self {
            Provider::Deezer(d) => d.loved(),
            Provider::Subsonic(s) => s.loved(),
        }
    }

    pub fn playlist(&self, id: &PlaylistId) -> Result<Vec<Track>> {
        match self {
            Provider::Deezer(d) => d.playlist(id),
            Provider::Subsonic(s) => s.playlist(id),
        }
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        match self {
            Provider::Deezer(d) => d.playlists(),
            Provider::Subsonic(s) => s.playlists(),
        }
    }

    pub fn album(&self, id: &str) -> Result<Vec<Track>> {
        match self {
            Provider::Deezer(d) => d.album(id),
            Provider::Subsonic(s) => s.album(id),
        }
    }

    pub fn album_page(&self, id: &str) -> Result<(Vec<Track>, AlbumInfo)> {
        match self {
            Provider::Deezer(d) => d.album_page(id),
            Provider::Subsonic(s) => s.album_page(id),
        }
    }

    pub fn artist(&self, id: &str) -> Result<ArtistPage> {
        match self {
            Provider::Deezer(d) => d.artist(id),
            Provider::Subsonic(s) => s.artist(id),
        }
    }

    pub fn artist_top(&self, id: &str) -> Result<Vec<Track>> {
        match self {
            Provider::Deezer(d) => d.artist_top(id),
            Provider::Subsonic(s) => s.artist_top(id),
        }
    }

    /// The Artists page: followed artists on Deezer, the whole library on a server.
    pub fn favorite_artists(&self) -> Result<Vec<Item>> {
        match self {
            Provider::Deezer(d) => d.favorite_artists(),
            Provider::Subsonic(s) => s.favorite_artists(),
        }
    }

    /// The Albums page: liked albums on Deezer, the whole library on a server.
    pub fn favorite_albums(&self) -> Result<Vec<Item>> {
        match self {
            Provider::Deezer(d) => d.favorite_albums(),
            Provider::Subsonic(s) => s.favorite_albums(),
        }
    }

    pub fn favorite_shows(&self) -> Result<Vec<Item>> {
        match self {
            Provider::Deezer(d) => d.favorite_shows(),
            Provider::Subsonic(_) => Ok(Vec::new()),
        }
    }

    pub fn show_episodes(&self, id: &str) -> Result<Vec<Track>> {
        match self {
            Provider::Deezer(d) => d.show_episodes(id),
            Provider::Subsonic(_) => Err(unsupported("Podcasts")),
        }
    }

    pub fn track_mix(&self, id: &str) -> Result<Vec<Track>> {
        match self {
            Provider::Deezer(d) => d.track_mix(id),
            Provider::Subsonic(_) => Err(unsupported("Track mixes")),
        }
    }

    pub fn mix(&self, id: &str) -> Result<Vec<Track>> {
        match self {
            Provider::Deezer(d) => d.mix(id),
            Provider::Subsonic(s) => s.mix(id),
        }
    }

    pub fn likes(&self) -> Result<Likes> {
        match self {
            Provider::Deezer(d) => d.likes(),
            Provider::Subsonic(s) => s.likes(),
        }
    }

    pub fn set_liked(&self, what: &Likeable, liked: bool) -> Result<()> {
        match self {
            Provider::Deezer(d) => d.set_liked(what, liked),
            Provider::Subsonic(s) => s.set_liked(what, liked),
        }
    }

    pub fn lyrics(&self, song_id: &TrackId) -> Result<Option<Lyrics>> {
        match self {
            Provider::Deezer(d) => d.lyrics(song_id),
            Provider::Subsonic(s) => s.lyrics(song_id),
        }
    }

    pub fn create_playlist(&self, title: &str, private: bool, tracks: &[TrackId]) -> Result<Option<PlaylistId>> {
        match self {
            Provider::Deezer(d) => d.create_playlist(title, private, tracks).map(Some),
            Provider::Subsonic(s) => s.create_playlist(title, tracks),
        }
    }

    pub fn add_to_playlist(&self, playlist: &PlaylistId, tracks: &[TrackId]) -> Result<usize> {
        match self {
            Provider::Deezer(d) => d.add_to_playlist(playlist, tracks),
            Provider::Subsonic(s) => s.add_to_playlist(playlist, tracks),
        }
    }

    pub fn remove_from_playlist(&self, playlist: &PlaylistId, tracks: &[TrackId]) -> Result<()> {
        match self {
            Provider::Deezer(d) => d.remove_from_playlist(playlist, tracks),
            Provider::Subsonic(s) => s.remove_from_playlist(playlist, tracks),
        }
    }

    pub fn rename_playlist(&self, playlist: &Playlist, title: &str) -> Result<()> {
        match self {
            Provider::Deezer(d) => d.rename_playlist(playlist, title),
            Provider::Subsonic(s) => s.rename_playlist(playlist, title),
        }
    }

    pub fn delete_playlist(&self, playlist: &PlaylistId) -> Result<()> {
        match self {
            Provider::Deezer(d) => d.delete_playlist(playlist),
            Provider::Subsonic(s) => s.delete_playlist(playlist),
        }
    }
}

/// Pictures come from the source that named them.
impl ArtResolver for Provider {
    fn art_url(&self, art: &Art, size: u32) -> String {
        match self {
            Provider::Deezer(_) => deezer::art_url(art, size),
            Provider::Subsonic(s) => s.art_url(art, size),
        }
    }
}

fn unsupported(what: &str) -> Error {
    Error::Unsupported(what.into())
}

impl Library for Provider {
    fn open(&self, track: &Track, quality: Quality, at: f64) -> std::result::Result<Box<dyn Playback>, OpenError> {
        match self {
            Provider::Deezer(d) => d.open_track(track, quality, at),
            Provider::Subsonic(s) => s.open_track(track, at),
        }
        .map(|stream| Box::new(stream) as Box<dyn Playback>)
    }

    fn flow(&self, mood: Option<&str>) -> std::result::Result<Vec<Track>, String> {
        match self {
            Provider::Deezer(d) => d.flow(mood).map_err(|e| e.to_string()),
            Provider::Subsonic(s) => s.radio(mood).map_err(|e| e.to_string()),
        }
    }

    fn report_listen_start(&self, song_id: TrackId) {
        match self {
            Provider::Deezer(d) => {
                let client = d.clone();
                std::thread::spawn(move || {
                    if let Err(e) = client.report_listen_start(&song_id) {
                        log_debug!("listen start report failed: {e}");
                    }
                });
            }
            Provider::Subsonic(s) => {
                let client = s.clone();
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                std::thread::spawn(move || {
                    if let Err(e) = client.scrobble(&song_id, false, now) {
                        log_debug!("now playing report failed: {e}");
                    }
                });
            }
        }
    }

    fn report_listen(&self, listen: Listen) {
        match self {
            Provider::Deezer(d) => {
                let client = d.clone();
                std::thread::spawn(move || match client.report_listen(&listen) {
                    Ok(()) => log_debug!("reported listen of {} ({} s, skipped: {})", listen.song_id, listen.listened_secs, listen.skipped),
                    Err(e) => log_warn!("listen report failed: {e}"),
                });
            }
            Provider::Subsonic(s) => {
                // Counts as a play once heard through, or for four minutes (as
                // Last.fm scrobbling does).
                if listen.skipped && listen.listened_secs < 240 {
                    return;
                }
                let client = s.clone();
                std::thread::spawn(move || match client.scrobble(&listen.song_id, true, listen.started_unix) {
                    Ok(()) => log_debug!("scrobbled {} ({} s)", listen.song_id, listen.listened_secs),
                    Err(e) => log_warn!("scrobble failed: {e}"),
                });
            }
        }
    }
}
