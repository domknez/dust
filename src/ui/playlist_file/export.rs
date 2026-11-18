//! Writing a list of tracks as CSV or M3U.

use crate::model::Track;
use crate::ui::format::clock;

/// The longest a file name's part from a list's title gets, in characters.
const MAX_SLUG: usize = 40;

/// The file formats a list can be saved as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// A spreadsheet: title, artist, album, length and link per row.
    Csv,
    /// A playlist other players open (UTF-8, so `.m3u8`); Deezer tracks as links.
    M3u,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::M3u => "m3u8",
        }
    }

    /// The file a list titled `title` is saved as on `day`: "dust-road-trip-2026-11-18.csv".
    pub fn file_name(self, title: &str, day: &str) -> String {
        format!("dust-{}-{day}.{}", slug(title), self.extension())
    }

    /// The list as this format's text.
    pub fn write(self, title: &str, tracks: &[Track]) -> String {
        match self {
            Format::Csv => to_csv(tracks),
            Format::M3u => to_m3u(title, tracks),
        }
    }
}

/// The first row of a CSV.
const CSV_HEADER: &str = "Title,Artist,Album,Duration,Link";

/// One CSV field, quoted when it holds a comma, quote or line break. A field a
/// spreadsheet would run as a formula (starting with `=`, `+`, `-` or `@`) gets
/// an apostrophe in front, which marks it as text.
fn field(text: &str) -> String {
    let text = if text.starts_with(['=', '+', '-', '@']) { format!("'{text}") } else { text.to_string() };
    if text.contains([',', '"', '\n', '\r']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text }
}

/// [`CSV_HEADER`], then a row per track, as RFC 4180 has it.
fn to_csv(tracks: &[Track]) -> String {
    let rows = tracks.iter().map(|t| {
        let duration = clock(u64::from(t.duration));
        let link = t.web_link().unwrap_or_default();
        [t.title.as_str(), &t.artist, &t.album, &duration, &link].map(field).join(",")
    });
    std::iter::once(CSV_HEADER.to_string()).chain(rows).map(|row| row + "\r\n").collect()
}

/// One line of an M3U: no line breaks inside.
fn line(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

/// Extended M3U: per track `#EXTINF` (length, "Artist - Title"), `#EXTALB`, then
/// its web page. A track without one (a server's song) has no entry line: other
/// players skip it, dust's import still reads it.
fn to_m3u(title: &str, tracks: &[Track]) -> String {
    let mut lines = vec!["#EXTM3U".to_string(), format!("#PLAYLIST:{}", line(title))];
    for t in tracks {
        lines.push(format!("#EXTINF:{},{} - {}", t.duration, line(&t.artist), line(&t.title)));
        if !t.album.is_empty() {
            lines.push(format!("#EXTALB:{}", line(&t.album)));
        }
        lines.extend(t.web_link());
    }
    lines.iter().map(|l| format!("{l}\n")).collect()
}

/// A file name from a list's title: "Rock & Roll Hits!" → "rock-roll-hits"; whole
/// words, up to [`MAX_SLUG`] characters.
fn slug(title: &str) -> String {
    let words: Vec<String> = title.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect();
    let mut slug = String::new();
    for w in words {
        if slug.chars().count() + w.chars().count() > MAX_SLUG {
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(&w);
    }
    if slug.is_empty() { "list".into() } else { slug }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::playlist_file::fixtures::song;

    #[test]
    fn a_list_as_csv_quotes_what_needs_it() {
        let tracks = [
            song("3135553", "One More Time", "Daft Punk", "Discovery", 320, true),
            song("s1", "Say \"Hi\", Bye", "A, B", "Live", 3725, false),
        ];
        assert_eq!(
            to_csv(&tracks),
            "Title,Artist,Album,Duration,Link\r\n\
             One More Time,Daft Punk,Discovery,5:20,https://www.deezer.com/track/3135553\r\n\
             \"Say \"\"Hi\"\", Bye\",\"A, B\",Live,1:02:05,\r\n"
        );
    }

    #[test]
    fn csv_fields_a_spreadsheet_would_run_stay_text() {
        assert_eq!(field("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!((field("+1"), field("-ish"), field("@me"), field("a=b")), ("'+1".into(), "'-ish".into(), "'@me".into(), "a=b".into()));
    }

    #[test]
    fn a_list_as_m3u_links_deezer_songs_and_names_the_album_otherwise() {
        let tracks =
            [song("3135553", "One More Time", "Daft Punk", "Discovery", 320, true), song("s1", "Song", "Band", "Record", 61, false)];
        assert_eq!(
            to_m3u("Mine\nnow", &tracks),
            "#EXTM3U\n#PLAYLIST:Mine now\n\
             #EXTINF:320,Daft Punk - One More Time\n#EXTALB:Discovery\nhttps://www.deezer.com/track/3135553\n\
             #EXTINF:61,Band - Song\n#EXTALB:Record\n"
        );
    }

    #[test]
    fn file_names_come_from_the_title() {
        assert_eq!(slug("Rock & Roll Hits!"), "rock-roll-hits");
        assert_eq!(slug("Ćevapi, šljive"), "ćevapi-šljive");
        assert_eq!(slug("!!!"), "list");
        assert!(slug(&"word ".repeat(30)).len() <= MAX_SLUG);
        assert_eq!(Format::M3u.file_name("Road Trip", "2026-11-18"), "dust-road-trip-2026-11-18.m3u8");
    }
}
