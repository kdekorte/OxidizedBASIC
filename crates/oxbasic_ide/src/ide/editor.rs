//! The code editor's text model: positions, the selection, editing a range
//! of lines, and undo/redo. It works on the editor's `Vec<String>` lines and
//! has no GUI in it, so every operation is tested headless.

/// A place in the text: `(row, column)`, both 0-based, the column counted in
/// characters.
pub type Pos = (usize, usize);

/// The text between two positions, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: Pos,
    pub end: Pos,
}

impl Range {
    /// The range from `a` to `b`, whichever comes first.
    pub fn between(a: Pos, b: Pos) -> Range {
        if a <= b {
            Range { start: a, end: b }
        } else {
            Range { start: b, end: a }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// The selected columns of `row`, as `(from, to)`; `to` is `None` when
    /// the selection runs on past the end of the row (the line break is
    /// selected too).
    pub fn columns_on(&self, row: usize, row_len: usize) -> Option<(usize, Option<usize>)> {
        if self.is_empty() || row < self.start.0 || row > self.end.0 {
            return None;
        }
        let from = if row == self.start.0 { self.start.1 } else { 0 };
        let to = if row == self.end.0 {
            Some(self.end.1.min(row_len))
        } else {
            None
        };
        Some((from.min(row_len), to))
    }
}

fn chars(line: &str) -> Vec<char> {
    line.chars().collect()
}

/// `pos` moved inside the text: a row that exists, a column on it.
pub fn clamp(lines: &[String], pos: Pos) -> Pos {
    let row = pos.0.min(lines.len().saturating_sub(1));
    let len = lines.get(row).map(|l| l.chars().count()).unwrap_or(0);
    (row, pos.1.min(len))
}

/// The text of `range`, rows joined with `\n`.
pub fn text_in(lines: &[String], range: Range) -> String {
    let (s, e) = (clamp(lines, range.start), clamp(lines, range.end));
    if s.0 == e.0 {
        return chars(&lines[s.0])[s.1..e.1].iter().collect();
    }
    let mut out: String = chars(&lines[s.0])[s.1..].iter().collect();
    for line in &lines[s.0 + 1..e.0] {
        out.push('\n');
        out.push_str(line);
    }
    out.push('\n');
    out.extend(chars(&lines[e.0])[..e.1].iter());
    out
}

/// Remove `range` and return where its start is now.
pub fn delete(lines: &mut Vec<String>, range: Range) -> Pos {
    let (s, e) = (clamp(lines, range.start), clamp(lines, range.end));
    if s == e {
        return s;
    }
    let head: String = chars(&lines[s.0])[..s.1].iter().collect();
    let tail: String = chars(&lines[e.0])[e.1..].iter().collect();
    lines.splice(s.0..=e.0, [format!("{}{}", head, tail)]);
    s
}

/// Insert `text` (which may hold line breaks) at `at`; returns the position
/// just after it.
pub fn insert(lines: &mut Vec<String>, at: Pos, text: &str) -> Pos {
    if lines.is_empty() {
        lines.push(String::new());
    }
    let at = clamp(lines, at);
    let line = chars(&lines[at.0]);
    let head: String = line[..at.1].iter().collect();
    let tail: String = line[at.1..].iter().collect();
    // Windows and old-Mac line breaks paste as plain ones
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let parts: Vec<&str> = text.split('\n').collect();
    let last = parts.len() - 1;
    let mut new_lines: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
    let end = (
        at.0 + last,
        if last == 0 { at.1 } else { 0 } + parts[last].chars().count(),
    );
    new_lines[0] = format!("{}{}", head, new_lines[0]);
    new_lines[last].push_str(&tail);
    lines.splice(at.0..=at.0, new_lines);
    end
}

/// Whether `c` is part of a word (for double-click and ⌥-arrow).
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The word around `pos` (the run of word characters it touches), or the
/// single character there when it isn't one.
pub fn word_at(lines: &[String], pos: Pos) -> Range {
    let pos = clamp(lines, pos);
    let line = chars(&lines[pos.0]);
    if line.is_empty() {
        return Range::between(pos, pos);
    }
    // a click just past the last character means that character
    let i = pos.1.min(line.len() - 1);
    if !is_word(line[i]) {
        return Range::between((pos.0, i), (pos.0, i + 1));
    }
    let mut s = i;
    while s > 0 && is_word(line[s - 1]) {
        s -= 1;
    }
    let mut e = i;
    while e < line.len() && is_word(line[e]) {
        e += 1;
    }
    Range::between((pos.0, s), (pos.0, e))
}

/// The whole text.
pub fn all(lines: &[String]) -> Range {
    let last = lines.len().saturating_sub(1);
    let len = lines.get(last).map(|l| l.chars().count()).unwrap_or(0);
    Range::between((0, 0), (last, len))
}

/// What kind of edit made a change, so consecutive typing undoes as one
/// step but a deletion, a paste or a new line each get their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// Typing letters (merged with the typing before it).
    Type,
    /// Backspace / Delete of single characters (merged likewise).
    Erase,
    /// Anything else: a new line, a paste, a cut, deleting a selection.
    Other,
}

/// The text and caret at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub lines: Vec<String>,
    pub cursor: Pos,
}

