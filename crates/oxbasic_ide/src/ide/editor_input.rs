//! Keyboard and mouse input for the code editor: moving the caret,
//! selecting with Shift and the mouse, editing, the clipboard (⌘X ⌘C ⌘V)
//! and undo/redo (⌘Z ⇧⌘Z). The text operations themselves are in
//! [`super::editor`]; this file connects them to the `Ide`'s `lines`,
//! `cursor`, `anchor` and `history`.

use super::editor::{self, EditKind, Pos, Range, State};
use super::*;

impl Ide {
    pub(super) fn line_len(&self, row: usize) -> usize {
        self.lines.get(row).map(|l| l.chars().count()).unwrap_or(0)
    }

    /// The selected range, if anything is selected.
    pub(super) fn selection(&self) -> Option<Range> {
        let a = self.anchor?;
        let r = Range::between(a, self.cursor);
        (!r.is_empty()).then_some(r)
    }

    /// The selected text, if any.
    pub(super) fn selected_text(&self) -> Option<String> {
        self.selection().map(|r| editor::text_in(&self.lines, r))
    }

    fn state(&self) -> State {
        State {
            lines: self.lines.clone(),
            cursor: self.cursor,
        }
    }

    /// Record the text as it is now, before an edit of `kind`.
    pub(super) fn begin_edit(&mut self, kind: EditKind) {
        let before = self.state();
        self.history.record(before, kind);
    }

    /// Before the IDE itself changes the code (an event handler added from
    /// the object/event boxes, a rename): make it one undoable step and drop
    /// the selection, which the change may have moved under.
    pub(super) fn begin_ide_edit(&mut self) {
        self.begin_edit(EditKind::Other);
        self.anchor = None;
    }

    /// Delete the selection, if there is one. Returns whether there was.
    fn delete_selection(&mut self) -> bool {
        let Some(r) = self.selection() else {
            self.anchor = None;
            return false;
        };
        self.cursor = editor::delete(&mut self.lines, r);
        self.anchor = None;
        true
    }

    /// Type `text` at the caret, replacing the selection.
    pub(super) fn type_text(&mut self, text: &str) {
        let kind = if self.selection().is_some() || text.contains('\n') {
            EditKind::Other
        } else {
            EditKind::Type
        };
        self.begin_edit(kind);
        self.delete_selection();
        self.cursor = editor::insert(&mut self.lines, self.cursor, text);
    }

    /// Backspace: the selection, or the character before the caret (joining
    /// with the line above at the start of a line).
    fn backspace(&mut self) {
        if self.selection().is_some() {
            self.begin_edit(EditKind::Other);
            self.delete_selection();
            return;
        }
        let (row, col) = self.cursor;
        if row == 0 && col == 0 {
            return;
        }
        self.begin_edit(EditKind::Erase);
        let start = if col > 0 {
            (row, col - 1)
        } else {
            (row - 1, self.line_len(row - 1))
        };
        self.cursor = editor::delete(&mut self.lines, Range::between(start, (row, col)));
    }

    /// Delete: the selection, or the character after the caret (joining
    /// with the line below at the end of a line).
    fn delete_forward(&mut self) {
        if self.selection().is_some() {
            self.begin_edit(EditKind::Other);
            self.delete_selection();
            return;
        }
        let (row, col) = self.cursor;
        let end = if col < self.line_len(row) {
            (row, col + 1)
        } else if row + 1 < self.lines.len() {
            (row + 1, 0)
        } else {
            return;
        };
        self.begin_edit(EditKind::Erase);
        self.cursor = editor::delete(&mut self.lines, Range::between((row, col), end));
    }

    /// Enter: a new line, indented like this one (one level more after a
    /// block opener such as `Then` or `Do`).
    fn newline(&mut self) {
        self.begin_edit(EditKind::Other);
        self.delete_selection();
        let row = self.cursor.0;
        let line = self.lines[row].clone();
        let indent: String = line.chars().take_while(|c| *c == ' ').collect();
        let trimmed = line.trim_end();
        let extra = if trimmed.ends_with("Then") || trimmed.ends_with("Do") {
            "  "
        } else {
            ""
        };
        let text = format!("\n{}{}", indent, extra);
        self.cursor = editor::insert(&mut self.lines, self.cursor, &text);
    }

