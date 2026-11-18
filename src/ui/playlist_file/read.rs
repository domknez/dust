//! Reading playlists from files: CSV/TSV sheets (Exportify, Soundiiz,
//! TuneMyMusic, dust's own export), Spotify's account data (JSON), M3U and plain
//! "Artist - Title" lines.

use serde_json::Value;
use std::path::Path;

/// A track a file asks for, to be found at the source.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wanted {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Length in seconds; 0 when the file doesn't say.
    pub secs: u32,
    /// A web address the file gives for the track, for a source to tell it by.
    pub link: Option<String>,
}

impl Wanted {
    /// Whether the file says enough to look the track up: a title or a link.
    fn is_named(&self) -> bool {
        !self.title.is_empty() || self.link.is_some()
    }

    /// "Artist - Title", as the file has it.
    pub fn name(&self) -> String {
        match (self.artist.is_empty(), self.title.is_empty()) {
            (false, false) => format!("{} - {}", self.artist, self.title),
            (true, false) => self.title.clone(),
            _ => self.link.clone().unwrap_or_default(),
        }
    }
}

/// A playlist read from a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    pub title: String,
    pub wanted: Vec<Wanted>,
}

/// A file's text, however it was saved: UTF-8 (with or without a byte order
/// mark), UTF-16 with one (Excel's "Unicode text"), else Windows-1252 (older
/// players' M3U files).
pub fn decode(bytes: &[u8]) -> String {
    let utf16 = |bytes: &[u8], word: fn([u8; 2]) -> u16| {
        let words: Vec<u16> = bytes.chunks_exact(2).map(|pair| word([pair[0], pair[1]])).collect();
        String::from_utf16_lossy(&words)
    };
    match bytes {
        [0xef, 0xbb, 0xbf, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xff, 0xfe, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xfe, 0xff, rest @ ..] => utf16(rest, u16::from_be_bytes),
        _ => match std::str::from_utf8(bytes) {
            Ok(text) => text.to_string(),
            Err(_) => bytes.iter().map(|&b| windows_1252(b)).collect(),
        },
    }
}

/// A Windows-1252 byte as its character: Latin-1, but for the printable
/// characters it puts at 0x80–0x9F (curly quotes, dashes, €).
fn windows_1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•',
        '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9f => HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

/// Whether an import reads files like `path`.
pub fn readable(path: &Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "csv" | "tsv" | "m3u" | "m3u8" | "json" | "txt")
}

/// Why a file gave no playlist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unreadable {
    /// Nothing in it names a track.
    NoTracks,
    /// A spreadsheet, but no column of its header is one of track titles.
    NoTitleColumn,
}

/// The playlists in a file named `name` (a Spotify data export can hold many);
/// the file's name is the title when it doesn't give one.
pub fn read(name: &str, text: &str) -> Result<Vec<Imported>, Unreadable> {
    let text = text.trim_start_matches('\u{feff}');
    let path = Path::new(name);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let lists = match ext.as_str() {
        "json" => from_json(text, &stem),
        "m3u" | "m3u8" => vec![from_lines(text, &stem, Comments::Skipped)],
        "csv" | "tsv" => match from_csv(text, &stem) {
            Some(list) => vec![list],
            None if text.trim().is_empty() => Vec::new(),
            None => return Err(Unreadable::NoTitleColumn),
        },
        // A text file can be a spreadsheet too.
        _ => vec![from_csv(text, &stem).unwrap_or_else(|| from_lines(text, &stem, Comments::Kept))],
    };
    let lists: Vec<Imported> = lists.into_iter().filter(|l| !l.wanted.is_empty()).collect();
    if lists.is_empty() { Err(Unreadable::NoTracks) } else { Ok(lists) }
}

/// The fields of a CSV (or TSV, or semicolon-separated) text, row by row.
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let first = text.lines().next().unwrap_or_default();
    let sep = [',', ';', '\t'].into_iter().max_by_key(|&c| first.matches(c).count()).unwrap_or(',');
    let (mut rows, mut row, mut cell, mut quoted) = (Vec::new(), Vec::new(), String::new(), false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' if quoted => quoted = false,
            '"' if cell.is_empty() => quoted = true,
            _ if quoted => cell.push(c),
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut cell));
                if row.iter().any(|f| !f.trim().is_empty()) {
                    rows.push(std::mem::take(&mut row));
                }
                row.clear();
            }
            _ if c == sep => row.push(std::mem::take(&mut cell)),
            _ => cell.push(c),
        }
    }
    row.push(cell);
    if row.iter().any(|f| !f.trim().is_empty()) {
        rows.push(row);
    }
    rows
}

