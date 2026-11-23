//! A single-line text box's editing: the caret is a character index into the
//! text, and the functions move it and edit at it. The find and replace boxes
//! use these; they have no GUI in them, so they are tested headless.

/// One key's effect on a single-line text and its caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Home,
    End,
    /// ⌥← and ⌥→: by word.
    WordLeft,
    WordRight,
}

/// The caret after moving it, clamped to the text.
pub fn moved(text: &str, caret: usize, m: Move) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let caret = caret.min(len);
    match m {
        Move::Left => caret.saturating_sub(1),
        Move::Right => (caret + 1).min(len),
        Move::Home => 0,
        Move::End => len,
        Move::WordLeft => {
            let mut i = caret;
            while i > 0 && !is_word(chars[i - 1]) {
                i -= 1;
            }
            while i > 0 && is_word(chars[i - 1]) {
                i -= 1;
            }
            i
        }
        Move::WordRight => {
            let mut i = caret;
            while i < len && !is_word(chars[i]) {
                i += 1;
            }
            while i < len && is_word(chars[i]) {
                i += 1;
            }
            i
        }
    }
}

/// Whether `c` is part of a word (letters, digits and `_`).
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Insert `s` at the caret; returns the new caret (after the insert).
pub fn insert(text: &mut String, caret: usize, s: &str) -> usize {
    let at = byte_index(text, caret);
    text.insert_str(at, s);
    caret.min(text.chars().count()) + s.chars().count()
}

/// Delete the character before the caret (Backspace); returns the new caret.
pub fn backspace(text: &mut String, caret: usize) -> usize {
    if caret == 0 {
        return 0;
    }
    let start = byte_index(text, caret - 1);
    let end = byte_index(text, caret);
    text.replace_range(start..end, "");
    caret - 1
}

/// Delete the character after the caret (Delete); the caret stays.
pub fn delete_forward(text: &mut String, caret: usize) -> usize {
    if caret >= text.chars().count() {
        return caret;
    }
    let start = byte_index(text, caret);
    let end = byte_index(text, caret + 1);
    text.replace_range(start..end, "");
    caret
}

/// The selected range of a single-line text: `anchor` is where the selection
/// began, `caret` where it is now (equal when nothing is selected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sel {
    pub anchor: usize,
    pub caret: usize,
}

impl Sel {
    /// Nothing selected, the caret at `at`.
    pub fn at(at: usize) -> Sel {
        Sel { anchor: at, caret: at }
    }

    /// The selected characters as a range `(from, to)`, `from <= to`.
    pub fn range(&self) -> (usize, usize) {
        (self.anchor.min(self.caret), self.anchor.max(self.caret))
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.caret
    }
}

/// Delete the selected characters (if any) and put the caret where they were.
/// Returns the new selection (empty).
pub fn delete_selection(text: &mut String, sel: Sel) -> Sel {
    if sel.is_empty() {
        return sel;
    }
    let (from, to) = sel.range();
    let start = byte_index(text, from);
    let end = byte_index(text, to);
    text.replace_range(start..end, "");
    Sel::at(from)
}

/// Type `s` over the selection (or at the caret): returns the new selection.
pub fn insert_over(text: &mut String, sel: Sel, s: &str) -> Sel {
    let sel = delete_selection(text, sel);
    let at = insert(text, sel.caret, s);
    Sel::at(at)
}

/// Backspace: deletes the selection, or the character before the caret.
pub fn backspace_sel(text: &mut String, sel: Sel) -> Sel {
    if !sel.is_empty() {
        return delete_selection(text, sel);
    }
    Sel::at(backspace(text, sel.caret))
}

/// Delete: deletes the selection, or the character after the caret.
pub fn delete_sel(text: &mut String, sel: Sel) -> Sel {
    if !sel.is_empty() {
        return delete_selection(text, sel);
    }
    Sel::at(delete_forward(text, sel.caret))
}

/// Move the caret by `m`. With `extend` (Shift) the selection grows from its
/// anchor; without it the selection collapses at the new caret.
pub fn move_sel(text: &str, sel: Sel, m: Move, extend: bool) -> Sel {
    let caret = moved(text, sel.caret, m);
    if extend {
        Sel { anchor: sel.anchor, caret }
    } else {
        Sel::at(caret)
    }
}

/// Select everything (⌘A): the caret at the end, the anchor at the start.
pub fn select_all(text: &str) -> Sel {
    Sel { anchor: 0, caret: text.chars().count() }
}

