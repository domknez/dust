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
    /// Index of the synced line being sung at `ms`, if one has started.
    pub fn line_at(&self, ms: u32) -> Option<usize> {
        self.synced.partition_point(|l| l.at_ms <= ms).checked_sub(1)
    }
}