/// A spreadsheet with a header naming at least the title column: exports from
/// Exportify, Soundiiz, TuneMyMusic and dust.
fn from_csv(text: &str, stem: &str) -> Option<Imported> {
    let rows = csv_rows(text);
    let (header, rows) = rows.split_first()?;
    let header: Vec<String> = header.iter().map(|h| h.trim().to_lowercase()).collect();
    let column = |names: &[&str]| names.iter().find_map(|n| header.iter().position(|h| h == n));
    let title = column(&["track name", "title", "track title", "song", "song name", "name", "track"])?;
    let artist = column(&["artist name(s)", "artist name", "artist names", "artist", "artists"]);
    let album = column(&["album name", "album", "album title", "release"]);
    let ms = column(&["duration (ms)", "duration_ms", "track duration (ms)"]);
    let length = column(&["duration", "length", "time"]);
    let link = column(&["link", "url", "deezer link", "deezer url"]);
    let get = |row: &[String], at: Option<usize>| at.and_then(|k| row.get(k)).map(|f| unguarded(f.trim()).to_string()).unwrap_or_default();
    let wanted = rows
        .iter()
        .map(|row| Wanted {
            title: get(row, Some(title)),
            artist: get(row, artist),
            album: get(row, album),
            secs: match ms {
                Some(_) => {
                    get(row, ms).parse::<u64>().ok().and_then(|ms| u32::try_from(ms / 1000 + u64::from(ms % 1000 >= 500)).ok()).unwrap_or(0)
                }
                None => seconds(&get(row, length)),
            },
            link: web_address(&get(row, link)),
        })
        .filter(Wanted::is_named)
        .collect();
    Some(Imported { title: stem.to_string(), wanted })
}

/// A field as written, without the apostrophe a spreadsheet export puts before
/// text that would otherwise run as a formula ("'=Song").
fn unguarded(field: &str) -> &str {
    match field.strip_prefix('\'') {
        Some(rest) if rest.starts_with(['=', '+', '-', '@']) => rest,
        _ => field,
    }
}

/// "3:07", "1:02:03" or a number of seconds; 0 for anything else.
fn seconds(text: &str) -> u32 {
    text.split(':').try_fold(0u32, |total, part| total.checked_mul(60)?.checked_add(part.trim().parse().ok()?)).unwrap_or(0)
}

/// Spotify's account data: `Playlist1.json` (every playlist) and `YourLibrary.json`
/// (the liked songs).
fn from_json(text: &str, stem: &str) -> Vec<Imported> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let s = |v: &Value| v.as_str().unwrap_or_default().trim().to_string();
    let mut lists: Vec<Imported> = v["playlists"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|list| {
            let wanted = list["items"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|item| &item["track"])
                .map(|t| Wanted { title: s(&t["trackName"]), artist: s(&t["artistName"]), album: s(&t["albumName"]), ..Default::default() })
                .filter(|w| !w.title.is_empty())
                .collect();
            let title = s(&list["name"]);
            Imported { title: if title.is_empty() { stem.to_string() } else { title }, wanted }
        })
        .collect();
    let liked: Vec<Wanted> = v["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| Wanted { title: s(&t["track"]), artist: s(&t["artist"]), album: s(&t["album"]), ..Default::default() })
        .filter(|w| !w.title.is_empty())
        .collect();
    if !liked.is_empty() {
        lists.push(Imported { title: "Liked songs".into(), wanted: liked });
    }
    lists
}

/// What an `#EXTINF` (and `#EXTALB`) says, waiting for the entry line it describes.
#[derive(Default)]
struct Info {
    secs: u32,
    name: String,
    album: String,
}

impl Info {
    /// `#EXTINF:<secs>,<name>`; a length of -1 (unknown) or past `u32` reads as 0.
    fn parse(rest: &str) -> Info {
        let (secs, name) = rest.split_once(',').unwrap_or((rest, ""));
        let secs = secs.split_whitespace().next().and_then(|n| n.parse::<i64>().ok()).and_then(|n| u32::try_from(n).ok()).unwrap_or(0);
        Info { secs, name: name.trim().to_string(), album: String::new() }
    }

