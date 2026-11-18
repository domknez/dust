//! Reading the head of an RTSP reply or an HTTP request from a device on the
//! network: its first line and its headers, within limits, so a misbehaving
//! one can't make dust read forever or hold any amount in memory.

use std::io::{BufRead, Read};

/// Longest line, and most headers, taken.
pub const MAX_LINE: u64 = 8 * 1024;
pub const MAX_HEADERS: usize = 64;

/// A message's first line ("RTSP/1.0 200 OK", "GET /path HTTP/1.1") and headers.
pub struct Head {
    pub first: String,
    pub headers: Vec<(String, String)>,
}

impl Head {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// The next head from `reader`; Err for a connection that closed or a head over
/// the limits.
pub fn read(reader: &mut impl BufRead) -> Result<Head, String> {
    let mut line = String::new();
    read_line(reader, &mut line)?;
    let first = line.trim_end().to_string();
    let mut headers = Vec::new();
    loop {
        read_line(reader, &mut line)?;
        let header = line.trim_end();
        if header.is_empty() {
            return Ok(Head { first, headers });
        }
        if headers.len() == MAX_HEADERS {
            return Err(format!("over {MAX_HEADERS} headers"));
        }
        if let Some((k, v)) = header.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
}

/// One whole line into `line`.
fn read_line(reader: &mut impl BufRead, line: &mut String) -> Result<(), String> {
    line.clear();
    reader.take(MAX_LINE).read_line(line).map_err(|e| e.to_string())?;
    if line.ends_with('\n') { Ok(()) } else { Err(format!("line cut off or over {MAX_LINE} bytes")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(text: &[u8]) -> Result<Head, String> {
        read(&mut std::io::Cursor::new(text.to_vec()))
    }

    #[test]
    fn heads_are_read_within_bounds() {
        let h = head(b"GET /x HTTP/1.1\r\nActive-Remote: 12\r\nHost: h\r\n\r\nbody").unwrap();
        assert_eq!((h.first.as_str(), h.header("active-remote")), ("GET /x HTTP/1.1", Some("12")));
        let long = [b"GET / HTTP/1.1\r\nX: ".as_slice(), &vec![b'a'; MAX_LINE as usize], b"\r\n\r\n"].concat();
        assert!(head(&long).is_err(), "no endless line");
        let many = [b"GET / HTTP/1.1\r\n".as_slice(), &b"X: y\r\n".repeat(MAX_HEADERS + 1), b"\r\n"].concat();
        assert!(head(&many).is_err(), "no endless headers");
        assert!(head(b"GET / HTTP/1.1\r\nHost: h").is_err(), "cut off");
        assert!(head(b"").is_err(), "closed");
    }
}
