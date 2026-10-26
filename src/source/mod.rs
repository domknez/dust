//! Music sources: Deezer and Navidrome / Subsonic servers behind one trait, so the
//! UI and the player work the same with any of them.
//!
//! - [`Source`]: what a logged-in source does (browse, play, likes, playlists, ...)
//! - [`SourceId`]: which kind of source, as the settings file, saved queues, the
//!   keychain and the account menu name it
//! - [`KINDS`]: the sources dust knows, in the account menu's order
//!
//! A source is used as a [`Shared`] (`Arc<dyn Source>`). What only some sources
//! have (Flow, Explore, podcasts, track mixes, links, lyrics, playlist editing) has default
//! methods that say it's unsupported, and is flagged in [`Features`] so the UI can
//! leave it out.

use crate::media::{OpenError, Playback};
use crate::model::{
    AlbumInfo, Art, ArtResolver, ArtistPage, Error, Item, Likeable, Likes, Link, Listen, Lyrics, Page, Playlist, PlaylistId, Quality,
    Result, SearchResults, Section, Track, TrackId,
};
use crate::player::library::Library;
use std::sync::Arc;

/// A kind of music source, whether or not it's logged in: its names in files and
/// on screen. Each source declares its own (`deezer::ID`, `subsonic::ID`); the
/// spellings are part of the files dust keeps, so they never change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceId {
    /// The settings file's `source=` value.
    pub key: &'static str,
    /// The service's name ("Deezer", "Navidrome"): the account menu, logs.
    pub name: &'static str,
    /// Its saved queue, next to the settings file.
    pub queue_file: &'static str,
    /// Its field in the keychain item holding every login.
    pub login_key: &'static str,
}

/// Every source dust knows, in the account menu's order; the first is the default.
/// A new source adds its [`SourceId`] here (and its row to `ui::sources::MENU`).
pub const KINDS: &[SourceId] = &[crate::deezer::ID, crate::subsonic::ID];

impl SourceId {
    /// The source a settings value names.
    pub fn from_key(key: &str) -> Option<SourceId> {
        KINDS.iter().copied().find(|kind| kind.key == key)
    }
}

/// What a source offers beyond browsing and playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    /// Endless play is Deezer's Flow (with moods) rather than a Radio of genres.
    pub flow: bool,
    /// Explore and New releases: the service's own pages of genres, moods, charts
    /// and new music.
    pub explore: bool,
    pub podcasts: bool,
    /// "Mixes inspired by" a track.
    pub track_mix: bool,
    /// Pasting a link to the service into search opens it.
    pub links: bool,
    /// The streaming quality setting applies.
    pub quality: bool,
    /// Playlists can be private rather than just the owner's.
    pub private_playlists: bool,
}

impl Features {
    /// Everything (what the UI assumes while no source is in use).
    pub const ALL: Features =
        Features { flow: true, explore: true, podcasts: true, track_mix: true, links: true, quality: true, private_playlists: true };
    /// Browsing and playing only.
    pub const NONE: Features =
        Features { flow: false, explore: false, podcasts: false, track_mix: false, links: false, quality: false, private_playlists: false };
}

/// A logged-in source, shared between the UI, background loads and the player.
pub type Shared = Arc<dyn Source>;

/// A logged-in music source. Calls block (on the network): the UI makes them from
/// background jobs, the player from its own thread.
pub trait Source: Send + Sync {
    // ------------------------------------------------------------ identity

    fn id(&self) -> SourceId;
    fn features(&self) -> Features;
    /// The account's name (sidebar, account menu, the home page's greeting).
    fn account(&self) -> String;
    /// Where the music comes from, under the account's name in the account menu
    /// ("Deezer account", "Navidrome · music.example.com").
    fn label(&self) -> String;
    /// The note on the source's row in the account menu, and under the account in
    /// the sidebar when quality doesn't apply (the account, a server's address).
    fn detail(&self) -> String;
    /// The line above the home page's greeting ("recommended by deezer").
    fn tagline(&self) -> String;
    /// The Settings switch for listen reports: its title and what it does.
    fn listen_reports(&self) -> (&'static str, &'static str);
    /// The account menu's way out ("Log out of Deezer").
    fn log_out_label(&self) -> String {
        format!("Log out of {}", self.id().name)
    }

    // ------------------------------------------------------------ browsing

    fn home(&self) -> Result<Vec<Section>>;
    fn search(&self, query: &str) -> Result<SearchResults>;
    /// An album's tracks.
    fn album(&self, id: &str) -> Result<Vec<Track>> {
        self.album_page(id).map(|(tracks, _)| tracks)
    }
    /// An album's tracks, with its year, label and the artist's other releases.
    fn album_page(&self, id: &str) -> Result<(Vec<Track>, AlbumInfo)>;
    fn artist(&self, id: &str) -> Result<ArtistPage>;
    /// An artist's popular tracks (played straight from an artist card).
    fn artist_top(&self, id: &str) -> Result<Vec<Track>>;
    /// A mix card's tracks.
    fn mix(&self, id: &str) -> Result<Vec<Track>>;
    /// Songs by an artist and ones like them.
    fn artist_mix(&self, id: &str) -> Result<Vec<Track>>;

    // ------------------------------------------------------------ the account

    /// Loved (liked, starred) tracks.
    fn loved(&self) -> Result<Vec<Track>>;
    fn playlists(&self) -> Result<Vec<Playlist>>;
    fn playlist(&self, id: &PlaylistId) -> Result<Vec<Track>>;
    /// The Artists page: followed artists, or a server's whole library.
    fn favorite_artists(&self) -> Result<Vec<Item>>;
    /// The Albums page: liked albums, or a server's whole library.
    fn favorite_albums(&self) -> Result<Vec<Item>>;
    fn likes(&self) -> Result<Likes>;
    fn set_liked(&self, what: &Likeable, liked: bool) -> Result<()>;