    /// The track this describes, at the entry `line` (None: it has none).
    fn wanted(self, line: Option<&str>) -> Wanted {
        let link = line.and_then(web_address);
        let name = match line {
            Some(line) if self.name.is_empty() && link.is_none() => entry_name(line),
            _ => self.name,
        };
        let (artist, title) = split_name(&name);
        Wanted { title, artist, album: self.album, secs: self.secs, link }
    }
}

/// What a line starting with `#` (other than an M3U directive) is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Comments {
    /// A comment, as in an M3U.
    Skipped,
    /// A name, as in plain text ("#9 Dream").
    Kept,
}

/// An M3U playlist or plain text: `#EXTINF:<secs>,<Artist> - <Title>` (and
/// `#EXTALB:<album>`) before each entry, or lines of "Artist - Title", file paths
/// or web links.
fn from_lines(text: &str, stem: &str, comments: Comments) -> Imported {
    let mut title = None;
    let mut wanted = Vec::new();
    let mut info: Option<Info> = None;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(name) = line.strip_prefix("#PLAYLIST:") {
            title = Some(name.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
            // The one before had no entry line (dust writes a server's songs so).
            wanted.extend(info.take().map(|i| i.wanted(None)));
            info = Some(Info::parse(rest));
        } else if let Some(album) = line.strip_prefix("#EXTALB:") {
            if let Some(info) = &mut info {
                info.album = album.trim().to_string();
            }
        } else if !line.starts_with('#') || comments == Comments::Kept {
            wanted.push(info.take().unwrap_or_default().wanted(Some(line)));
        }
    }
    wanted.extend(info.map(|i| i.wanted(None)));
    wanted.retain(Wanted::is_named);
    Imported { title: title.filter(|t| !t.is_empty()).unwrap_or_else(|| stem.to_string()), wanted }
}

/// "Artist - Title" → ("Artist", "Title"); a name without a dash is a title.
fn split_name(name: &str) -> (String, String) {
    match name.split_once(" - ").or_else(|| name.split_once(" – ")) {
        Some((artist, title)) => (artist.trim().to_string(), title.trim().to_string()),
        None => (String::new(), name.trim().to_string()),
    }
}

/// Extensions of the audio files a playlist lists.
const AUDIO: [&str; 11] = ["mp3", "flac", "m4a", "aac", "ogg", "opus", "wav", "aiff", "aif", "alac", "wma"];

/// What an entry line names: an audio file's name without its number and
/// extension ("Music/Band/01 - Band - Song.mp3" → "Band - Song"), or the line.
fn entry_name(line: &str) -> String {
    let file = line.rsplit(['/', '\\']).next().unwrap_or(line);
    let path = Path::new(file);
    let audio = path.extension().is_some_and(|e| AUDIO.contains(&e.to_string_lossy().to_lowercase().as_str()));
    if !audio && !line.contains(['/', '\\']) {
        return line.to_string();
    }
    let stem = path.file_stem().map_or(file.to_string(), |s| s.to_string_lossy().to_string());
    without_track_number(&stem).trim().to_string()
}

/// A file name without the track number in front: "01 - Song", "01. Song",
/// "01_Song", "01 Song" (a space only after a zero-padded number). A number that
/// is the name or starts it ("1979", "99 Luftballons", "21 Guns") stays.
fn without_track_number(stem: &str) -> &str {
    let rest = stem.trim_start_matches(|c: char| c.is_ascii_digit());
    if rest.len() == stem.len() {
        return stem;
    }
    let padded = stem.starts_with('0');
    let name = [" - ", ". ", "_", "-", "."]
        .into_iter()
        .find_map(|separator| rest.strip_prefix(separator))
        .or_else(|| rest.strip_prefix(' ').filter(|_| padded));
    name.filter(|n| !n.trim().is_empty()).unwrap_or(stem)
}

