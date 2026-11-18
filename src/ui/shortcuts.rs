//! The keyboard shortcuts, each defined once, and how they read where they're
//! shown: ⇧⌘S on macOS, Ctrl+Shift+S on Windows and Linux.

use eframe::egui::{Key, KeyboardShortcut, ModifierNames, Modifiers};

const MAC: bool = cfg!(target_os = "macos");

const COMMAND_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);

/// The command palette.
pub const PALETTE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::K);
/// Settings: ⌘, on macOS, Ctrl+, elsewhere (as in most apps there).
pub const SETTINGS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma);
/// The search field.
pub const SEARCH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::F);
/// The track playing into the Crate.
pub const SAVE_TO_CRATE: KeyboardShortcut = KeyboardShortcut::new(COMMAND_SHIFT, Key::S);
/// The mini player, or back to the full window.
pub const MINI_PLAYER: KeyboardShortcut = KeyboardShortcut::new(COMMAND_SHIFT, Key::M);
/// The track playing onto the playlist used last.
pub const ADD_TO_RECENT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::D);
/// Back and forward through pages: ⌘← / ⌘→, Alt+← / Alt+→ elsewhere.
const NAV: Modifiers = if MAC { Modifiers::COMMAND } else { Modifiers::ALT };
pub const BACK: KeyboardShortcut = KeyboardShortcut::new(NAV, Key::ArrowLeft);
pub const FORWARD: KeyboardShortcut = KeyboardShortcut::new(NAV, Key::ArrowRight);

/// A page by its number key (⌘1 is Home).
pub const fn page(key: Key) -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, key)
}

/// Windows' and Linux's modifier names, with keys as symbols (Ctrl+,).
const NAMES: ModifierNames<'static> = ModifierNames { is_short: true, ..ModifierNames::NAMES };

/// How `shortcut` reads on this computer: "⇧⌘S", "Ctrl+Shift+S".
pub fn label(shortcut: &KeyboardShortcut) -> String {
    let key = match shortcut.logical_key {
        Key::ArrowLeft => "←",
        Key::ArrowRight => "→",
        key => key.symbol_or_name(),
    };
    let names = if MAC { &ModifierNames::SYMBOLS } else { &NAMES };
    let modifiers = names.format(&shortcut.modifiers, MAC);
    if modifiers.is_empty() { key.to_string() } else { format!("{modifiers}{}{key}", names.concat) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_read_like_the_platform() {
        let read: Vec<String> = [SAVE_TO_CRATE, SETTINGS, BACK, page(Key::Num3)].iter().map(label).collect();
        let expected = if MAC { ["⇧⌘S", "⌘,", "⌘←", "⌘3"] } else { ["Ctrl+Shift+S", "Ctrl+,", "Alt+←", "Ctrl+3"] };
        assert_eq!(read, expected);
    }
}
