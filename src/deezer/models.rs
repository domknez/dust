//! Data types returned by the client.

/// An artist named on a track or album, so the name can link to their page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtistRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: u64,
    pub title: String,
    /// All artists joined for display ("A, B").
    pub artist: String,
    /// The same artists, each linkable; empty when Deezer didn't say.
    pub artists: Vec<ArtistRef>,
    pub album: String,
    /// Deezer album id, so the album name and cover can open it; empty if unknown.
    pub album_id: String,
    pub duration: u32,
    pub token: String,
    /// Album cover id (md5) on Deezer's image CDN.
    pub cover: String,
    /// Alternative version (id, token) Deezer offers when this one isn't licensed
    /// in the listener's region, e.g. a different remaster.
    pub fallback: Option<(u64, String)>,
}

#[derive(Clone, Debug)]
pub struct Playlist {
    pub id: u64,
    pub title: String,
    pub count: u32,
    /// (`cover`|`playlist`|..., md5) on Deezer's image CDN.
    pub picture: Option<(String, String)>,
}

/// One entry on a Deezer page (home): a playlist, album, artist, mix, flow, ...
#[derive(Clone, Debug)]
pub struct Item {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    /// (`cover`|`playlist`|`artist`|..., md5) on Deezer's image CDN.
    pub picture: Option<(String, String)>,
}

/// An artist's page: header, popular tracks and card sections (discography,
/// related artists, playlists).
#[derive(Clone, Debug, Default)]
pub struct ArtistPage {
    pub name: String,
    /// Picture md5 on the image CDN (`artist` kind).
    pub picture: Option<String>,
    pub fans: u64,
    pub top: Vec<Track>,
    pub sections: Vec<Section>,
}

/// Search hits: tracks, plus artist, album and playlist card sections.
#[derive(Clone, Debug, Default)]
pub struct SearchResults {
    pub tracks: Vec<Track>,
    pub sections: Vec<Section>,
}

#[derive(Clone, Debug)]
pub struct Section {
    pub title: String,
    pub layout: String,
    pub items: Vec<Item>,
}

/// Square image from Deezer's CDN, e.g. `image_url("cover", md5, 120)`.
pub fn image_url(kind: &str, md5: &str, size: u32) -> String {
    format!("https://cdn-images.dzcdn.net/images/{kind}/{md5}/{size}x{size}-000000-80-0-0.jpg")
}

/// Streaming quality the listener asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    Mp3_128,
    Mp3_320,
    Flac,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Mp3_128, Quality::Mp3_320, Quality::Flac];

    pub fn label(self) -> &'static str {
        match self {
            Quality::Mp3_128 => "MP3 128",
            Quality::Mp3_320 => "MP3 320",
            Quality::Flac => "FLAC",
        }
    }

    /// Stable key for the settings file.
    pub fn key(self) -> &'static str {
        match self {
            Quality::Mp3_128 => "mp3_128",
            Quality::Mp3_320 => "mp3_320",
            Quality::Flac => "flac",
        }
    }

    pub fn from_key(k: &str) -> Option<Quality> {
        Quality::ALL.into_iter().find(|q| q.key() == k)
    }

    /// Requested format and every lower one, best first.
    pub(super) fn api_formats(self) -> &'static [&'static str] {
        match self {
            Quality::Flac => &["FLAC", "MP3_320", "MP3_128"],
            Quality::Mp3_320 => &["MP3_320", "MP3_128"],
            Quality::Mp3_128 => &["MP3_128"],
        }
    }
}

/// Format actually delivered for a stream.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    /// Constant bitrate MP3, kbps.
    Mp3(u32),
    Flac,
}

impl Format {
    pub(super) fn from_api(name: Option<&str>) -> Format {
        match name {
            Some("FLAC") => Format::Flac,
            Some("MP3_128") => Format::Mp3(128),
            _ => Format::Mp3(320),
        }
    }

    pub(super) fn api_name(self) -> &'static str {
        match self {
            Format::Flac => "FLAC",
            Format::Mp3(128) => "MP3_128",
            Format::Mp3(_) => "MP3_320",
        }
    }
}

/// One listen, as reported to Deezer.
#[derive(Clone, Debug)]
pub struct Listen {
    pub song_id: u64,
    pub format: Format,
    pub started_unix: u64,
    pub listened_secs: u64,
    /// Ended before the track did (skip/seek), as opposed to playing through.
    pub skipped: bool,
    pub stream_id: String,
}
