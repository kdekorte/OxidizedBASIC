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
/// The caption a menu shows. The `&` marks a mnemonic in VB's captions; a Mac
/// menu has no mnemonics (its shortcut is drawn to the right of the item), so
/// the `&` is dropped. A doubled `&&` is a literal ampersand.
pub fn display_caption(caption: &str) -> String {
    let mut out = String::with_capacity(caption.len());
    let mut chars = caption.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                out.push('&');
                chars.next();
            }
            // a single & is only a mnemonic marker: drop it
        } else {
            out.push(c);
        }
    }
    out
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

/// The modifier keys of a shortcut, as the Menu Editor's toggles show them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
}

/// Build a `Shortcut` string from the modifier toggles and the key. An empty
/// key means no shortcut, whatever the toggles say.
pub fn shortcut_from(mods: Modifiers, key: &str) -> String {
    let key = key.trim();
    if key.is_empty() {
        return String::new();
    }
    let mut parts: Vec<&str> = Vec::new();
    if mods.ctrl {
        parts.push("Ctrl");
    }
    if mods.cmd {
        parts.push("Cmd");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    let mut out = parts.join("+");
    if !out.is_empty() {
        out.push('+');
    }
    out.push_str(&key.to_ascii_uppercase());
    out
}

/// The key a typed text means in a Shortcut: one letter or digit, or a function
/// key F1–F12 (`f5` or `F5`). `None` for anything else.
pub fn key_from_text(text: &str) -> Option<String> {
    let t = text.trim();
    if t.len() == 1 && t.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(t.to_ascii_uppercase());
    }
    let upper = t.to_ascii_uppercase();
    let f_key = upper.strip_prefix('F').and_then(|n| n.parse::<u8>().ok());
    match f_key {
        Some(n) if (1..=12).contains(&n) => Some(format!("F{}", n)),
        _ => None,
    }
}

/// Split a `Shortcut` string into its modifier toggles and key, so the editor
/// can show an existing shortcut. Text that doesn't parse gives no toggles and
/// the whole text as the key.
pub fn shortcut_parts(text: &str) -> (Modifiers, String) {
    let mut mods = Modifiers::default();
    if text.trim().is_empty() {
        return (mods, String::new());
    }
    let parts: Vec<&str> = text.split('+').map(|p| p.trim()).collect();
    let Some((key, rest)) = parts.split_last() else {
        return (mods, String::new());
    };
    for m in rest {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" => mods.ctrl = true,
            "cmd" => mods.cmd = true,
            "alt" => mods.alt = true,
            "shift" => mods.shift = true,
            _ => return (Modifiers::default(), text.trim().to_string()),
        }
    }
    (mods, key.to_string())
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

/// The menu editor's operations on a form's menu items. Each keeps every
/// `parent` pointing at the item it named before, after items move or go.
///
/// `add` puts a new item at the end of the list under `parent`.
pub fn add(items: &mut Vec<MenuItem>, parent: Option<usize>, caption: &str) -> usize {
    let name = next_name(items, caption);
    items.push(MenuItem::new(&name, caption, parent));
    items.len() - 1
}

/// A name for a new item: `mnu` and its caption's letters, numbered if taken.
fn next_name(items: &[MenuItem], caption: &str) -> String {
    let base: String = caption
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let base = if base.is_empty() {
        "Item".to_string()
    } else {
        base
    };
    let base = format!("mnu{}", base);
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            base.clone()
        } else {
            format!("{}{}", base, n)
        };
        if !items
            .iter()
            .any(|m| m.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        n += 1;
    }
}

/// Remove item `i` and everything under it. Parents that pointed past it
/// are shifted to match the new positions.
pub fn remove(items: &mut Vec<MenuItem>, i: usize) {
    let mut gone = vec![false; items.len()];
    let mut stack = vec![i];
    while let Some(k) = stack.pop() {
        if k < items.len() && !gone[k] {
            gone[k] = true;
            stack.extend(children(items, Some(k)));
        }
    }
    let mut new_index = vec![None; items.len()];
    let mut next = 0;
    for (k, g) in gone.iter().enumerate() {
        if !g {
            new_index[k] = Some(next);
            next += 1;
        }
    }
    let mut kept = Vec::new();
    for (k, m) in items.iter().enumerate() {
        if gone[k] {
            continue;
        }
        let mut m = m.clone();
        m.parent = m.parent.and_then(|p| new_index.get(p).copied().flatten());
        kept.push(m);
    }
    *items = kept;
}

/// Indent item `i` one level: it becomes the last child of the item above it
/// (the previous sibling in the list). Does nothing if there is no item above
/// to take it, or it is already under that item.
pub fn indent(items: &mut [MenuItem], i: usize) -> bool {
    if i == 0 || i >= items.len() {
        return false;
    }
    // the item directly above in the list becomes the new parent, if it can
    // hold children (it is not a separator, and not under `i`)
    let above = i - 1;
    if items[above].is_separator() || items[i].parent == Some(above) {
        return false;
    }
    // it can't become its own descendant
    let mut at = Some(above);
    while let Some(p) = at {
        if p == i {
            return false;
        }
        at = items[p].parent;
    }
    items[i].parent = Some(above);
    true
}

/// Outdent item `i` one level: it moves up to its parent's level, sitting
/// after that parent. Does nothing for a top-level item.
pub fn outdent(items: &mut [MenuItem], i: usize) -> bool {
    match items.get(i).and_then(|m| m.parent) {
        Some(p) => {
            items[i].parent = items[p].parent;
            true
        }
        None => false,
    }
}
