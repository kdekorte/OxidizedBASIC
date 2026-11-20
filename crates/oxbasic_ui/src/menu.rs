//! Menus: a form's menu bar. Each item is a record in the form's `menus`
//! list; a submenu is an item whose `parent` is another item's index. The
//! top-level items form the bar, and each item's `Click` runs its handler.

use serde::{Deserialize, Serialize};

/// One item of a form's menu (VB's Menu Editor row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MenuItem {
    /// The item's name, for its `Name_Click` handler (`mnuOpen_Click`).
    pub name: String,
    /// `Caption`: the text shown. A `-` makes a separator.
    pub caption: String,
    /// The index of the item this one sits under, or `None` for a top-level
    /// item on the bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<usize>,
    /// `Shortcut`, such as `Ctrl+O` (shown beside the caption).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shortcut: String,
    /// `Enabled`: a disabled item is greyed and doesn't run.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// `Checked`: a tick beside the item.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub checked: bool,
}

fn default_enabled() -> bool {
    true
}

impl MenuItem {
    pub fn new(name: &str, caption: &str, parent: Option<usize>) -> MenuItem {
        MenuItem {
            name: name.to_string(),
            caption: caption.to_string(),
            parent,
            shortcut: String::new(),
            enabled: true,
            checked: false,
        }
    }

    /// Whether the item is a separator line (`-`).
    pub fn is_separator(&self) -> bool {
        self.caption == "-"
    }
}

/// The items directly under `parent` (`None` for the top-level bar), in the
/// order they were added.
pub fn children(items: &[MenuItem], parent: Option<usize>) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, m)| m.parent == parent)
        .map(|(i, _)| i)
        .collect()
}

/// The menu bar's top-level items' indexes.
pub fn bar(items: &[MenuItem]) -> Vec<usize> {
    children(items, None)
}

/// Whether `index` names a menu item, and if so whether it sits in a menu that
/// is itself a valid item (no cycles, no dangling parents).
pub fn is_well_formed(items: &[MenuItem]) -> bool {
    items.iter().enumerate().all(|(i, m)| {
        let mut at = m.parent;
        let mut steps = 0;
        while let Some(p) = at {
            if p >= items.len() || p == i || steps > items.len() {
                return false;
            }
            at = items[p].parent;
            steps += 1;
        }
        true
    })
}
