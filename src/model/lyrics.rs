//! Song lyrics: time-synced lines when the source has them, plain text otherwise.

/// One synced line, shown from `at_ms` into the track.
#[derive(Clone, Debug, PartialEq)]
pub struct LyricLine {
    pub at_ms: u32,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lyrics {
    /// Time-synced lines, in order; empty when only plain text exists.
    pub synced: Vec<LyricLine>,
    /// The lyrics as plain lines (blank lines separate verses).
    pub plain: Vec<String>,
    pub writers: String,
    pub copyright: String,
}

impl Lyrics {
    /// Lyrics of these lines; None when there's nothing but blank lines (sources
    /// answer an empty text rather than "none").
    pub fn new(synced: Vec<LyricLine>, plain: Vec<String>, writers: String, copyright: String) -> Option<Lyrics> {
        if plain.iter().all(|l| l.trim().is_empty()) {
            return None;
        }
        Some(Lyrics { synced, plain, writers, copyright })
    }

    /// Index of the synced line being sung at `ms`, if one has started.
    pub fn line_at(&self, ms: u32) -> Option<usize> {
        self.synced.partition_point(|l| l.at_ms <= ms).checked_sub(1)
    }
}