    /// ⌘C: copy the selection to the clipboard. Returns whether anything
    /// was copied.
    pub(super) fn copy(&mut self, cx: &mut App) -> bool {
        let Some(text) = self.selected_text() else {
            return false;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        true
    }

    /// ⌘X: copy the selection, then delete it.
    pub(super) fn cut(&mut self, cx: &mut App) {
        if self.copy(cx) {
            self.begin_edit(EditKind::Other);
            self.delete_selection();
        }
    }

    /// ⌘V: insert the clipboard's text, replacing the selection.
    pub(super) fn paste(&mut self, cx: &mut App) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.begin_edit(EditKind::Other);
        self.delete_selection();
        self.cursor = editor::insert(&mut self.lines, self.cursor, &text);
    }

    /// ⌘Z: step back.
    pub(super) fn undo(&mut self) {
        if let Some(s) = self.history.undo(self.state()) {
            self.restore(s);
        }
    }

    /// ⇧⌘Z: step forward again.
    pub(super) fn redo(&mut self) {
        if let Some(s) = self.history.redo(self.state()) {
            self.restore(s);
        }
    }

    fn restore(&mut self, s: State) {
        self.lines = s.lines;
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.cursor = editor::clamp(&self.lines, s.cursor);
        self.anchor = None;
    }

    /// ⌘A: select everything.
    pub(super) fn select_all(&mut self) {
        let r = editor::all(&self.lines);
        self.anchor = Some(r.start);
        self.cursor = r.end;
        self.history.break_step();
    }

    /// Run an Edit-menu command on the code, when the code editor has the
    /// focus (it is the only text the Edit menu works on so far).
    pub(super) fn edit_action(
        &mut self,
        win: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Ide, &mut App),
    ) {
        if !self.editor_focus.is_focused(win) {
            return;
        }
        f(self, cx);
        self.reveal_cursor = true;
        cx.notify();
    }
}

/// Where a word-wise move (⌥← ⌥→) lands.
fn word_step(lines: &[String], pos: Pos, forward: bool) -> Pos {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let (row, col) = pos;
    let line: Vec<char> = lines[row].chars().collect();
    if forward {
        if col >= line.len() {
            return if row + 1 < lines.len() {
                (row + 1, 0)
            } else {
                pos
            };
        }
        let mut i = col;
        while i < line.len() && !is_word(line[i]) {
            i += 1;
        }
        while i < line.len() && is_word(line[i]) {
            i += 1;
        }
        (row, i)
    } else {
        if col == 0 {
            return if row > 0 {
                (row - 1, lines[row - 1].chars().count())
            } else {
                pos
            };
        }
        let mut i = col;
        while i > 0 && !is_word(line[i - 1]) {
            i -= 1;
        }
        while i > 0 && is_word(line[i - 1]) {
            i -= 1;
        }
        (row, i)
    }
}

impl Ide {
    /// Move the caret to `to`. With Shift the selection grows to it (from
    /// the caret); without, any selection is dropped.
    fn move_to(&mut self, to: Pos, extend: bool) {
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = editor::clamp(&self.lines, to);
        self.history.break_step();
    }

    /// Where an arrow key (or Home/End) moves the caret. A plain ← or →
    /// with a selection collapses it to that side instead.
    fn arrow_target(&self, key: &str, m: &gpui::Modifiers) -> Option<Pos> {
        let (row, col) = self.cursor;
        let last = self.lines.len().saturating_sub(1);
        if !m.shift && !m.alt && !m.platform {
            if let Some(r) = self.selection() {
                match key {
                    "left" | "up" => return Some(r.start),
                    "right" | "down" => return Some(r.end),
                    _ => {}
                }
            }
        }
        Some(match key {
            // ⌘← / ⌘→ go to the line's ends, ⌥ moves by a word
            "left" if m.platform => (row, 0),
            "right" if m.platform => (row, self.line_len(row)),
            "left" if m.alt => word_step(&self.lines, self.cursor, false),
            "right" if m.alt => word_step(&self.lines, self.cursor, true),
            "left" if col > 0 => (row, col - 1),
            "left" if row > 0 => (row - 1, self.line_len(row - 1)),
            "left" => (0, 0),
            "right" if col < self.line_len(row) => (row, col + 1),
            "right" if row < last => (row + 1, 0),
            "right" => self.cursor,
            // ⌘↑ / ⌘↓ go to the text's ends
            "up" if m.platform => (0, 0),
            "down" if m.platform => (last, self.line_len(last)),
            "up" if row > 0 => (row - 1, col.min(self.line_len(row - 1))),
            "up" => (0, 0),
            "down" if row < last => (row + 1, col.min(self.line_len(row + 1))),
            "down" => (last, self.line_len(last)),
            "home" => (row, 0),
            "end" => (row, self.line_len(row)),
            "pageup" => (row.saturating_sub(20), col),
            "pagedown" => ((row + 20).min(last), col),
            _ => return None,
        })
    }

