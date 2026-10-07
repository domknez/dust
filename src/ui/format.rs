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

/// "2026-11-20" -> "20 Nov 2026"; None for anything else.
pub fn release_date(iso: &str) -> Option<String> {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let mut parts = iso.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?.parse::<usize>().ok()?, parts.next()?.parse::<u32>().ok()?);
    (year.len() == 4 && (1..=12).contains(&month)).then(|| format!("{day} {} {year}", MONTHS[month - 1]))
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
    fn release_dates_read_naturally() {
        assert_eq!(release_date("2026-11-20").as_deref(), Some("20 Nov 2026"));
        assert_eq!(release_date("2000-01-01").as_deref(), Some("1 Jan 2000"));
        assert_eq!(release_date(""), None);
        assert_eq!(release_date("2026-13-01"), None);
    }

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
            album_id: String::new(),
            duration: d,
            token: String::new(),
            cover: String::new(),
            fallback: None,
            available: true,
            available_from: String::new(),
        };
        assert_eq!(total_duration(&[t(3600), t(840)]), "1 hr 14 min");
        assert_eq!(total_duration(&[t(125)]), "2 min");
    }
}
