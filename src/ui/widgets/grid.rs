//! A grid of artwork cells (a cover with text under it), laid out row by row and
//! drawn only where it's in view.

use eframe::egui::{Rect, Sense, Ui, UiBuilder, vec2};

/// How wide a [`Grid`]'s columns are.
#[derive(Clone, Copy)]
pub enum GridColumns {
    /// As many cells of this width as fit.
    Fit(f32),
    /// This many columns sharing the width.
    Fixed(usize),
}

/// Square artwork cells with room for text under each.
#[derive(Clone, Copy)]
pub struct Grid {
    pub columns: GridColumns,
    /// Between columns.
    pub gap: f32,
    /// Between rows.
    pub row_gap: f32,
    /// Under the artwork, for the title and subtitle.
    pub text: f32,
}

impl Grid {
    /// The number of columns and the artwork's side in `width`.
    fn layout(&self, width: f32) -> (usize, f32) {
        match self.columns {
            GridColumns::Fit(side) => ((((width + self.gap) / (side + self.gap)).floor() as usize).max(1), side),
            GridColumns::Fixed(n) => (n, (width - self.gap * (n - 1) as f32) / n as f32),
        }
    }

    /// The height `count` cells take in `width`.
    pub fn height(&self, count: usize, width: f32) -> f32 {
        let (columns, side) = self.layout(width);
        count.div_ceil(columns) as f32 * (side + self.text + self.row_gap)
    }

    /// Lay `count` cells out in `area`. `cell` draws cell `k` in a Ui of its own,
    /// whose `max_rect` is the cell (the artwork square at its top).
    pub fn show(&self, ui: &mut Ui, area: Rect, count: usize, mut cell: impl FnMut(&mut Ui, usize)) {
        let (columns, side) = self.layout(area.width());
        for k in 0..count {
            let (column, row) = ((k % columns) as f32, (k / columns) as f32);
            let at = area.min + vec2(column * (side + self.gap), row * (side + self.text + self.row_gap));
            let rect = Rect::from_min_size(at, vec2(side, side + self.text));
            if !ui.is_rect_visible(rect) {
                continue;
            }
            // Its own id, so what a cell remembers (an open menu) stays with it
            // while others scroll in and out of view.
            let id = ui.scope_id().with(("grid-cell", k));
            cell(&mut ui.new_child(UiBuilder::new().scope_id(id).max_rect(rect)), k);
        }
    }

    /// The grid as wide as `ui` has room for, below what's there.
    pub fn add(&self, ui: &mut Ui, count: usize, cell: impl FnMut(&mut Ui, usize)) {
        let width = ui.available_width();
        let (area, _) = ui.allocate_exact_size(vec2(width, self.height(count, width)), Sense::hover());
        self.show(ui, area, count, cell);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_fit_or_share_the_width() {
        let fit = Grid { columns: GridColumns::Fit(100.0), gap: 10.0, row_gap: 20.0, text: 30.0 };
        assert_eq!(fit.layout(319.0), (2, 100.0), "a third cell and its gap don't quite fit");
        assert_eq!(fit.layout(320.0), (3, 100.0));
        assert_eq!(fit.layout(50.0), (1, 100.0), "always at least one column");
        assert_eq!(fit.height(5, 320.0), 2.0 * 150.0);
        let fixed = Grid { columns: GridColumns::Fixed(3), gap: 12.0, row_gap: 12.0, text: 30.0 };
        assert_eq!(fixed.layout(324.0), (3, 100.0));
        assert_eq!(fixed.height(3, 324.0), 142.0);
        assert_eq!(fixed.height(0, 324.0), 0.0);
    }
}
