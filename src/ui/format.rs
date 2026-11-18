//! Display formatting.

use crate::model::Track;

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

/// `n` of `thing`: "1 track", "12 tracks" (a regular plural).
pub fn count(n: usize, thing: &str) -> String {
    if n == 1 { format!("1 {thing}") } else { format!("{n} {thing}s") }
}

/// How long ago something `secs` ago was: "just now", "a minute ago", "5 minutes
/// ago", "an hour ago", "3 hours ago", "yesterday", "4 days ago".
pub fn ago(secs: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    match secs {
        0..MINUTE => "just now".into(),
        MINUTE..HOUR if secs < 2 * MINUTE => "a minute ago".into(),
        MINUTE..HOUR => format!("{} minutes ago", secs / MINUTE),
        HOUR..DAY if secs < 2 * HOUR => "an hour ago".into(),
        HOUR..DAY => format!("{} hours ago", secs / HOUR),
        _ if secs < 2 * DAY => "yesterday".into(),
        _ => format!("{} days ago", secs / DAY),
    }
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
    fn counts_and_times_read_naturally() {
        assert_eq!((count(1, "track"), count(0, "track"), count(12, "track")), ("1 track".into(), "0 tracks".into(), "12 tracks".into()));
        let ago_all = [0, 59, 60, 119, 120, 3599, 3600, 7199, 7200, 86_399, 86_400, 172_800].map(ago);
        assert_eq!(
            ago_all,
            [
                "just now",
                "just now",
                "a minute ago",
                "a minute ago",
                "2 minutes ago",
                "59 minutes ago",
                "an hour ago",
                "an hour ago",
                "2 hours ago",
                "23 hours ago",
                "yesterday",
                "2 days ago"
            ]
        );
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
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            album_id: String::new(),
            duration: d,
            ..Track::test(1)
        };
        assert_eq!(total_clock(&[t(100), t(3012)]), "51:52");
        assert_eq!(total_clock(&[t(3600), t(123)]), "1:02:03");
        assert_eq!(total_clock(&[]), "0:00");
    }
}
