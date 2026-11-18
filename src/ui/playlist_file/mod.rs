//! Lists of tracks as files: a list on screen saved as CSV or M3U, readable by
//! other players and services (and by dust itself); and playlists read back from
//! such files (Spotify exports among them), each track then told apart among a
//! source's search results.
//!
//! - [`export`]: writing CSV and M3U
//! - [`read`]: reading sheets, Spotify's JSON, M3U and plain lines
//! - [`matching`]: which found track a file means

mod export;
mod matching;
mod read;

pub use export::Format;
pub use matching::{best, query};
pub use read::{Imported, Unreadable, Wanted, decode, read, readable};

#[cfg(test)]
pub(super) mod fixtures {
    use super::Wanted;
    use crate::model::{Origin, ServerSong, Track, TrackId};
    use crate::ui::app::testing;

    /// A Deezer song, or a server's when `deezer` is false.
    pub fn song(id: &str, title: &str, artist: &str, album: &str, secs: u32, deezer: bool) -> Track {
        let mut t = testing::track(1, true);
        (t.id, t.title, t.artist, t.album, t.duration) = (TrackId::new(id), title.into(), artist.into(), album.into(), secs);
        if !deezer {
            t.origin = Origin::Server(ServerSong { suffix: "flac".into() });
        }
        t
    }

    pub fn want(artist: &str, title: &str, secs: u32) -> Wanted {
        Wanted { title: title.into(), artist: artist.into(), secs, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::song;
    use super::*;

    #[test]
    fn what_dust_exports_it_reads_back() {
        let tracks =
            [song("3135553", "One More Time", "Daft Punk", "Discovery", 320, true), song("s1", "Song, Too", "Band", "Record", 61, false)];
        for format in [Format::Csv, Format::M3u] {
            let list = &read(&format.file_name("Mine", "2026-11-18"), &format.write("Mine", &tracks)).unwrap()[0];
            assert_eq!(list.wanted.len(), 2, "{format:?}");
            assert_eq!(list.wanted[0].link.as_deref(), Some("https://www.deezer.com/track/3135553"));
            let w = &list.wanted[1];
            assert_eq!((w.title.as_str(), w.artist.as_str(), w.album.as_str(), w.secs), ("Song, Too", "Band", "Record", 61), "{format:?}");
        }
    }
}
