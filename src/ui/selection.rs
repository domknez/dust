//! [`Selection`]: the rows picked in the track list on screen, to act on together
//! from the right-click menu.

use eframe::egui::Modifiers;
use std::collections::BTreeSet;

/// Rows of a track list, by index. A click picks one row, ⌘-click (Ctrl-click) adds
/// or drops one, Shift-click picks the range from the last row clicked.
#[derive(Default, Debug)]
pub struct Selection {
    rows: BTreeSet<usize>,
    /// Where a Shift-click range starts.
    anchor: Option<usize>,
}

impl Selection {
    pub fn contains(&self, row: usize) -> bool {
        self.rows.contains(&row)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The picked rows, top to bottom.
    pub fn rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.rows.iter().copied()
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.anchor = None;
    }

    /// Just `row`.
    pub fn only(&mut self, row: usize) {
        self.rows = BTreeSet::from([row]);
        self.anchor = Some(row);
    }

    /// Every row of a list of `count`.
    pub fn all(&mut self, count: usize) {
        self.rows = (0..count).collect();
        self.anchor = None;
    }

    /// A click on `row` with `modifiers` held.
    pub fn click(&mut self, row: usize, modifiers: Modifiers) {
        if modifiers.shift
            && let Some(anchor) = self.anchor
        {
            self.rows = (anchor.min(row)..=anchor.max(row)).collect();
        } else if modifiers.command {
            if !self.rows.remove(&row) {
                self.rows.insert(row);
            }
            self.anchor = Some(row);
        } else {
            self.only(row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(selection: &Selection) -> Vec<usize> {
        selection.rows().collect()
    }

    #[test]
    fn a_click_picks_one_row() {
        let mut s = Selection::default();
        s.click(3, Modifiers::NONE);
        s.click(5, Modifiers::NONE);
        assert_eq!(rows(&s), [5]);
    }

    #[test]
    fn command_click_adds_and_drops() {
        let mut s = Selection::default();
        s.click(1, Modifiers::NONE);
        s.click(4, Modifiers::COMMAND);
        s.click(2, Modifiers::COMMAND);
        assert_eq!(rows(&s), [1, 2, 4]);
        s.click(4, Modifiers::COMMAND);
        assert_eq!(rows(&s), [1, 2]);
    }

    #[test]
    fn shift_click_picks_a_range_from_the_last_click() {
        let mut s = Selection::default();
        s.click(6, Modifiers::NONE);
        s.click(3, Modifiers::SHIFT);
        assert_eq!(rows(&s), [3, 4, 5, 6]);
        s.click(8, Modifiers::SHIFT);
        assert_eq!(rows(&s), [6, 7, 8], "still from the row clicked first");
        let mut fresh = Selection::default();
        fresh.click(2, Modifiers::SHIFT);
        assert_eq!(rows(&fresh), [2], "with nothing to start from, just the row");
    }

    #[test]
    fn all_and_clear() {
        let mut s = Selection::default();
        s.all(3);
        assert_eq!(rows(&s), [0, 1, 2]);
        s.clear();
        assert!(s.is_empty());
        s.click(1, Modifiers::SHIFT);
        assert_eq!(rows(&s), [1], "clearing forgets where a range starts");
    }
}
