//! A Navidrome / Subsonic server as a music source: the library, likes (stars),
//! playlists, lyrics and a Radio of genres; no podcasts, track mixes, links or
//! quality setting (files play as they are).

use super::{SERVICE, Subsonic};
use crate::media::{OpenError, Playback};
use crate::model::{
    AlbumInfo, Art, ArtistPage, Item, Likeable, Likes, Listen, Lyrics, Playlist, PlaylistId, Quality, Result, SearchResults, Section,
    Track, TrackId,
};
use crate::source::{Features, Source, SourceId};

/// A server's names (whichever Subsonic-compatible server it is).
pub const ID: SourceId = SourceId { key: "navidrome", name: SERVICE.name, queue_file: "queue-navidrome.json", login_key: "server" };

/// A listen counts as a play once heard through, or for four minutes (as Last.fm
/// scrobbling does).
const PLAYED_SECS: u64 = 240;

impl Source for Subsonic {
    fn id(&self) -> SourceId {
        ID
    }

    fn features(&self) -> Features {
        Features::NONE
    }

    fn account(&self) -> String {
        self.name().to_string()
    }

    fn label(&self) -> String {
        format!("{} · {}", ID.name, self.host())
    }

    fn detail(&self) -> String {
        self.host().to_string()
    }

    fn tagline(&self) -> String {
        format!("from {}", self.host())
    }

    fn listen_reports(&self) -> (&'static str, &'static str) {
        ("Report plays to the server", "Play counts and \"Recently played\" (and Last.fm, if the server scrobbles)")
    }

    fn log_out_label(&self) -> String {
        format!("Disconnect from {}", ID.name)
    }

    fn home(&self) -> Result<Vec<Section>> {
        Subsonic::home(self)
    }

    fn search(&self, query: &str) -> Result<SearchResults> {
        Subsonic::search(self, query)
    }

    fn album(&self, id: &str) -> Result<Vec<Track>> {
        Subsonic::album(self, id)
    }

    fn album_page(&self, id: &str) -> Result<(Vec<Track>, AlbumInfo)> {
        Subsonic::album_page(self, id)
    }

    fn artist(&self, id: &str) -> Result<ArtistPage> {
        Subsonic::artist(self, id)
    }

    fn artist_top(&self, id: &str) -> Result<Vec<Track>> {
        Subsonic::artist_top(self, id)
    }

    fn mix(&self, id: &str) -> Result<Vec<Track>> {
        Subsonic::mix(self, id)
    }

    fn loved(&self) -> Result<Vec<Track>> {
        Subsonic::loved(self)
    }

    fn playlists(&self) -> Result<Vec<Playlist>> {
        Subsonic::playlists(self)
    }

    fn playlist(&self, id: &PlaylistId) -> Result<Vec<Track>> {
        Subsonic::playlist(self, id)
    }

    fn favorite_artists(&self) -> Result<Vec<Item>> {
        Subsonic::favorite_artists(self)
    }

    fn favorite_albums(&self) -> Result<Vec<Item>> {
        Subsonic::favorite_albums(self)
    }

    fn likes(&self) -> Result<Likes> {
        Subsonic::likes(self)
    }

    fn set_liked(&self, what: &Likeable, liked: bool) -> Result<()> {
        Subsonic::set_liked(self, what, liked)
    }

    fn open(&self, track: &Track, _quality: Quality, at: f64) -> std::result::Result<Box<dyn Playback>, OpenError> {
        Ok(Box::new(self.open_track(track, at)?))
    }

    fn endless(&self, genre: Option<&str>) -> Result<Vec<Track>> {
        self.radio(genre)
    }

    fn report_listen_start(&self, song: TrackId) {
        let client = self.clone();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        std::thread::spawn(move || {
            if let Err(e) = client.scrobble(&song, false, now) {
                log_debug!("now playing report failed: {e}");
            }
        });
    }

    fn report_listen(&self, listen: Listen) {
        if listen.skipped && listen.listened_secs < PLAYED_SECS {
            return;
        }
        let client = self.clone();
        std::thread::spawn(move || match client.scrobble(&listen.song_id, true, listen.started_unix) {
            Ok(()) => log_debug!("scrobbled {} ({} s)", listen.song_id, listen.listened_secs),
            Err(e) => log_warn!("scrobble failed: {e}"),
        });
    }

    fn art_url(&self, art: &Art, size: u32) -> String {
        Subsonic::art_url(self, art, size)
    }

    fn lyrics(&self, track: &TrackId) -> Result<Option<Lyrics>> {
        Subsonic::lyrics(self, track)
    }

    fn create_playlist(&self, title: &str, _private: bool, tracks: &[TrackId]) -> Result<Option<PlaylistId>> {
        Subsonic::create_playlist(self, title, tracks)
    }

    fn add_to_playlist(&self, playlist: &PlaylistId, tracks: &[TrackId]) -> Result<usize> {
        Subsonic::add_to_playlist(self, playlist, tracks)
    }

    fn remove_from_playlist(&self, playlist: &PlaylistId, tracks: &[TrackId]) -> Result<()> {
        Subsonic::remove_from_playlist(self, playlist, tracks)
    }

    fn rename_playlist(&self, playlist: &Playlist, title: &str) -> Result<()> {
        Subsonic::rename_playlist(self, playlist, title)
    }

    fn delete_playlist(&self, playlist: &PlaylistId) -> Result<()> {
        Subsonic::delete_playlist(self, playlist)
    }
}