/// `text` when it's a web address ("https://…").
fn web_address(text: &str) -> Option<String> {
    let text = text.trim();
    let scheme = text.get(..8).unwrap_or(text).to_ascii_lowercase();
    (scheme.starts_with("https://") || scheme.starts_with("http://")).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::playlist_file::fixtures::want;

    #[test]
    fn exportify_csv_reads_with_quoted_artists_and_milliseconds() {
        let text = "\u{feff}Track URI,Track Name,Artist Name(s),Album Name,Duration (ms)\n\
                    spotify:track:1,One More Time,Daft Punk,Discovery,320357\n\
                    spotify:track:2,\"Get Lucky (feat. Pharrell Williams)\",\"Daft Punk, Pharrell Williams\",Random Access Memories,369626\n";
        let lists = read("Road trip.csv", text).unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].title, "Road trip");
        assert_eq!(
            lists[0].wanted[0],
            Wanted { title: "One More Time".into(), artist: "Daft Punk".into(), album: "Discovery".into(), secs: 320, link: None }
        );
        assert_eq!(lists[0].wanted[1].artist, "Daft Punk, Pharrell Williams");
        assert_eq!(lists[0].wanted[1].secs, 370);
    }

    #[test]
    fn semicolon_and_tab_sheets_read_too() {
        let semicolons = read("list.txt", "Title;Artist;Length\nSong;Band;3:07\n").unwrap();
        assert_eq!(semicolons[0].wanted, vec![Wanted { title: "Song".into(), artist: "Band".into(), secs: 187, ..Default::default() }]);
        let tabs = read("list.tsv", "Artist\tTitle\r\nBand\tSong\r\n").unwrap();
        assert_eq!(tabs[0].wanted, vec![want("Band", "Song", 0)]);
    }

    #[test]
    fn quoted_fields_keep_separators_quotes_and_line_breaks() {
        let rows = csv_rows("a,\"b, \"\"c\"\"\",\"d\ne\"\r\n\r\nf,g,h");
        assert_eq!(rows, [vec!["a", "b, \"c\"", "d\ne"], vec!["f", "g", "h"]]);
    }

    #[test]
    fn lengths_read_as_clocks_or_seconds() {
        assert_eq!((seconds("3:07"), seconds("1:02:03"), seconds("245"), seconds("")), (187, 3723, 245, 0));
        assert_eq!(seconds("3:xx"), 0, "not a length");
        assert_eq!(seconds("100000000:00"), 0, "too long to be one");
        let huge = read("x.csv", "Title,Duration (ms)\nSong,99999999999999999\n").unwrap();
        assert_eq!(huge[0].wanted[0].secs, 0);
    }

    #[test]
    fn a_spreadsheets_formula_guard_comes_off() {
        let list = &read("x.csv", "Title,Artist\n'=Song,'-Band\n'Quoted,x\n").unwrap()[0];
        assert_eq!((list.wanted[0].title.as_str(), list.wanted[0].artist.as_str()), ("=Song", "-Band"));
        assert_eq!(list.wanted[1].title, "'Quoted", "an apostrophe of the title's own stays");
    }

    #[test]
    fn spotify_account_data_gives_every_playlist_and_the_liked_songs() {
        let text = r#"{"playlists":[
            {"name":"Chill","items":[{"track":{"trackName":"Teardrop","artistName":"Massive Attack","albumName":"Mezzanine"}},{"track":null,"episode":{}}]},
            {"name":"Empty","items":[]}],
          "tracks":[{"artist":"Air","album":"Moon Safari","track":"La femme d'argent"}]}"#;
        let lists = read("Playlist1.json", text).unwrap();
        assert_eq!(lists.iter().map(|l| l.title.as_str()).collect::<Vec<_>>(), ["Chill", "Liked songs"]);
        assert_eq!(
            lists[0].wanted,
            vec![Wanted { title: "Teardrop".into(), artist: "Massive Attack".into(), album: "Mezzanine".into(), ..Default::default() }]
        );
        assert_eq!(read("broken.json", "{not json"), Err(Unreadable::NoTracks));
    }

    #[test]
    fn m3u_reads_names_lengths_albums_links_and_files() {
        let text = "#EXTM3U\n#PLAYLIST:Mine\n\
                    #EXTINF:320,Daft Punk - One More Time\n#EXTALB:Discovery\nhttps://www.deezer.com/track/3135553\n\
                    /Music/Other/03 - Other Band - Tune.flac\n\
                    07 - Next to the list.mp3\n\
                    #EXTINF:61,Band - Song\n#EXTALB:Record\n\
                    #EXTINF:-1,Last One\n";
        let list = &read("x.m3u8", text).unwrap()[0];
        assert_eq!(list.title, "Mine");
        assert_eq!(list.wanted[0].link.as_deref(), Some("https://www.deezer.com/track/3135553"));
        assert_eq!((list.wanted[0].artist.as_str(), list.wanted[0].album.as_str(), list.wanted[0].secs), ("Daft Punk", "Discovery", 320));
        assert_eq!(list.wanted[1], want("Other Band", "Tune", 0));
        assert_eq!(list.wanted[2], want("", "Next to the list", 0), "a file beside the playlist");
        assert_eq!(list.wanted[3], Wanted { album: "Record".into(), ..want("Band", "Song", 61) }, "no entry line of its own");
        assert_eq!(list.wanted[4], want("", "Last One", 0), "-1: length unknown");
    }

    #[test]
    fn track_numbers_come_off_file_names_but_numbers_in_titles_stay() {
        assert_eq!(without_track_number("01 - Band - Song"), "Band - Song");
        assert_eq!(without_track_number("01. Song"), "Song");
        assert_eq!(without_track_number("07_Song"), "Song");
        assert_eq!(without_track_number("01 Song"), "Song");
        assert_eq!(without_track_number("1979"), "1979");
        assert_eq!(without_track_number("99 Luftballons"), "99 Luftballons");
        assert_eq!(without_track_number("21 Guns"), "21 Guns");
        assert_eq!(without_track_number("03 - "), "03 - ");
        assert_eq!(entry_name("/Music/Smashing Pumpkins/1979.mp3"), "1979");
    }

    #[test]
    fn plain_text_lines_are_artist_and_title_or_links() {
        let list = &read("songs.txt", "Massive Attack - Teardrop\n\nhttps://deezer.com/en/track/42?utm=x\nJust a title\n").unwrap()[0];
        assert_eq!(list.title, "songs");
        assert_eq!(list.wanted[0], want("Massive Attack", "Teardrop", 0));
        assert_eq!(list.wanted[1].link.as_deref(), Some("https://deezer.com/en/track/42?utm=x"));
        assert_eq!(list.wanted[1].name(), "https://deezer.com/en/track/42?utm=x", "a link is its own name");
        assert_eq!(list.wanted[2], want("", "Just a title", 0));
    }

    #[test]
    fn text_is_read_however_it_was_saved() {
        assert_eq!(decode("Café – Ž".as_bytes()), "Café – Ž");
        assert_eq!(decode(b"\xef\xbb\xbfSong"), "Song");
        let utf16le: Vec<u8> = [0xff, 0xfe].into_iter().chain("Ćao".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        assert_eq!(decode(&utf16le), "Ćao");
        let utf16be: Vec<u8> = [0xfe, 0xff].into_iter().chain("Ćao".encode_utf16().flat_map(u16::to_be_bytes)).collect();
        assert_eq!(decode(&utf16be), "Ćao");
        assert_eq!(decode(b"Caf\xe9 \x96 Beyonc\xe9"), "Café – Beyoncé", "Windows-1252");
    }

    #[test]
    fn a_hash_starts_a_name_in_plain_text_but_a_comment_in_m3u() {
        assert_eq!(read("songs.txt", "#9 Dream\n").unwrap()[0].wanted, vec![want("", "#9 Dream", 0)]);
        assert_eq!(read("songs.m3u", "#9 Dream\n"), Err(Unreadable::NoTracks));
    }

    #[test]
    fn files_without_tracks_give_no_playlist() {
        assert_eq!(read("empty.csv", ""), Err(Unreadable::NoTracks));
        assert_eq!(read("header.csv", "Title,Artist\n"), Err(Unreadable::NoTracks));
        assert_eq!(read("comments.m3u", "#EXTM3U\n# nothing here\n"), Err(Unreadable::NoTracks));
        assert_eq!(read("canciones.csv", "Canción,Artista\nHola,Banda\n"), Err(Unreadable::NoTitleColumn), "not read as lines");
        assert!(readable(Path::new("Mix.M3U8")) && !readable(Path::new("cover.jpg")));
    }
}
