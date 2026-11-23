//! The DataGrid control's data: a table of text cells with fixed header
//! rows and columns, a current cell, and an `Editable` switch. Render-
//! agnostic, like the rest of the form model.

use serde::{Deserialize, Serialize};

/// Width in pixels a new column gets.
pub const DEFAULT_COL_WIDTH: f32 = 80.0;
/// Height in pixels a new row gets.
pub const DEFAULT_ROW_HEIGHT: f32 = 22.0;

/// A DataGrid's contents and settings. `Default` is a new grid as dropped
/// from the toolbox: 5 x 5 cells with one fixed header row and column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GridSpec {
    /// `Rows` and `Cols`, fixed ones included.
    pub rows: usize,
    pub cols: usize,
    /// `FixedRows` and `FixedCols`: the header cells, which are never edited.
    pub fixed_rows: usize,
    pub fixed_cols: usize,
    /// `Editable`: whether the user can change a cell. The program can
    /// always set `TextMatrix`.
    pub editable: bool,
    /// The current cell (`Row`, `Col`).
    pub row: usize,
    pub col: usize,
    /// `rows` lists of `cols` texts.
    pub cells: Vec<Vec<String>>,
    /// `ColWidth(c)` and `RowHeight(r)`, in pixels.
    pub col_widths: Vec<f32>,
    pub row_heights: Vec<f32>,
    /// `TopRow` and `LeftCol`: the first scrolling row and column in view
    /// (the fixed ones always show).
    pub top_row: usize,
    pub left_col: usize,
    /// `ScrollBars`: 0 none, 1 horizontal, 2 vertical, 3 both.
    pub scroll_bars: u8,
}

impl Default for GridSpec {
    fn default() -> GridSpec {
        GridSpec::new(5, 5)
    }
}

impl GridSpec {
    /// An empty grid of `rows` x `cols` with one fixed row and column (fewer
    /// when the grid is smaller), the current cell the first editable one.
    pub fn new(rows: usize, cols: usize) -> GridSpec {
        let mut g = GridSpec {
            rows: 0,
            cols: 0,
            fixed_rows: 1,
            fixed_cols: 1,
            editable: false,
            row: 0,
            col: 0,
            cells: Vec::new(),
            col_widths: Vec::new(),
            row_heights: Vec::new(),
            top_row: 0,
            left_col: 0,
            scroll_bars: 3,
        };
        g.resize(rows, cols);
        g.row = g.fixed_rows.min(g.rows.saturating_sub(1));
        g.col = g.fixed_cols.min(g.cols.saturating_sub(1));
        g
    }

    /// Set the size, keeping the cells (and widths and heights) that still
    /// fit. The fixed counts and the current cell are pulled back inside.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.cells.resize_with(rows, Vec::new);
        for r in &mut self.cells {
            r.resize(cols, String::new());
        }
        self.col_widths.resize(cols, DEFAULT_COL_WIDTH);
        self.row_heights.resize(rows, DEFAULT_ROW_HEIGHT);
        self.rows = rows;
        self.cols = cols;
        self.fixed_rows = self.fixed_rows.min(rows);
        self.fixed_cols = self.fixed_cols.min(cols);
        self.row = self.row.min(rows.saturating_sub(1));
        self.col = self.col.min(cols.saturating_sub(1));
        self.clamp_view();
    }

    /// Pull `TopRow` and `LeftCol` back to a scrolling row and column.
    pub fn clamp_view(&mut self) {
        let last_row = self.rows.saturating_sub(1).max(self.fixed_rows);
        let last_col = self.cols.saturating_sub(1).max(self.fixed_cols);
        self.top_row = self.top_row.max(self.fixed_rows).min(last_row);
        self.left_col = self.left_col.max(self.fixed_cols).min(last_col);
    }

    /// Whether `(r, c)` is a cell of the grid.
    pub fn contains(&self, r: usize, c: usize) -> bool {
        r < self.rows && c < self.cols
    }

    /// Whether `(r, c)` is a header cell (in a fixed row or column).
    pub fn is_fixed(&self, r: usize, c: usize) -> bool {
        r < self.fixed_rows || c < self.fixed_cols
    }

    /// Whether the user may edit `(r, c)`: the grid is editable and the cell
    /// is not a header.
    pub fn can_edit(&self, r: usize, c: usize) -> bool {
        self.editable && self.contains(r, c) && !self.is_fixed(r, c)
    }

    /// The text of a cell, or `None` outside the grid.
    pub fn cell(&self, r: usize, c: usize) -> Option<&str> {
        self.cells.get(r)?.get(c).map(String::as_str)
    }

    /// Set a cell's text. `false` (and no change) outside the grid.
    pub fn set_cell(&mut self, r: usize, c: usize, text: &str) -> bool {
        match self.cells.get_mut(r).and_then(|row| row.get_mut(c)) {
            Some(cell) => {
                *cell = text.to_string();
                true
            }
            None => false,
        }
    }

    /// Make `(r, c)` the current cell. `false` (and no change) outside.
    pub fn select(&mut self, r: usize, c: usize) -> bool {
        if !self.contains(r, c) {
            return false;
        }
        self.row = r;
        self.col = c;
        true
    }

    /// `AddItem`: append a row whose cells are the tab-separated parts of
    /// `item` (missing ones empty, extra ones dropped).
    pub fn add_item(&mut self, item: &str) {
        let (rows, cols) = (self.rows + 1, self.cols);
        self.resize(rows, cols);
        for (c, part) in item.split('\t').take(cols).enumerate() {
            self.cells[rows - 1][c] = part.to_string();
        }
    }

    /// `RemoveItem`: delete row `r` (a fixed row can't be removed). `false`
    /// when there is no such row.
    pub fn remove_item(&mut self, r: usize) -> bool {
        if r >= self.rows || r < self.fixed_rows {
            return false;
        }
        self.cells.remove(r);
        self.row_heights.remove(r);
        self.rows -= 1;
        self.row = self.row.min(self.rows.saturating_sub(1));
        self.clamp_view();
        true
    }

    /// `Clear`: empty every cell, keeping the size and settings.
    pub fn clear(&mut self) {
        for row in &mut self.cells {
            for cell in row {
                cell.clear();
            }
        }
    }

    /// `FixedRows`: no more than `Rows`.
    pub fn set_fixed_rows(&mut self, n: usize) -> bool {
        if n > self.rows {
            return false;
        }
        self.fixed_rows = n;
        self.clamp_view();
        true
    }

    /// `FixedCols`: no more than `Cols`.
    pub fn set_fixed_cols(&mut self, n: usize) -> bool {
        if n > self.cols {
            return false;
        }
        self.fixed_cols = n;
        self.clamp_view();
        true
    }

    /// `ColWidth(c)`, `None` outside the grid.
    pub fn col_width(&self, c: usize) -> Option<f32> {
        self.col_widths.get(c).copied()
    }

    /// `RowHeight(r)`, `None` outside the grid.
    pub fn row_height(&self, r: usize) -> Option<f32> {
        self.row_heights.get(r).copied()
    }

    /// Set `ColWidth(c)` (at least 0). `false` outside the grid.
    pub fn set_col_width(&mut self, c: usize, w: f32) -> bool {
        match self.col_widths.get_mut(c) {
            Some(x) => {
                *x = w.max(0.0);
                true
            }
            None => false,
        }
    }

    /// Set `RowHeight(r)` (at least 0). `false` outside the grid.
    pub fn set_row_height(&mut self, r: usize, h: f32) -> bool {
        match self.row_heights.get_mut(r) {
            Some(x) => {
                *x = h.max(0.0);
                true
            }
            None => false,
        }
    }
}
