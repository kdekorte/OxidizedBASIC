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
}