/// The horizontal scroll (in characters) that keeps `caret` inside a field
/// showing `visible` characters, moving the current `scroll` the least.
pub fn scrolled_to_caret(scroll: usize, caret: usize, visible: usize) -> usize {
    let visible = visible.max(1);
    if caret < scroll {
        caret
    } else if caret >= scroll + visible {
        caret + 1 - visible
    } else {
        scroll
    }
}


/// The byte offset of character `i` (the text's length if `i` is past it).
fn byte_index(text: &str, i: usize) -> usize {
    text.char_indices()
        .nth(i)
        .map(|(b, _)| b)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_home_and_end_move_the_caret() {
        assert_eq!(moved("abc", 1, Move::Left), 0);
        assert_eq!(moved("abc", 0, Move::Left), 0, "not past the start");
        assert_eq!(moved("abc", 1, Move::Right), 2);
        assert_eq!(moved("abc", 3, Move::Right), 3, "not past the end");
        assert_eq!(moved("abc", 2, Move::Home), 0);
        assert_eq!(moved("abc", 0, Move::End), 3);
    }

    #[test]
    fn option_arrows_move_by_word() {
        let t = "Label1.Caption now";
        assert_eq!(moved(t, 18, Move::WordLeft), 15, "back over now");
        assert_eq!(
            moved(t, 15, Move::WordLeft),
            7,
            "back over Caption to its start"
        );
        assert_eq!(
            moved(t, 7, Move::WordLeft),
            0,
            "then over Label1 and the dot"
        );
        assert_eq!(moved(t, 0, Move::WordRight), 6, "over Label1");
        assert_eq!(moved(t, 6, Move::WordRight), 14, "over .Caption");
    }

    #[test]
    fn typing_goes_in_at_the_caret() {
        let mut t = String::from("ac");
        let c = insert(&mut t, 1, "b");
        assert_eq!(t, "abc");
        assert_eq!(c, 2);
        let c = insert(&mut t, 0, "xy");
        assert_eq!(t, "xyabc");
        assert_eq!(c, 2);
    }

    #[test]
    fn backspace_and_delete_work_either_side_of_the_caret() {
        let mut t = String::from("abcd");
        assert_eq!(backspace(&mut t, 2), 1);
        assert_eq!(t, "acd", "removes the b before the caret");
        assert_eq!(delete_forward(&mut t, 1), 1);
        assert_eq!(t, "ad", "removes the c after the caret");
        assert_eq!(backspace(&mut t, 0), 0, "nothing before the start");
        assert_eq!(delete_forward(&mut t, 2), 2, "nothing after the end");
    }

    #[test]
    fn characters_not_bytes_count() {
        let mut t = String::from("é€x");
        assert_eq!(moved(&t, 0, Move::Right), 1);
        assert_eq!(backspace(&mut t, 2), 1, "removes the €");
        assert_eq!(t, "éx");
    }

#[cfg(test)]
mod selection_tests {
    use super::*;

    #[test]
    fn typing_replaces_the_selection() {
        let mut t = String::from("C:/icons/app.png");
        let after = insert_over(&mut t, Sel { anchor: 3, caret: 9 }, "X");
        assert_eq!(t, "C:/Xapp.png");
        assert_eq!(after, Sel::at(4));
    }

    #[test]
    fn backspace_and_delete_remove_the_selection_first() {
        let mut t = String::from("abcdef");
        let s = backspace_sel(&mut t, Sel { anchor: 1, caret: 4 });
        assert_eq!((t.as_str(), s), ("aef", Sel::at(1)));
        let mut u = String::from("abc");
        assert_eq!(backspace_sel(&mut u, Sel::at(2)), Sel::at(1));
        assert_eq!(u, "ac");
        let mut v = String::from("abc");
        assert_eq!(delete_sel(&mut v, Sel::at(0)), Sel::at(0));
        assert_eq!(v, "bc");
    }

    #[test]
    fn shift_moves_extend_and_plain_moves_collapse() {
        let t = "hello";
        let ext = move_sel(t, Sel::at(2), Move::Right, true);
        assert_eq!(ext, Sel { anchor: 2, caret: 3 });
        assert_eq!(ext.range(), (2, 3));
        assert_eq!(move_sel(t, ext, Move::Left, false), Sel::at(2));
    }

    #[test]
    fn select_all_covers_the_text() {
        assert_eq!(select_all("abc").range(), (0, 3));
    }

    #[test]
    fn the_scroll_follows_the_caret_and_stays_put_inside_the_view() {
        // 10 visible characters: a caret past the end scrolls it into view
        assert_eq!(scrolled_to_caret(0, 12, 10), 3);
        assert_eq!(scrolled_to_caret(5, 2, 10), 2, "left of the view");
        assert_eq!(scrolled_to_caret(3, 7, 10), 3, "already visible: no scroll");
    }
}

}
