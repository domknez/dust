//! What the UI is showing and what it can open: views, collections and their sources.

use crate::deezer::{self, ArtistRef, Deezer, Item, Playlist, Track};
use crate::player::Cmd;

/// The page in the main area.
#[derive(Clone, PartialEq, Debug)]
pub enum View {
    Home,
    Search,
    Loved,
    Playlists,
    /// Artists the account follows.
    Artists,
    /// Albums the account has liked.
    Albums,
    Collection(Box<Coll>),
    Artist(Box<ArtistView>),
}

/// An artist page being shown: enough to draw its header before the page loads.
#[derive(Clone, PartialEq, Debug)]
pub struct ArtistView {
    pub id: String,
    pub name: String,
    /// Picture md5 (`artist` kind on the image CDN), if known yet.
    pub picture: Option<String>,
}

impl ArtistView {
    pub fn from_ref(artist: &ArtistRef) -> Self {
        ArtistView { id: artist.id.clone(), name: artist.name.clone(), picture: None }
    }
}

/// Where a collection's tracks come from.
#[derive(Clone, PartialEq, Debug)]
pub enum Source {
    Playlist(u64),
    Album(String),
    /// Opens the artist's page; played directly, their top tracks.
    Artist(String),
    /// The artist's popular tracks as a plain list ("Show all").
    TopTracks(String),
    Mix(String),
}

impl Source {
    pub fn fetch(&self, client: &Deezer) -> deezer::Result<Vec<Track>> {
        match self {
            Source::Playlist(id) => client.playlist(*id),
            Source::Album(id) => client.album(id),
            Source::Artist(id) | Source::TopTracks(id) => client.artist_top(id),
            Source::Mix(id) => client.mix(id),
        }
    }
}

/// A track collection opened from the sidebar or the home page.
#[derive(Clone, PartialEq, Debug)]
pub struct Coll {
    pub source: Source,
    /// Uppercase label above the title ("PLAYLIST", "ALBUM", ...).
    pub kind: &'static str,
    pub title: String,
    pub subtitle: String,
    pub picture: Option<(String, String)>,
}

impl Coll {
    /// The album a track is on, if Deezer said which.
    pub fn album_of(track: &Track) -> Option<Self> {
        (!track.album_id.is_empty()).then(|| Coll {
            source: Source::Album(track.album_id.clone()),
            kind: "ALBUM",
            title: track.album.clone(),
            subtitle: track.artist.clone(),
            picture: (!track.cover.is_empty()).then(|| ("cover".to_string(), track.cover.clone())),
        })
    }

    pub fn from_playlist(p: &Playlist) -> Self {
        Coll {
            source: Source::Playlist(p.id),
            kind: "PLAYLIST",
            title: p.title.clone(),
            subtitle: String::new(),
            picture: p.picture.clone(),
        }
    }

    /// None for item types we don't open (flows play directly; channels, shows, ...).
    pub fn from_item(it: &Item) -> Option<Self> {
        let (source, kind) = match it.kind.as_str() {
            "playlist" => (Source::Playlist(it.id.parse().ok()?), "PLAYLIST"),
            "album" => (Source::Album(it.id.clone()), "ALBUM"),
            "artist" => (Source::Artist(it.id.clone()), "ARTIST · TOP TRACKS"),
            "smarttracklist" => (Source::Mix(it.id.clone()), "MIX"),
            _ => return None,
        };
        Some(Coll { source, kind, title: it.title.clone(), subtitle: it.subtitle.clone(), picture: it.picture.clone() })
    }

    /// Artists get round artwork.
    pub fn round(&self) -> bool {
        matches!(self.source, Source::Artist(_))
    }
}

/// What to do with fetched tracks.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PlayMode {
    Now,
    Next,
    Queue,
}

impl PlayMode {
    pub fn command(self, tracks: Vec<Track>) -> Cmd {
        match self {
            PlayMode::Now => Cmd::Play(tracks, 0),
            PlayMode::Next => Cmd::PlayNext(tracks),
            PlayMode::Queue => Cmd::Enqueue(tracks),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, id: &str) -> Item {
        Item { kind: kind.into(), id: id.into(), title: "T".into(), subtitle: "S".into(), picture: None }
    }

    #[test]
    fn album_of_track() {
        let track = Track {
            id: 1,
            title: "One More Time".into(),
            artist: "Daft Punk".into(),
            artists: Vec::new(),
            album: "Discovery".into(),
            album_id: "302127".into(),
            duration: 320,
            token: String::new(),
            cover: "abc".into(),
            fallback: None,
        };
        let coll = Coll::album_of(&track).unwrap();
        assert_eq!((coll.source, coll.title.as_str(), coll.subtitle.as_str()), (Source::Album("302127".into()), "Discovery", "Daft Punk"));
        assert_eq!(coll.picture, Some(("cover".into(), "abc".into())));
        assert!(Coll::album_of(&Track { album_id: String::new(), ..track }).is_none());
    }

    #[test]
    fn items_map_to_sources() {
        assert_eq!(Coll::from_item(&item("playlist", "42")).map(|c| c.source), Some(Source::Playlist(42)));
        assert_eq!(Coll::from_item(&item("smarttracklist", "discovery")).map(|c| c.source), Some(Source::Mix("discovery".into())));
        assert!(Coll::from_item(&item("artist", "27")).is_some_and(|c| c.round()));
        assert!(Coll::from_item(&item("channel", "x")).is_none());
        assert!(Coll::from_item(&item("playlist", "not-a-number")).is_none());
    }
}
