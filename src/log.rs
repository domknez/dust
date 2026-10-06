//! Minimal leveled logging to stderr, configured with `DUST_LOG`
//! (`error`, `warn`, `info`, `debug`, `trace`; default `warn`).
//! `DUST_DEBUG=1` is kept as a shorthand for `debug`.
//!
//! Use the macros: `log_error!`, `log_warn!`, `log_info!`, `log_debug!`, `log_trace!`.

use std::sync::OnceLock;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Level {
    fn parse(s: &str) -> Option<Level> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "error" => Level::Error,
            "warn" | "warning" => Level::Warn,
            "info" => Level::Info,
            "debug" => Level::Debug,
            "trace" => Level::Trace,
            _ => return None,
        })
    }
}

fn max_level() -> Level {
    static LEVEL: OnceLock<Level> = OnceLock::new();
    *LEVEL.get_or_init(|| {
        let from_env = std::env::var("DUST_LOG").ok().and_then(|v| Level::parse(&v));
        let legacy_debug = std::env::var_os("DUST_DEBUG").is_some().then_some(Level::Debug);
        from_env.or(legacy_debug).unwrap_or(Level::Warn)
    })
}

pub fn enabled(level: Level) -> bool {
    level <= max_level()
}

#[doc(hidden)]
pub fn write(level: Level, args: std::fmt::Arguments) {
    // Seconds since the first log line: enough to line up events across threads.
    static START: OnceLock<Instant> = OnceLock::new();
    let t = START.get_or_init(Instant::now).elapsed().as_secs_f64();
    eprintln!("dust {t:9.3} [{}]: {args}", format!("{level:?}").to_lowercase());
}

#[macro_export]
macro_rules! log_at {
    ($level:expr, $($arg:tt)*) => {
        if $crate::log::enabled($level) {
            $crate::log::write($level, format_args!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_error { ($($arg:tt)*) => { $crate::log_at!($crate::log::Level::Error, $($arg)*) }; }
#[macro_export]
macro_rules! log_warn { ($($arg:tt)*) => { $crate::log_at!($crate::log::Level::Warn, $($arg)*) }; }
#[macro_export]
macro_rules! log_info { ($($arg:tt)*) => { $crate::log_at!($crate::log::Level::Info, $($arg)*) }; }
#[macro_export]
macro_rules! log_debug { ($($arg:tt)*) => { $crate::log_at!($crate::log::Level::Debug, $($arg)*) }; }
#[macro_export]
macro_rules! log_trace { ($($arg:tt)*) => { $crate::log_at!($crate::log::Level::Trace, $($arg)*) }; }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_parse_and_order() {
        assert_eq!(Level::parse(" DEBUG "), Some(Level::Debug));
        assert_eq!(Level::parse("nope"), None);
        assert!(Level::Error < Level::Warn && Level::Debug < Level::Trace);
    }
}
