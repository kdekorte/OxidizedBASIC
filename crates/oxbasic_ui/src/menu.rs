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

/// The name a menu item's click is known by, for `Name_Click`.
pub fn click_handler(item: &MenuItem) -> String {
    format!("{}_Click", item.name)
}

/// Which item a click is for, by the handler name it runs (`mnuOpen_Click`),
/// matched without case. `None` when no item has that name.
pub fn item_for_handler(items: &[MenuItem], handler: &str) -> Option<usize> {
    items
        .iter()
        .position(|m| click_handler(m).eq_ignore_ascii_case(handler))
}

/// Whether a `Shortcut` (`Ctrl+O`, `Ctrl+Shift+S`, `F5`) is one this module
/// can bind: optional modifiers (Ctrl, Shift, Alt, Cmd) and one key (a letter,
/// digit, or F1 to F12). An empty shortcut is valid: it has none.
pub fn shortcut_is_valid(text: &str) -> bool {
    if text.trim().is_empty() {
        return true;
    }
    let parts: Vec<String> = text
        .split('+')
        .map(|p| p.trim().to_ascii_lowercase())
        .collect();
    let Some((key, mods)) = parts.split_last() else {
        return false;
    };
    let mods_ok = mods
        .iter()
        .all(|m| matches!(m.as_str(), "ctrl" | "shift" | "alt" | "cmd"));
    let plain_key = key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric());
    let function_key = key
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n));
    mods_ok && (plain_key || function_key)
}

/// The keystroke GPUI binds for a shortcut: Ctrl and Cmd both mean Command on
/// the Mac, as the IDE's own bindings use it. `None` when the text is empty or
/// not a valid shortcut.
pub fn keystroke_for(text: &str) -> Option<String> {
    if text.trim().is_empty() || !shortcut_is_valid(text) {
        return None;
    }
    let parts: Vec<String> = text
        .split('+')
        .map(|p| p.trim().to_ascii_lowercase())
        .collect();
    let (key, mods) = parts.split_last()?;
    let has = |m: &str| mods.iter().any(|x| x == m);
    let mut out: Vec<&str> = Vec::new();
    if has("ctrl") || has("cmd") {
        out.push("cmd");
    }
    if has("alt") {
        out.push("alt");
    }
    if has("shift") {
        out.push("shift");
    }
    out.push(key);
    Some(out.join("-"))
}
