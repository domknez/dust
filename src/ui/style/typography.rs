//! Type scale: every text role in the app, so views never pick raw font sizes.

use eframe::egui::{FontFamily, FontId};

/// A text role: size plus weight.
#[derive(Clone, Copy, Debug)]
pub struct Text {
    size: f32,
    strong: bool,
}

impl Text {
    const fn new(size: f32) -> Self {
        Self { size, strong: false }
    }

    /// Semibold variant of the same role.
    pub const fn strong(self) -> Self {
        Self { strong: true, ..self }
    }

    pub fn font(self) -> FontId {
        let family = if self.strong { semibold() } else { FontFamily::Proportional };
        FontId::new(self.size, family)
    }
}

/// Login screen wordmark.
pub const DISPLAY: Text = Text::new(48.0).strong();
/// Collection (playlist, album, ...) title.
pub const HERO: Text = Text::new(40.0).strong();
/// Page titles: Home, Playlists, Search.
pub const PAGE_TITLE: Text = Text::new(30.0).strong();
/// Sidebar wordmark.
pub const WORDMARK: Text = Text::new(28.0).strong();
/// Side panel titles (Queue).
pub const PANEL_TITLE: Text = Text::new(20.0).strong();
/// Home page section headings.
pub const SECTION_TITLE: Text = Text::new(19.0).strong();
/// Taglines and larger supporting text.
pub const LEAD: Text = Text::new(15.0);
/// Track titles, buttons, navigation.
pub const TITLE: Text = Text::new(14.0);
/// Names in lists, cards and menus.
pub const ITEM: Text = Text::new(13.5);
/// Table cells and links.
pub const BODY: Text = Text::new(13.0);
/// Secondary lines (artists) and small controls.
pub const SECONDARY: Text = Text::new(12.5);
/// Card subtitles, hints.
pub const SMALL: Text = Text::new(12.0);
/// Counts, times, metadata.
pub const CAPTION: Text = Text::new(11.5);
/// Uppercase section captions and column headers (already semibold).
pub const OVERLINE: Text = Text::new(11.0).strong();

/// Font family name registered for semibold text.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}
