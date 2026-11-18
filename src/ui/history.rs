//! Back / forward through visited pages, like a browser: ⌘← / ⌘→ (Alt+← / Alt+→ on
//! Windows and Linux), the mouse's side buttons, and the arrows by the search field.

use crate::ui::action::{Action, Actions};
use crate::ui::app::App;
use crate::ui::state::View;
use eframe::egui::{self, PointerButton};

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

    /// The page before `current`, skipping (and forgetting) ones not `allowed` any
    /// more: a part turned off in Settings since.
    pub fn back(&mut self, current: Visit, allowed: impl Fn(&Visit) -> bool) -> Option<Visit> {
        let previous = pop_allowed(&mut self.back, allowed)?;
        self.forward.push(current);
        Some(previous)
    }

    /// The page after `current`, skipping ones not `allowed`, as [`History::back`].
    pub fn forward(&mut self, current: Visit, allowed: impl Fn(&Visit) -> bool) -> Option<Visit> {
        let next = pop_allowed(&mut self.forward, allowed)?;
        self.back.push(current);
        Some(next)
    }
}

fn pop_allowed(visits: &mut Vec<Visit>, allowed: impl Fn(&Visit) -> bool) -> Option<Visit> {
    while let Some(visit) = visits.pop() {
        if allowed(&visit) {
            return Some(visit);
        }
    }
    None
}

impl App {
    /// The page on screen now, as a history entry.
    pub(in crate::ui) fn current_visit(&self) -> Visit {
        let query = if self.browse.view == View::Search { self.browse.searched.clone() } else { String::new() };
        Visit { view: self.browse.view.clone(), query }
    }

    pub(in crate::ui) fn go_back(&mut self) {
        let current = self.current_visit();
        let mut history = std::mem::take(&mut self.browse.history);
        let visit = history.back(current, |v| self.view_allowed(&v.view));
        self.browse.history = history;
        if let Some(visit) = visit {
            self.revisit(visit);
        }
    }

    pub(in crate::ui) fn go_forward(&mut self) {
        let current = self.current_visit();
        let mut history = std::mem::take(&mut self.browse.history);
        let visit = history.forward(current, |v| self.view_allowed(&v.view));
        self.browse.history = history;
        if let Some(visit) = visit {
            self.revisit(visit);
        }
    }

    fn revisit(&mut self, visit: Visit) {
        if visit.view == View::Search {
            self.browse.search = visit.query.clone();
            self.browse.searched = visit.query;
        }
        self.show_view(visit.view);
    }

    /// Keyboard shortcuts and mouse side buttons (not while typing in a field).
    pub(in crate::ui) fn history_input(&self, ctx: &egui::Context, acts: &mut Actions) {
        let (mut back, mut forward) =
            ctx.input(|i| (i.pointer.button_pressed(PointerButton::Extra1), i.pointer.button_pressed(PointerButton::Extra2)));
        if !ctx.egui_wants_keyboard_input() {
            back |= ctx.input_mut(|i| i.consume_shortcut(&crate::ui::shortcuts::BACK));
            forward |= ctx.input_mut(|i| i.consume_shortcut(&crate::ui::shortcuts::FORWARD));
        }
        if back {
            acts.push(Action::Back);
        } else if forward {
            acts.push(Action::Forward);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(view: View) -> Visit {
        Visit { view, query: String::new() }
    }

    fn any(_: &Visit) -> bool {
        true
    }

    #[test]
    fn pages_turned_off_since_are_skipped() {
        let mut h = History::default();
        h.leave(at(View::Home), &at(View::Crate));
        h.leave(at(View::Crate), &at(View::Loved));
        let no_crate = |v: &Visit| v.view != View::Crate;
        assert_eq!(h.back(at(View::Loved), no_crate), Some(at(View::Home)), "past the Crate");
        assert_eq!(h.forward(at(View::Home), no_crate), Some(at(View::Loved)));
        assert_eq!(h.back(at(View::Loved), any), Some(at(View::Home)), "and it's forgotten");
    }

    #[test]
    fn back_and_forward_like_a_browser() {
        let mut h = History::default();
        h.leave(at(View::Home), &at(View::Loved));
        h.leave(at(View::Loved), &at(View::Playlists));
        assert_eq!(h.back(at(View::Playlists), any), Some(at(View::Loved)));
        assert_eq!(h.back(at(View::Loved), any), Some(at(View::Home)));
        assert_eq!(h.back(at(View::Home), any), None);
        assert_eq!(h.forward(at(View::Home), any), Some(at(View::Loved)));
        // A new page drops the forward trail.
        h.leave(at(View::Loved), &at(View::Artists));
        assert!(!h.can_go_forward());
        assert_eq!(h.back(at(View::Artists), any), Some(at(View::Loved)));
    }

    #[test]
    fn same_page_is_not_recorded() {
        let mut h = History::default();
        h.leave(at(View::Home), &at(View::Home));
        assert!(!h.can_go_back());
        let search = |q: &str| Visit { view: View::Search, query: q.into() };
        h.leave(search("daft"), &search("justice"));
        assert_eq!(h.back(search("justice"), any), Some(search("daft")), "a new query is a new page");
    }
}
