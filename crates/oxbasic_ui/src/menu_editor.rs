//! The menu editor's state: the form's menu items, the one selected, and the
//! actions the editor's buttons run. The IDE draws it; this holds the rules,
//! so they are tested without a window.

use crate::menu::{self, MenuItem};

/// A menu being edited: its items and the selected item (if any).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MenuEditor {
    pub items: Vec<MenuItem>,
    pub selected: Option<usize>,
}

impl MenuEditor {
    pub fn new(items: Vec<MenuItem>) -> MenuEditor {
        MenuEditor {
            items,
            selected: None,
        }
    }

    /// The rows in the order the editor shows them, each with its depth
    /// (0 on the bar), so a submenu is drawn indented under its item.
    pub fn rows(&self) -> Vec<(usize, usize)> {
        fn walk(items: &[MenuItem], parent: Option<usize>, depth: usize, out: &mut Vec<(usize, usize)>) {
            for i in menu::children(items, parent) {
                out.push((i, depth));
                walk(items, Some(i), depth + 1, out);
            }
        }
        let mut out = Vec::new();
        walk(&self.items, None, 0, &mut out);
        out
    }

    /// Select row `i` (an index into `items`), or clear the selection.
    pub fn select(&mut self, i: Option<usize>) {
        self.selected = i.filter(|&k| k < self.items.len());
    }

    /// "New": add an item under the selected item's parent, after the
    /// selection, and select it. With nothing selected it goes on the bar.
    pub fn add(&mut self, caption: &str) -> usize {
        let parent = self.selected.and_then(|i| self.items[i].parent);
        let at = menu::add(&mut self.items, parent, caption);
        self.selected = Some(at);
        at
    }

    /// "Delete": remove the selected item and its submenu.
    pub fn delete(&mut self) -> bool {
        match self.selected {
            Some(i) => {
                menu::remove(&mut self.items, i);
                self.selected = None;
                true
            }
            None => false,
        }
    }

    /// "Indent" (→): make the selected item a child of the item above it.
    pub fn indent(&mut self) -> bool {
        self.selected
            .is_some_and(|i| menu::indent(&mut self.items, i))
    }

    /// "Outdent" (←): move the selected item up a level.
    pub fn outdent(&mut self) -> bool {
        self.selected
            .is_some_and(|i| menu::outdent(&mut self.items, i))
    }
}
