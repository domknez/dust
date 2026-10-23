//! A playable track, whichever source it comes from.

/// An artist named on a track or album, so the name can link to their page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtistRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// The source's id for the track (a hash of a music server's string id).
    pub id: u64,
    pub title: String,
    /// All artists joined for display ("A, B").
    pub artist: String,
    /// The same artists, each linkable; empty when the source didn't say.
    pub artists: Vec<ArtistRef>,
    pub album: String,
    /// The album's id, so the album name and cover can open it; empty if unknown.
    pub album_id: String,
    pub duration: u32,
    /// Album cover id (md5) on Deezer's image CDN.
    pub cover: String,
    /// Streamable now. False e.g. for tracks of an album that isn't out yet.
    pub available: bool,
    /// When it becomes streamable (`YYYY-MM-DD`), if the source says; empty otherwise.
    pub available_from: String,
    /// What the track's source needs to play it.
    pub origin: Origin,
}

impl Track {
    pub fn is_episode(&self) -> bool {
        matches!(self.origin, Origin::Episode(_))
    }

    /// The podcast episode details, for an episode.
    pub fn episode(&self) -> Option<&Episode> {
        match &self.origin {
            Origin::Episode(e) => Some(e),
            _ => None,
        }
    }
}

/// Where a track plays from: what only its source reads, kept apart from what
/// every track has.
#[derive(Clone, Debug, PartialEq)]
pub enum Origin {
    /// A song on Deezer.
    Deezer(DeezerSong),
    /// A podcast episode (from Deezer): the show's own audio file.
    Episode(Episode),
    /// A song on a music server (Navidrome / Subsonic).
    Server(ServerSong),
}

/// What Deezer needs to stream a song.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeezerSong {
    /// Grants the media service's stream URL.
    pub token: String,
    /// Alternative version (id, token) Deezer offers when this one isn't licensed
    /// in the listener's region, e.g. a different remaster.
    pub fallback: Option<(u64, String)>,
}

/// What a podcast episode has beyond a track's fields. Its `artist` is the show
/// and its `cover` the show's artwork.
#[derive(Clone, Debug, PartialEq)]
pub struct Episode {
    /// The show's own audio file (plain, no Deezer encryption).
    pub url: String,
    pub show_id: String,
    /// Publication date, `YYYY-MM-DD`.
    pub published: String,
}

/// A song on a Subsonic-compatible server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerSong {
    /// The server's id for the song.
    pub id: String,
    /// File type (`mp3`, `flac`, `m4a`, ...), to pick a decoder or ask for a
    /// conversion.
    pub suffix: String,
}
