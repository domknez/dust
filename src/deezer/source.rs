//! Deezer as a music source: everything, including Flow, Explore, podcasts, track mixes,
//! Deezer links and the quality setting.

use super::{Deezer, SERVICE, images, links};
use crate::media::{OpenError, Playback};
use crate::model::{
    AlbumInfo, Art, ArtistPage, Item, Likeable, Likes, Link, Listen, Lyrics, Page, Playlist, PlaylistId, Quality, Result, SearchResults,
    Section, Track, TrackId,
};
use crate::source::{Features, Source, SourceId};

/// Deezer's names. It came first: its queue file has no suffix, its login is the
/// `arl` cookie.
pub const ID: SourceId = SourceId { key: "deezer", name: SERVICE.name, queue_file: "queue.json", login_key: "arl" };

impl Source for Deezer {
    fn id(&self) -> SourceId {
        ID
    }

    fn features(&self) -> Features {
        Features::ALL
    }

    fn account(&self) -> String {
        self.name().to_string()
    }

    fn label(&self) -> String {
        "Deezer account".into()
    }

    fn detail(&self) -> String {
        self.account()
    }

    fn tagline(&self) -> String {
        "recommended by deezer".into()
    }

    fn listen_reports(&self) -> (&'static str, &'static str) {
        ("Share listening with Deezer", "History, Flow and Last.fm scrobbling via Deezer")
    }

    fn home(&self) -> Result<Vec<Section>> {
        Deezer::home(self)
    }

    fn search(&self, query: &str) -> Result<SearchResults> {
        Deezer::search(self, query)
    }

    fn album_page(&self, id: &str) -> Result<(Vec<Track>, AlbumInfo)> {
        Deezer::album_page(self, id)
    }

    fn artist(&self, id: &str) -> Result<ArtistPage> {
        Deezer::artist(self, id)
    }

    fn artist_top(&self, id: &str) -> Result<Vec<Track>> {
        Deezer::artist_top(self, id)
    }

    fn mix(&self, id: &str) -> Result<Vec<Track>> {
        Deezer::mix(self, id)
    }

    fn artist_mix(&self, id: &str) -> Result<Vec<Track>> {
        Deezer::artist_mix(self, id)
    }

    fn page(&self, id: &str) -> Result<Page> {
        Deezer::page(self, id)
    }

    fn loved(&self) -> Result<Vec<Track>> {
        Deezer::loved(self)
    }

    fn playlists(&self) -> Result<Vec<Playlist>> {
        Deezer::playlists(self)
    }

    fn playlist(&self, id: &PlaylistId) -> Result<Vec<Track>> {
        Deezer::playlist(self, id)
    }

    fn favorite_artists(&self) -> Result<Vec<Item>> {
        Deezer::favorite_artists(self)
    }

    fn favorite_albums(&self) -> Result<Vec<Item>> {
        Deezer::favorite_albums(self)
    }

    fn likes(&self) -> Result<Likes> {
        Deezer::likes(self)
    }

    fn set_liked(&self, what: &Likeable, liked: bool) -> Result<()> {
        Deezer::set_liked(self, what, liked)
    }

    fn open(&self, track: &Track, quality: Quality, at: f64) -> std::result::Result<Box<dyn Playback>, OpenError> {
        Ok(Box::new(self.open_track(track, quality, at)?))
    }

    fn endless(&self, tuning: Option<&str>) -> Result<Vec<Track>> {
        self.flow(tuning)
    }

    fn report_listen_start(&self, song: TrackId) {
        let client = self.clone();
        std::thread::spawn(move || {
            if let Err(e) = Deezer::report_listen_start(&client, &song) {
                log_debug!("listen start report failed: {e}");
            }
        });
    }

    fn report_listen(&self, listen: Listen) {
        let client = self.clone();
        std::thread::spawn(move || match Deezer::report_listen(&client, &listen) {
            Ok(()) => log_debug!("reported listen of {} ({} s, skipped: {})", listen.song_id, listen.listened_secs, listen.skipped),
            Err(e) => log_warn!("listen report failed: {e}"),
        });
    }

    fn art_url(&self, art: &Art, size: u32) -> String {
        images::art_url(art, size)
    }

    fn favorite_shows(&self) -> Result<Vec<Item>> {
        Deezer::favorite_shows(self)
    }

    fn show_episodes(&self, id: &str) -> Result<Vec<Track>> {
        Deezer::show_episodes(self, id)
    }

    fn track_mix(&self, id: &str) -> Result<Vec<Track>> {
        Deezer::track_mix(self, id)
    }

    fn is_link(&self, text: &str) -> bool {
        links::is_link(text)
    }

    fn resolve_link(&self, pasted: &str) -> Result<Link> {
        Deezer::resolve_link(self, pasted)
    }

    fn lyrics(&self, track: &TrackId) -> Result<Option<Lyrics>> {
        Deezer::lyrics(self, track)
    }

    fn create_playlist(&self, title: &str, private: bool, tracks: &[TrackId]) -> Result<Option<PlaylistId>> {
        Deezer::create_playlist(self, title, private, tracks).map(Some)
    }

    fn add_to_playlist(&self, playlist: &PlaylistId, tracks: &[TrackId]) -> Result<usize> {
        Deezer::add_to_playlist(self, playlist, tracks)
    }

    fn remove_from_playlist(&self, playlist: &PlaylistId, tracks: &[TrackId]) -> Result<()> {
        Deezer::remove_from_playlist(self, playlist, tracks)
    }

    fn rename_playlist(&self, playlist: &Playlist, title: &str) -> Result<()> {
        Deezer::rename_playlist(self, playlist, title)
    }

    fn delete_playlist(&self, playlist: &PlaylistId) -> Result<()> {
        Deezer::delete_playlist(self, playlist)
    }
}
