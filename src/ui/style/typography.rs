//! Type scale: every text role in the app, so views never pick raw font sizes.

use eframe::egui::{FontFamily, FontId};

/// Typefaces: Space Grotesk for text and (bold) titles, JetBrains Mono for labels,
/// times and counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Face {
    Text,
    Display,
    Mono,
}

/// A text role: typeface, size and weight.
#[derive(Clone, Copy, Debug)]
pub struct Text {
    size: f32,
    strong: bool,
    face: Face,
}

impl Text {
    const fn new(size: f32) -> Self {
        Self { size, strong: false, face: Face::Text }
    }

    /// Bold display face, for titles.
    const fn display(size: f32) -> Self {
        Self { size, strong: true, face: Face::Display }
    }

    /// Monospaced labels.
    const fn mono(size: f32) -> Self {
        Self { size, strong: false, face: Face::Mono }
    }

    /// Semibold variant of the same role.
    pub const fn strong(self) -> Self {
        Self { strong: true, ..self }
    }

    pub fn font(self) -> FontId {
        let family = match self.face {
            Face::Display => display(),
            Face::Mono if self.strong => mono_bold(),
            Face::Mono => FontFamily::Monospace,
            Face::Text if self.strong => semibold(),
            Face::Text => FontFamily::Proportional,
        };
        FontId::new(self.size, family)
    }
}

/// Login screen wordmark.
pub const DISPLAY: Text = Text::mono(48.0).strong();
/// Collection (playlist, album, ...) and artist names in page headers.
pub const HERO: Text = Text::display(52.0);
/// Page titles: Home, Playlists, Search.
pub const PAGE_TITLE: Text = Text::display(34.0);
/// Sidebar wordmark.
pub const WORDMARK: Text = Text::mono(17.0).strong();
/// Home page section headings.
pub const SECTION_TITLE: Text = Text::display(19.0);
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
/// Uppercase section captions and column headers.
pub const OVERLINE: Text = Text::mono(10.5);
/// Times, counts, track numbers, shortcut hints and tags.
pub const META: Text = Text::mono(11.0);

/// Font family name registered for titles.
pub fn display() -> FontFamily {
    FontFamily::Name("display".into())
}

/// Font family name registered for bold monospaced text.
pub fn mono_bold() -> FontFamily {
    FontFamily::Name("mono-bold".into())
}

/// Font family name registered for semibold text.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}
