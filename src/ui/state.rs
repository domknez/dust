//! What the UI is showing and what it can open: views, collections and their sources.

use crate::model::{Art, ArtistRef, Item, ItemKind, Playlist, PlaylistId, Track};
use crate::player::Cmd;
use crate::source::Source;

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
    /// Podcasts the account follows.
    Podcasts,
    Collection(Box<Coll>),
    Artist(Box<ArtistView>),
}

/// An artist page being shown: enough to draw its header before the page loads.
#[derive(Clone, PartialEq, Debug)]
pub struct ArtistView {
    pub id: String,
    pub name: String,
    /// The artist's picture, if known yet.
    pub picture: Option<Art>,
}

impl ArtistView {
    pub fn from_ref(artist: &ArtistRef) -> Self {
        ArtistView { id: artist.id.clone(), name: artist.name.clone(), picture: None }
    }
}

/// Where a collection's tracks come from.
#[derive(Clone, PartialEq, Debug)]
pub enum CollSource {
    Playlist(PlaylistId),
    Album(String),
    /// Opens the artist's page; played directly, their top tracks.
    Artist(String),
    /// The artist's popular tracks as a plain list ("Show all").
    TopTracks(String),
    /// Songs like a track, starting with it ("Mixes inspired by…").
    TrackMix(String),
    Mix(String),
    /// A podcast: its episodes, newest first.
    Show(String),
}

impl CollSource {
    pub fn fetch(&self, client: &dyn Source) -> crate::model::Result<Vec<Track>> {
        match self {
            CollSource::Playlist(id) => client.playlist(id),
            CollSource::Album(id) => client.album(id),
            CollSource::Artist(id) | CollSource::TopTracks(id) => client.artist_top(id),
            CollSource::TrackMix(id) => client.track_mix(id),
            CollSource::Mix(id) => client.mix(id),
            CollSource::Show(id) => client.show_episodes(id),
        }
    }
}

/// A track collection opened from the sidebar or the home page.
#[derive(Clone, PartialEq, Debug)]
pub struct Coll {
    pub source: CollSource,
    /// Uppercase label above the title ("PLAYLIST", "ALBUM", ...).
    pub kind: &'static str,
    pub title: String,
    pub subtitle: String,
    pub picture: Option<Art>,
}

impl Coll {
    /// The album a track is on, if Deezer said which.
    pub fn album_of(track: &Track) -> Option<Self> {
        (!track.album_id.is_empty()).then(|| Coll {
            source: CollSource::Album(track.album_id.clone()),
            kind: "ALBUM",
            title: track.album.clone(),
            subtitle: track.artist.clone(),
            picture: track.art.clone(),
        })
    }

    /// Where the title of a playing track leads: its album, or an episode's podcast.
    pub fn origin_of(track: &Track) -> Option<Self> {
        match track.episode() {
            Some(episode) if !episode.show_id.is_empty() => Some(Coll {
                source: CollSource::Show(episode.show_id.clone()),
                kind: "PODCAST",
                title: track.artist.clone(),
                subtitle: String::new(),
                picture: track.art.clone(),
            }),
            Some(_) => None,
            None => Self::album_of(track),
        }
    }

    pub fn from_playlist(p: &Playlist) -> Self {
        Coll {
            source: CollSource::Playlist(p.id.clone()),
            kind: "PLAYLIST",
            title: p.title.clone(),
            subtitle: String::new(),
            picture: p.picture.clone(),
        }
    }

    /// None for cards we don't open (Flow and Radio play directly; channels, ...).
    pub fn from_item(it: &Item) -> Option<Self> {
        let (source, kind) = match &it.kind {
            ItemKind::Playlist if it.id.is_empty() => return None,
            ItemKind::Playlist => (CollSource::Playlist(PlaylistId::new(&it.id)), "PLAYLIST"),
            ItemKind::Album => (CollSource::Album(it.id.clone()), "ALBUM"),
            ItemKind::Artist => (CollSource::Artist(it.id.clone()), "ARTIST · TOP TRACKS"),
            ItemKind::Mix => (CollSource::Mix(it.id.clone()), "MIX"),
            ItemKind::TrackMix => (CollSource::TrackMix(it.id.clone()), "TRACK MIX"),
            ItemKind::Show => (CollSource::Show(it.id.clone()), "PODCAST"),
            ItemKind::Flow { .. } | ItemKind::Radio { .. } | ItemKind::Other(_) => return None,
        };
        Some(Coll { source, kind, title: it.title.clone(), subtitle: it.subtitle.clone(), picture: it.picture.clone() })
    }

