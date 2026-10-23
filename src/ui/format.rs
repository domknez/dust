//! Display formatting and artwork URLs.

use crate::deezer::image_url;
use crate::model::{Playlist, Track};

/// 245.3 -> "4:05"
pub fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Total length of a track list as a clock: "51:52", or "1:02:03" past an hour.
pub fn total_clock(tracks: &[Track]) -> String {
    let secs: u64 = tracks.iter().map(|t| t.duration as u64).sum();
    match secs / 3600 {
        0 => format!("{}:{:02}", secs / 60, secs % 60),
        h => format!("{h}:{:02}:{:02}", (secs % 3600) / 60, secs % 60),
    }
}

/// "2026-11-20" -> "20 Nov 2026"; None for anything else.
pub fn release_date(iso: &str) -> Option<String> {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let mut parts = iso.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?.parse::<usize>().ok()?, parts.next()?.parse::<u32>().ok()?);
    (year.len() == 4 && (1..=12).contains(&month)).then(|| format!("{day} {} {year}", MONTHS[month - 1]))
}

/// 1234567 -> "1,234,567"
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn cover_url(t: &Track, size: u32) -> Option<String> {
    (!t.cover.is_empty()).then(|| image_url(if t.is_episode() { "talk" } else { "cover" }, &t.cover, size))
}

pub fn playlist_url(p: &Playlist, size: u32) -> Option<String> {
    p.picture.as_ref().map(|(kind, md5)| image_url(kind, md5, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DeezerSong, Origin};

    #[test]
    fn release_dates_read_naturally() {
        assert_eq!(release_date("2026-11-20").as_deref(), Some("20 Nov 2026"));
        assert_eq!(release_date("2000-01-01").as_deref(), Some("1 Jan 2000"));
        assert_eq!(release_date(""), None);
        assert_eq!(release_date("2026-13-01"), None);
    }

    #[test]
    fn counts_have_separators() {
        assert_eq!(thousands(4702289), "4,702,289");
        assert_eq!(thousands(999), "999");
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
            cover: String::new(),
            available: true,
            available_from: String::new(),
            origin: Origin::Deezer(DeezerSong::default()),
        };
        assert_eq!(total_clock(&[t(100), t(3012)]), "51:52");
        assert_eq!(total_clock(&[t(3600), t(123)]), "1:02:03");
        assert_eq!(total_clock(&[]), "0:00");
    }
}
