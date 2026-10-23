//! A short message beside the search field for what happened away from the page on
//! screen: a playlist changed, or a like, a quick play or a playlist change that
//! failed. It fades after a few seconds.

use eframe::egui;
use std::time::{Duration, Instant};

/// How long a notice stays up.
const SHOW_FOR: Duration = Duration::from_secs(4);

#[derive(Default)]
pub struct Notice {
    /// The message, whether it's an error, and when it was posted.
    current: Option<(String, bool, Instant)>,
}

impl Notice {
    pub fn info(&mut self, ctx: &egui::Context, message: impl Into<String>) {
        self.post(ctx, message.into(), false);
    }

    pub fn error(&mut self, ctx: &egui::Context, message: impl Into<String>) {
        self.post(ctx, message.into(), true);
    }

    fn post(&mut self, ctx: &egui::Context, message: String, error: bool) {
        self.current = Some((message, error, Instant::now()));
        // Redraw once it's due to go.
        ctx.request_repaint_after(SHOW_FOR);
    }

    /// The message and whether it's an error, while it's fresh.
    pub fn current(&self) -> Option<(&str, bool)> {
        self.current.as_ref().filter(|(_, _, at)| at.elapsed() < SHOW_FOR).map(|(m, error, _)| (m.as_str(), *error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_message_shows() {
        let ctx = egui::Context::default();
        let mut notice = Notice::default();
        assert_eq!(notice.current(), None);
        notice.info(&ctx, "Added 2 tracks to Mix");
        assert_eq!(notice.current(), Some(("Added 2 tracks to Mix", false)));
        notice.error(&ctx, "Couldn't update your likes: offline");
        assert_eq!(notice.current(), Some(("Couldn't update your likes: offline", true)));
    }
}
