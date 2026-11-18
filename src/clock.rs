//! The time now, as the history, the Crate and listen reports keep it.

/// Seconds since 1970 (0 on a clock set before then).
pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