    /// Artists get round artwork.
    pub fn round(&self) -> bool {
        matches!(self.source, CollSource::Artist(_))
    }
}

/// The tracks of `tracks` that can be played (some can't, yet or where the
/// listener is).
pub fn playable(mut tracks: Vec<Track>) -> Vec<Track> {
    tracks.retain(|t| t.available);
    tracks
}

/// The tracks of `tracks` that can go into a playlist: playable songs, no episodes.
pub fn for_playlist(tracks: Vec<Track>) -> Vec<Track> {
    playable(tracks).into_iter().filter(|t| !t.is_episode()).collect()
}

/// The playable tracks of `tracks`, and where `start` lands among them (the next
/// playable one if `start` itself isn't). None when nothing can be played.
pub fn playable_from(tracks: Vec<Track>, start: usize) -> Option<(Vec<Track>, usize)> {
    let index = tracks.iter().take(start).filter(|t| t.available).count();
    let playable = playable(tracks);
    (index < playable.len()).then_some((playable, index))
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
    use crate::model::{DeezerSong, Origin, TrackId};

    fn item(kind: ItemKind, id: &str) -> Item {
        Item { kind, id: id.into(), title: "T".into(), subtitle: "S".into(), picture: None }
    }

    #[test]
    fn album_of_track() {
        let track = Track {
            id: TrackId::from(1),
            title: "One More Time".into(),
            artist: "Daft Punk".into(),
            artists: Vec::new(),
            album: "Discovery".into(),
            album_id: "302127".into(),
            duration: 320,
            art: Art::new("cover/abc"),
            available: true,
            available_from: String::new(),
            origin: Origin::Deezer(DeezerSong::default()),
        };
        let coll = Coll::album_of(&track).unwrap();
        assert_eq!(
            (coll.source, coll.title.as_str(), coll.subtitle.as_str()),
            (CollSource::Album("302127".into()), "Discovery", "Daft Punk")
        );
        assert_eq!(coll.picture, Art::new("cover/abc"));
        assert!(Coll::album_of(&Track { album_id: String::new(), ..track }).is_none());
    }

    #[test]
    fn only_playable_tracks_are_queued() {
        let t = |id: u64, available| Track {
            id: TrackId::from(id),
            title: String::new(),
            artist: String::new(),
            artists: Vec::new(),
            album: String::new(),
            album_id: String::new(),
            duration: 1,
            art: None,
            available,
            available_from: String::new(),
            origin: Origin::Deezer(DeezerSong::default()),
        };
        let tracks = vec![t(1, true), t(2, false), t(3, false), t(4, true)];
        let ids = |(q, i): (Vec<Track>, usize)| (q.iter().map(|t| t.id.to_string()).collect::<Vec<_>>(), i);
        assert_eq!(playable_from(tracks.clone(), 3).map(ids), Some((vec!["1".into(), "4".into()], 1)));
        assert_eq!(playable_from(tracks, 1).map(ids), Some((vec!["1".into(), "4".into()], 1)), "an unavailable start moves on");
        assert_eq!(playable_from(vec![t(2, false)], 0).map(ids), None);
    }

    #[test]
    fn items_map_to_sources() {
        assert_eq!(Coll::from_item(&item(ItemKind::Playlist, "42")).map(|c| c.source), Some(CollSource::Playlist(PlaylistId::new("42"))));
        assert_eq!(Coll::from_item(&item(ItemKind::Mix, "discovery")).map(|c| c.source), Some(CollSource::Mix("discovery".into())));
        assert!(Coll::from_item(&item(ItemKind::Artist, "27")).is_some_and(|c| c.round()));
        assert!(Coll::from_item(&item(ItemKind::Other("channel".into()), "x")).is_none());
        assert_eq!(
            Coll::from_item(&item(ItemKind::TrackMix, "4229026412")).map(|c| c.source),
            Some(CollSource::TrackMix("4229026412".into()))
        );
        assert_eq!(
            Coll::from_item(&item(ItemKind::Playlist, "J78Lrnn5aCX1IyCU5feMey")).map(|c| c.source),
            Some(CollSource::Playlist(PlaylistId::new("J78Lrnn5aCX1IyCU5feMey"))),
            "a music server's own id"
        );
        assert!(Coll::from_item(&item(ItemKind::Playlist, "")).is_none());
        assert_eq!(Coll::from_item(&item(ItemKind::Show, "724752")).map(|c| c.source), Some(CollSource::Show("724752".into())));
        assert!(Coll::from_item(&item(ItemKind::Flow { mood: None }, "default")).is_none(), "Flow plays, it doesn't open");
    }
}
