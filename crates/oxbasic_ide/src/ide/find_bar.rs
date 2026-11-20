//! The find bar of the code window: a row under the header with a Find box,
//! a Replace box, the match count and buttons. Its state (what is typed,
//! the matches, which one is current) lives on the `Ide` as `find`; the
//! matching itself is in [`super::find`].

use super::editor::Pos;
use super::find::{self, Search};
use super::*;

/// The find bar's state.
#[derive(Debug, Default)]
pub(super) struct FindBar {
    pub find: String,
    pub replace: String,
    pub match_case: bool,
    /// The matches of `find` in the code, rebuilt when the text or the
    /// search changes.
    pub found: Vec<editor::Range>,
    /// Which of `found` is current.
    pub current: Option<usize>,
    /// Whether the Replace box is shown (⌥⌘F), or only Find (⌘F).
    pub replacing: bool,
    /// Where the search began (the caret when the bar opened); typing
    /// searches from here.
    pub from: Pos,
}

impl FindBar {
    pub(super) fn search(&self) -> Search {
        Search {
            text: self.find.clone(),
            match_case: self.match_case,
        }
    }
}

impl Ide {
    /// Recompute the matches after the text or the search changed. The
    /// current match stays the one nearest to where it was.
    pub(super) fn refresh_matches(&mut self) {
        let from = self.find.as_ref().map(|b| b.from).unwrap_or(self.cursor);
        let Some(bar) = self.find.as_mut() else {
            return;
        };
        bar.found = find::matches(&self.lines, &bar.search());
        // the first match from where the search began, not from the caret
        // (typing moves the caret, so it would jump past matches)
        bar.current = find::next_from(&bar.found, from);
    }

    /// ⌘F / ⌥⌘F: open the bar (with Replace when asked), with the selection
    /// (if it is on one line) as the text to find.
    pub(super) fn open_find(&mut self, replacing: bool, win: &mut Window) {
        let selected = self
            .selected_text()
            .filter(|s| !s.contains('\n'))
            .unwrap_or_default();
        // a new search starts where the caret is now
        let from = self.cursor;
        let bar = self.find.get_or_insert_with(FindBar::default);
        bar.replacing = replacing;
        bar.from = from;
        if !selected.is_empty() {
            bar.find = selected;
        }
        self.refresh_matches();
        win.focus(&self.find_focus);
    }

    /// Close the bar and return to the text.
    pub(super) fn close_find(&mut self, win: &mut Window) {
        self.find = None;
        win.focus(&self.editor_focus);
    }

    /// Go to the next (or previous) match and select it, so typing replaces
    /// it.
    pub(super) fn find_step(&mut self, forward: bool) {
        let Some(bar) = self.find.as_mut() else {
            return;
        };
        // from the current match when there is one, so ⌘G moves on from it;
        // otherwise from the caret
        let pos = match (forward, bar.current) {
            (true, Some(i)) if !bar.found.is_empty() => Some((i + 1) % bar.found.len()),
            (false, Some(i)) if !bar.found.is_empty() => {
                Some((i + bar.found.len() - 1) % bar.found.len())
            }
            (true, _) => find::next_from(&bar.found, self.cursor),
            (false, _) => find::prev_from(&bar.found, self.cursor),
        };
        let Some(i) = pos else { return };
        bar.current = Some(i);
        let r = bar.found[i];
        self.anchor = Some(r.start);
        self.cursor = r.end;
        self.history.break_step();
    }

    /// Replace the current match, then go on to the next one.
    pub(super) fn replace_current(&mut self) {
        let Some(bar) = self.find.as_ref() else {
            return;
        };
        let Some(i) = bar.current else { return };
        let r = bar.found[i];
        let text = bar.replace.clone();
        self.begin_edit(editor::EditKind::Other);
        let at = find::replace_one(&mut self.lines, r, &text);
        self.cursor = at;
        self.anchor = None;
        self.refresh_matches();
        self.find_step(true);
    }

    /// Replace every match as one undo step; returns how many.
    pub(super) fn replace_everything(&mut self) -> usize {
        let Some(bar) = self.find.as_ref() else {
            return 0;
        };
        let search = bar.search();
        let text = bar.replace.clone();
        if find::matches(&self.lines, &search).is_empty() {
            return 0;
        }
        self.begin_edit(editor::EditKind::Other);
        let n = find::replace_all(&mut self.lines, &search, &text);
        self.anchor = None;
        self.cursor = editor::clamp(&self.lines, self.cursor);
        self.refresh_matches();
        n
    }

    /// Keys typed in the bar's boxes: the Find or Replace text. `which` is
    /// true for Replace. Returns whether the key was used.
    pub(super) fn find_key(&mut self, replace: bool, ev: &KeyDownEvent, win: &mut Window) -> bool {
        let ks = &ev.keystroke;
        match ks.key.as_str() {
            "escape" => {
                self.close_find(win);
                return true;
            }
            "enter" => {
                if ks.modifiers.shift {
                    self.find_step(false);
                } else if replace {
                    self.replace_current();
                } else {
                    self.find_step(true);
                }
                return true;
            }
            _ => {}
        }
        if ks.modifiers.platform || ks.modifiers.control {
            return false;
        }
        let Some(bar) = self.find.as_mut() else {
            return false;
        };
        let buf = if replace {
            &mut bar.replace
        } else {
            &mut bar.find
        };
        if ks.key == "backspace" {
            buf.pop();
        } else if let Some(s) = typed_text(ev) {
            buf.push_str(&s);
        } else {
            return false;
        }
        if !replace {
            // typing selects the first match from the caret (refresh_matches
            // sets it); it doesn't step past that one
            self.refresh_matches();
            if let Some(i) = self.find.as_ref().and_then(|b| b.current) {
                let r = self.find.as_ref().unwrap().found[i];
                self.anchor = Some(r.start);
                self.cursor = r.end;
                self.history.break_step();
            }
        }
        true
    }
}
