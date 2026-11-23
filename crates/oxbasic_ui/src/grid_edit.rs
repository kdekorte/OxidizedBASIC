//! Keyboard rules of a DataGrid, with no window: where the arrow keys, Tab,
//! Enter and the paging keys move the current cell, and the inline edit of a
//! cell (its text, caret, and how it ends). The window and the program
//! host both use this, so a key means the same thing in each.

use crate::grid::GridSpec;

/// How a key moves the current cell: `Some((row, col))` is where it goes
/// (already clamped to the scrolling cells, never into a header), `None` for
/// a key that doesn't move it. `page` is how many rows a page is.
pub fn move_by_key(g: &GridSpec, key: &str, shift: bool, page: usize) -> Option<(usize, usize)> {
    if g.rows == 0 || g.cols == 0 {
        return None;
    }
    let (fr, fc) = (g.fixed_rows.min(g.rows - 1), g.fixed_cols.min(g.cols - 1));
    let (r, c) = (g.row, g.col);
    let (last_r, last_c) = (g.rows - 1, g.cols - 1);
    let to = match key {
        "up" => (r.saturating_sub(1), c),
        "down" => ((r + 1).min(last_r), c),
        "left" => (r, c.saturating_sub(1)),
        "right" => (r, (c + 1).min(last_c)),
        "pageup" => (r.saturating_sub(page.max(1)), c),
        "pagedown" => ((r + page.max(1)).min(last_r), c),
        "home" => (r, fc),
        "end" => (r, last_c),
        // Tab goes right and wraps to the next row; Shift-Tab the reverse
        "tab" if !shift => {
            if c < last_c {
                (r, c + 1)
            } else if r < last_r {
                (r + 1, fc)
            } else {
                (r, c)
            }
        }
        "tab" => {
            if c > fc {
                (r, c - 1)
            } else if r > fr {
                (r - 1, last_c)
            } else {
                (r, c)
            }
        }
        _ => return None,
    };
    // a header is never the current cell: stay on the first scrolling one
    Some((to.0.max(fr), to.1.max(fc)))
}

/// An inline edit of one cell.
#[derive(Debug, Clone, PartialEq)]
pub struct CellEdit {
    pub row: usize,
    pub col: usize,
    /// What the cell held when the edit began (what Esc restores).
    pub original: String,
    pub text: String,
    /// The caret, in characters.
    pub caret: usize,
}

/// How an edit ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditEnd {
    /// Enter, Tab, or a click elsewhere: keep the text.
    Commit,
    /// Esc: put the original text back.
    Cancel,
}

impl CellEdit {
    /// Begin editing `(row, col)`. `replace` starts from an empty cell with
    /// the caret at its start (typing over it); otherwise the caret is at
    /// the end of the cell's text (F2, or a double click).
    pub fn begin(g: &GridSpec, row: usize, col: usize, replace: bool) -> Option<CellEdit> {
        if !g.can_edit(row, col) {
            return None;
        }
        let original = g.cell(row, col)?.to_string();
        let text = if replace { String::new() } else { original.clone() };
        let caret = text.chars().count();
        Some(CellEdit { row, col, original, text, caret })
    }

    /// Insert `s` at the caret.
    pub fn insert(&mut self, s: &str) {
        let chars: Vec<char> = self.text.chars().collect();
        let mut out: String = chars[..self.caret].iter().collect();
        out.push_str(s);
        out.extend(chars[self.caret..].iter());
        self.text = out;
        self.caret += s.chars().count();
    }

    /// Delete the character before the caret. `false` at the start.
    pub fn backspace(&mut self) -> bool {
        if self.caret == 0 {
            return false;
        }
        let mut chars: Vec<char> = self.text.chars().collect();
        chars.remove(self.caret - 1);
        self.text = chars.into_iter().collect();
        self.caret -= 1;
        true
    }

    /// Delete the character after the caret. `false` at the end.
    pub fn delete(&mut self) -> bool {
        let mut chars: Vec<char> = self.text.chars().collect();
        if self.caret >= chars.len() {
            return false;
        }
        chars.remove(self.caret);
        self.text = chars.into_iter().collect();
        true
    }

    /// Move the caret by a key (`left`, `right`, `home`, `end`). `false` when
    /// the key isn't a caret key or nothing moves.
    pub fn move_caret(&mut self, key: &str) -> bool {
        let len = self.text.chars().count();
        let to = match key {
            "left" => self.caret.saturating_sub(1),
            "right" => (self.caret + 1).min(len),
            "home" => 0,
            "end" => len,
            _ => return false,
        };
        let moved = to != self.caret;
        self.caret = to;
        moved
    }

    /// The text the cell should hold when the edit ends this way.
    pub fn result(&self, end: EditEnd) -> &str {
        match end {
            EditEnd::Commit => &self.text,
            EditEnd::Cancel => &self.original,
        }
    }
}
