//! Streaming quality: what the listener asks for and what a stream delivers.

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
}

/// Format actually delivered for a stream.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    /// Constant bitrate MP3, kbps.
    Mp3(u32),
    Flac,
    /// A podcast episode's own file (MP3 at any bitrate).
    Podcast,
    /// A song file from a music server (any codec, any sample rate).
    File,
}