    // ------------------------------------------------------------ playing

    /// Start decoding `track` at `at` seconds (`quality` where it applies).
    fn open(&self, track: &Track, quality: Quality, at: f64) -> std::result::Result<Box<dyn Playback>, OpenError>;
    /// The next batch of endless play (Flow, Radio), optionally tuned to a mood or
    /// genre.
    fn endless(&self, tuning: Option<&str>) -> Result<Vec<Track>>;
    /// A track started. Fire-and-forget: may report from a thread of its own.
    fn report_listen_start(&self, song: TrackId);
    /// A track finished or was skipped. Fire-and-forget.
    fn report_listen(&self, listen: Listen);
    /// The URL of a picture this source named, `size` px square.
    fn art_url(&self, art: &Art, size: u32) -> String;

    // ------------------------------------------------------------ optional

    /// A page of sections a [`Channel`](crate::model::ItemKind::Channel) card or a
    /// section's "See all" leads to.
    fn page(&self, _id: &str) -> Result<Page> {
        Err(unsupported("Explore"))
    }
    fn favorite_shows(&self) -> Result<Vec<Item>> {
        Ok(Vec::new())
    }
    fn show_episodes(&self, _id: &str) -> Result<Vec<Track>> {
        Err(unsupported("Podcasts"))
    }
    /// Songs like a track, starting with it.
    fn track_mix(&self, _id: &str) -> Result<Vec<Track>> {
        Err(unsupported("Track mixes"))
    }
    /// Whether `text` pasted into search is a link to this source.
    fn is_link(&self, _text: &str) -> bool {
        false
    }
    fn resolve_link(&self, _pasted: &str) -> Result<Link> {
        Err(unsupported("Links"))
    }
    /// A track's lyrics; None when there are none.
    fn lyrics(&self, _track: &TrackId) -> Result<Option<Lyrics>> {
        Ok(None)
    }
    /// A new playlist holding `tracks`; its id, if the source says.
    fn create_playlist(&self, _title: &str, _private: bool, _tracks: &[TrackId]) -> Result<Option<PlaylistId>> {
        Err(unsupported("Playlist changes"))
    }
    /// Add the tracks that aren't on the playlist yet; how many were added.
    fn add_to_playlist(&self, _playlist: &PlaylistId, _tracks: &[TrackId]) -> Result<usize> {
        Err(unsupported("Playlist changes"))
    }
    fn remove_from_playlist(&self, _playlist: &PlaylistId, _tracks: &[TrackId]) -> Result<()> {
        Err(unsupported("Playlist changes"))
    }
    fn rename_playlist(&self, _playlist: &Playlist, _title: &str) -> Result<()> {
        Err(unsupported("Playlist changes"))
    }
    fn delete_playlist(&self, _playlist: &PlaylistId) -> Result<()> {
        Err(unsupported("Playlist changes"))
    }
}

fn unsupported(what: &str) -> Error {
    Error::Unsupported(what.into())
}

/// Pictures come from the source that named them.
impl ArtResolver for Shared {
    fn art_url(&self, art: &Art, size: u32) -> String {
        (**self).art_url(art, size)
    }
}

/// The player's view of a source.
impl Library for Shared {
    fn open(&self, track: &Track, quality: Quality, at: f64) -> std::result::Result<Box<dyn Playback>, OpenError> {
        (**self).open(track, quality, at)
    }

    fn flow(&self, mood: Option<&str>) -> std::result::Result<Vec<Track>, String> {
        self.endless(mood).map_err(|e| e.to_string())
    }

    fn report_listen_start(&self, song_id: TrackId) {
        (**self).report_listen_start(song_id);
    }

    fn report_listen(&self, listen: Listen) {
        (**self).report_listen(listen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_keep_their_spellings() {
        let deezer = SourceId::from_key("deezer").unwrap();
        assert_eq!((deezer.name, deezer.queue_file, deezer.login_key), ("Deezer", "queue.json", "arl"));
        let server = SourceId::from_key("navidrome").unwrap();
        assert_eq!((server.name, server.queue_file, server.login_key), ("Navidrome", "queue-navidrome.json", "server"));
        assert_eq!(SourceId::from_key("spotify"), None);
        assert_eq!(KINDS[0], deezer, "the default");
    }

    #[test]
    fn kinds_have_their_own_names_and_files() {
        for (i, a) in KINDS.iter().enumerate() {
            for b in &KINDS[i + 1..] {
                assert!(a.key != b.key && a.name != b.name && a.queue_file != b.queue_file && a.login_key != b.login_key, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn sources_say_who_they_are() {
        let deezer: Shared = Arc::new(crate::deezer::Deezer::offline("Dee Zer"));
        assert_eq!((deezer.id(), deezer.account(), deezer.features()), (crate::deezer::ID, "Dee Zer".into(), Features::ALL));
        let server: Shared = Arc::new(crate::subsonic::Subsonic::offline("nas:4533", "me"));
        assert_eq!((server.id(), server.detail(), server.label()), (crate::subsonic::ID, "nas:4533".into(), "Navidrome · nas:4533".into()));
        assert!(!server.features().podcasts && !server.is_link("https://www.deezer.com/album/1"));
        assert!(matches!(server.show_episodes("1"), Err(Error::Unsupported(_))));
    }
}