    /// A key pressed in the editor. Returns whether it was handled.
    pub(super) fn editor_key(&mut self, ev: &KeyDownEvent, cx: &mut App) -> bool {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        let key = ks.key.as_str();
        // the editing shortcuts
        if m.platform && !m.control {
            match key {
                "z" if m.shift => self.redo(),
                "z" => self.undo(),
                "y" => self.redo(),
                "x" => self.cut(cx),
                "c" => {
                    self.copy(cx);
                }
                "v" => self.paste(cx),
                "a" => self.select_all(),
                "left" | "right" | "up" | "down" => {
                    if let Some(to) = self.arrow_target(key, m) {
                        self.move_to(to, m.shift);
                    }
                }
                _ => return false,
            }
            return true;
        }
        if m.control {
            return false;
        }
        if let Some(to) = self.arrow_target(key, m) {
            self.move_to(to, m.shift);
            return true;
        }
        match key {
            "enter" => self.newline(),
            "backspace" => self.backspace(),
            "delete" => self.delete_forward(),
            "tab" => self.type_text("  "),
            "space" => self.type_text(" "),
            "escape" => self.anchor = None,
            _ => match typed_text(ev) {
                Some(s) => self.type_text(&s),
                None => return false,
            },
        }
        true
    }
}
/// The mouse in the editor.
impl Ide {
    /// The text position under `pos` (window coordinates), measuring the
    /// line in the editor's monospaced font.
    pub(super) fn pos_at(&self, pos: gpui::Point<gpui::Pixels>, win: &mut Window) -> Option<Pos> {
        if self.lines.is_empty() {
            return None;
        }
        let bounds = self.editor_scroll.bounds();
        let offset = self.editor_scroll.offset();
        let y = f32::from(pos.y - bounds.origin.y - offset.y);
        let x = f32::from(pos.x - bounds.origin.x - offset.x) - CODE_X;
        // above the text: its start; below: its end
        if y < 0.0 {
            return Some((0, 0));
        }
        let last = self.lines.len() - 1;
        if y >= ROW_H * self.lines.len() as f32 {
            return Some((last, self.line_len(last)));
        }
        let row = ((y / ROW_H) as usize).min(last);
        let ts = win.text_system();
        let font_id = ts.resolve_font(&gpui::font("Menlo"));
        let size = win.text_style().font_size.to_pixels(win.rem_size());
        let mut acc = 0.0;
        let mut col = 0;
        for ch in self.lines[row].chars() {
            let w = ts
                .advance(font_id, size, ch)
                .map(|s| f32::from(s.width))
                .unwrap_or(8.0);
            if acc + w / 2.0 > x {
                break;
            }
            acc += w;
            col += 1;
        }
        Some((row, col))
    }

    /// A press in the editor: place the caret (Shift extends the
    /// selection), a double click selects a word, a triple click the line.
    pub(super) fn editor_mouse_down(&mut self, ev: &MouseDownEvent, win: &mut Window) {
        let Some(pos) = self.pos_at(ev.position, win) else {
            return;
        };
        self.history.break_step();
        match ev.click_count {
            2 => {
                let w = editor::word_at(&self.lines, pos);
                self.anchor = Some(w.start);
                self.cursor = w.end;
            }
            n if n >= 3 => {
                let row = pos.0;
                self.anchor = Some((row, 0));
                // the whole line, its line break included
                self.cursor = if row + 1 < self.lines.len() {
                    (row + 1, 0)
                } else {
                    (row, self.line_len(row))
                };
            }
            _ if ev.modifiers.shift => self.move_to(pos, true),
            _ => {
                self.anchor = Some(pos);
                self.cursor = pos;
                self.selecting = true;
            }
        }
    }

    /// The pointer moved: while the button is held, drag the selection
    /// out to it. Returns whether the selection changed.
    pub(super) fn editor_mouse_drag(&mut self, ev: &MouseMoveEvent, win: &mut Window) -> bool {
        if !self.selecting {
            return false;
        }
        if !ev.dragging() {
            self.selecting = false;
            return false;
        }
        match self.pos_at(ev.position, win) {
            Some(pos) if pos != self.cursor => {
                self.cursor = pos;
                true
            }
            _ => false,
        }
    }
}