/// Undo and redo for the editor: the states before each step.
#[derive(Debug, Default, Clone)]
pub struct History {
    undo: Vec<State>,
    redo: Vec<State>,
    /// The kind of the last step, while it may still be extended.
    last: Option<EditKind>,
}

/// How many steps are kept.
const LIMIT: usize = 500;

impl History {
    /// Record `before` (the text just before an edit of `kind`). Typing or
    /// erasing right after the same kind extends the previous step instead.
    pub fn record(&mut self, before: State, kind: EditKind) {
        let merge = kind != EditKind::Other && self.last == Some(kind);
        if !merge {
            self.undo.push(before);
            if self.undo.len() > LIMIT {
                self.undo.remove(0);
            }
        }
        self.last = Some(kind);
        self.redo.clear();
    }

    /// End the current step (the caret moved, the text was clicked): the
    /// next typing starts a new one.
    pub fn break_step(&mut self) {
        self.last = None;
    }

    /// Step back: returns the state to restore, given the current one.
    pub fn undo(&mut self, now: State) -> Option<State> {
        let back = self.undo.pop()?;
        self.redo.push(now);
        self.last = None;
        Some(back)
    }

    /// Step forward again after an undo.
    pub fn redo(&mut self, now: State) -> Option<State> {
        let fwd = self.redo.pop()?;
        self.undo.push(now);
        self.last = None;
        Some(fwd)
    }

    #[cfg(test)]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[cfg(test)]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Forget everything (another document is shown).
    pub fn clear(&mut self) {
        *self = History::default();
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.split('\n').map(|l| l.to_string()).collect()
    }

    #[test]
    fn a_range_is_ordered_whichever_way_it_was_made() {
        let r = Range::between((2, 1), (0, 4));
        assert_eq!((r.start, r.end), ((0, 4), (2, 1)));
        assert!(Range::between((1, 1), (1, 1)).is_empty());
    }

    #[test]
    fn text_in_a_range_on_one_line_and_across_lines() {
        let l = lines("Sub Main()\n  Print 1\nEnd Sub");
        assert_eq!(text_in(&l, Range::between((0, 4), (0, 8))), "Main");
        assert_eq!(
            text_in(&l, Range::between((0, 4), (2, 3))),
            "Main()\n  Print 1\nEnd"
        );
        assert_eq!(text_in(&l, all(&l)), "Sub Main()\n  Print 1\nEnd Sub");
        assert_eq!(text_in(&l, Range::between((0, 10), (1, 0))), "\n");
    }

    #[test]
    fn deleting_a_range_joins_its_ends() {
        let mut l = lines("Sub Main()\n  Print 1\nEnd Sub");
        let at = delete(&mut l, Range::between((0, 3), (2, 3)));
        assert_eq!(at, (0, 3));
        assert_eq!(l, lines("Sub Sub"));
        let at = delete(&mut l, Range::between((0, 2), (0, 2)));
        assert_eq!((at, l.clone()), ((0, 2), lines("Sub Sub")));
    }

