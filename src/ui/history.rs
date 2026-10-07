//! Back / forward through visited pages, like a browser: ⌘← / ⌘→ (Alt+← / Alt+→ on
//! Windows and Linux), the mouse's side buttons, and the arrows by the search field.

use crate::ui::app::App;
use crate::ui::state::View;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, PointerButton};

/// Pages kept in each direction.
const DEPTH: usize = 50;

/// A page as it was shown: searches remember their query.
#[derive(Clone, PartialEq, Debug)]
pub struct Visit {
    pub view: View,
    pub query: String,
}

#[derive(Default)]
pub struct History {
    back: Vec<Visit>,
    forward: Vec<Visit>,
}

impl History {
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// A new page is about to replace `current` (unless it's the same page).
    pub fn leave(&mut self, current: Visit, next: &Visit) {
        if current == *next {
            return;
        }
        self.back.push(current);
        if self.back.len() > DEPTH {
            self.back.remove(0);
        }
        self.forward.clear();
    }

    pub fn back(&mut self, current: Visit) -> Option<Visit> {
        let previous = self.back.pop()?;
        self.forward.push(current);
        Some(previous)
    }

    pub fn forward(&mut self, current: Visit) -> Option<Visit> {
        let next = self.forward.pop()?;
        self.back.push(current);
        Some(next)
    }
}

const NAV_MODIFIER: Modifiers = if cfg!(target_os = "macos") { Modifiers::COMMAND } else { Modifiers::ALT };

impl App {
    /// The page on screen now, as a history entry.
    pub(in crate::ui) fn current_visit(&self) -> Visit {
        let query = if self.view == View::Search { self.searched.clone() } else { String::new() };
        Visit { view: self.view.clone(), query }
    }

    pub(in crate::ui) fn go_back(&mut self, ctx: &egui::Context) {
        let current = self.current_visit();
        if let Some(visit) = self.history.back(current) {
            self.revisit(ctx, visit);
        }
    }

    pub(in crate::ui) fn go_forward(&mut self, ctx: &egui::Context) {
        let current = self.current_visit();
        if let Some(visit) = self.history.forward(current) {
            self.revisit(ctx, visit);
        }
    }

    fn revisit(&mut self, ctx: &egui::Context, visit: Visit) {
        if visit.view == View::Search {
            self.search = visit.query.clone();
            self.searched = visit.query;
        }
        self.show_view(ctx, visit.view);
    }

    /// Keyboard shortcuts and mouse side buttons (not while typing in a field).
    pub(in crate::ui) fn history_input(&mut self, ctx: &egui::Context) {
        let (mut back, mut forward) =
            ctx.input(|i| (i.pointer.button_pressed(PointerButton::Extra1), i.pointer.button_pressed(PointerButton::Extra2)));
        if !ctx.egui_wants_keyboard_input() {
            back |= ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(NAV_MODIFIER, Key::ArrowLeft)));
            forward |= ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(NAV_MODIFIER, Key::ArrowRight)));
        }
        if back {
            self.go_back(ctx);
        } else if forward {
            self.go_forward(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(view: View) -> Visit {
        Visit { view, query: String::new() }
    }

    #[test]
    fn back_and_forward_like_a_browser() {
        let mut h = History::default();
        h.leave(at(View::Home), &at(View::Loved));
        h.leave(at(View::Loved), &at(View::Playlists));
        assert_eq!(h.back(at(View::Playlists)), Some(at(View::Loved)));
        assert_eq!(h.back(at(View::Loved)), Some(at(View::Home)));
        assert_eq!(h.back(at(View::Home)), None);
        assert_eq!(h.forward(at(View::Home)), Some(at(View::Loved)));
        // A new page drops the forward trail.
        h.leave(at(View::Loved), &at(View::Artists));
        assert!(!h.can_go_forward());
        assert_eq!(h.back(at(View::Artists)), Some(at(View::Loved)));
    }

    #[test]
    fn same_page_is_not_recorded() {
        let mut h = History::default();
        h.leave(at(View::Home), &at(View::Home));
        assert!(!h.can_go_back());
        let search = |q: &str| Visit { view: View::Search, query: q.into() };
        h.leave(search("daft"), &search("justice"));
        assert_eq!(h.back(search("justice")), Some(search("daft")), "a new query is a new page");
    }
}
