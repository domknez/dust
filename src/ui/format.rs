//! Display formatting and artwork URLs.

use crate::deezer::{Playlist, Track, image_url};

/// 245.3 -> "4:05"
pub fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Total length of a track list: "2 hr 14 min" / "37 min".
pub fn total_duration(tracks: &[Track]) -> String {
    let secs: u64 = tracks.iter().map(|t| t.duration as u64).sum();
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    if h > 0 { format!("{h} hr {m} min") } else { format!("{m} min") }
}

pub fn cover_url(t: &Track, size: u32) -> Option<String> {
    (!t.cover.is_empty()).then(|| image_url("cover", &t.cover, size))
}

pub fn playlist_url(p: &Playlist, size: u32) -> Option<String> {
    p.picture.as_ref().map(|(kind, md5)| image_url(kind, md5, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(mmss(245.3), "4:05");
        assert_eq!(mmss(-3.0), "0:00");
        let t = |d| Track {
            id: 1,
            title: String::new(),
            artist: String::new(),
            artists: Vec::new(),
            album: String::new(),
            duration: d,
            token: String::new(),
            cover: String::new(),
            fallback: None,
        };
        assert_eq!(total_duration(&[t(3600), t(840)]), "1 hr 14 min");
        assert_eq!(total_duration(&[t(125)]), "2 min");
    }
}