    #[test]
    fn inserting_text_with_and_without_line_breaks() {
        let mut l = lines("ab");
        assert_eq!(insert(&mut l, (0, 1), "XY"), (0, 3));
        assert_eq!(l, lines("aXYb"));
        let mut l = lines("ab");
        assert_eq!(insert(&mut l, (0, 1), "1\n22\n333"), (2, 3));
        assert_eq!(l, lines("a1\n22\n333b"));
        // CRLF from the clipboard becomes plain line breaks
        let mut l = lines("");
        assert_eq!(insert(&mut l, (0, 0), "x\r\ny"), (1, 1));
        assert_eq!(l, lines("x\ny"));
    }

    #[test]
    fn delete_then_insert_round_trips_a_cut_and_paste() {
        let original = lines("one\ntwo\nthree");
        let mut l = original.clone();
        let r = Range::between((0, 1), (2, 2));
        let cut = text_in(&l, r);
        let at = delete(&mut l, r);
        insert(&mut l, at, &cut);
        assert_eq!(l, original);
    }

    #[test]
    fn a_word_is_found_around_the_click() {
        let l = lines("  Label1.Caption = x");
        assert_eq!(text_in(&l, word_at(&l, (0, 4))), "Label1");
        assert_eq!(text_in(&l, word_at(&l, (0, 9))), "Caption");
        assert_eq!(text_in(&l, word_at(&l, (0, 8))), ".");
        assert_eq!(text_in(&l, word_at(&l, (0, 99))), "x");
    }

    #[test]
    fn columns_selected_on_each_row() {
        let r = Range::between((0, 2), (2, 1));
        assert_eq!(r.columns_on(0, 5), Some((2, None)));
        assert_eq!(r.columns_on(1, 3), Some((0, None)));
        assert_eq!(r.columns_on(2, 4), Some((0, Some(1))));
        assert_eq!(r.columns_on(3, 4), None);
    }

    fn state(s: &str, cursor: Pos) -> State {
        State {
            lines: lines(s),
            cursor,
        }
    }

    #[test]
    fn typing_undoes_as_one_step_and_redoes() {
        let mut h = History::default();
        h.record(state("", (0, 0)), EditKind::Type);
        h.record(state("a", (0, 1)), EditKind::Type);
        h.record(state("ab", (0, 2)), EditKind::Type);
        let back = h.undo(state("abc", (0, 3))).unwrap();
        assert_eq!(back, state("", (0, 0)));
        assert!(!h.can_undo());
        let fwd = h.redo(back).unwrap();
        assert_eq!(fwd, state("abc", (0, 3)));
        assert!(!h.can_redo());
    }

    #[test]
    fn a_new_kind_of_edit_or_a_caret_move_starts_a_new_step() {
        let mut h = History::default();
        h.record(state("", (0, 0)), EditKind::Type);
        h.record(state("ab", (0, 2)), EditKind::Erase);
        h.break_step();
        h.record(state("a", (0, 1)), EditKind::Erase);
        h.record(state("", (0, 0)), EditKind::Other);
        h.record(state("x", (0, 1)), EditKind::Other);
        let mut undos = 0;
        let mut now = state("xy", (0, 2));
        while let Some(s) = h.undo(now.clone()) {
            now = s;
            undos += 1;
        }
        assert_eq!(undos, 5);
        assert_eq!(now, state("", (0, 0)));
    }

    #[test]
    fn a_new_edit_after_undo_drops_the_redo_steps() {
        let mut h = History::default();
        h.record(state("", (0, 0)), EditKind::Other);
        let back = h.undo(state("a", (0, 1))).unwrap();
        assert!(h.can_redo());
        h.record(back, EditKind::Other);
        assert!(!h.can_redo(), "the undone future is gone");
    }
}
