//! The menu model: items form a tree through `parent`, the bar is the top
//! level, and a broken tree (dangling parent, cycle) is caught.

use crate::menu::{bar, children, is_well_formed, MenuItem};

/// File (0) > Open (1), Save (2); a separator (3); Edit (4) > Copy (5).
fn file_menu() -> Vec<MenuItem> {
    vec![
        MenuItem::new("mnuFile", "&File", None),
        MenuItem::new("mnuOpen", "&Open", Some(0)),
        MenuItem::new("mnuSave", "&Save", Some(0)),
        MenuItem::new("mnuSep", "-", Some(0)),
        MenuItem::new("mnuEdit", "&Edit", None),
        MenuItem::new("mnuCopy", "&Copy", Some(4)),
    ]
}

#[test]
fn the_bar_is_the_top_level_items_and_each_submenu_holds_its_own() {
    let items = file_menu();
    assert_eq!(bar(&items), vec![0, 4]);
    assert_eq!(children(&items, Some(0)), vec![1, 2, 3]);
    assert_eq!(children(&items, Some(4)), vec![5]);
}

#[test]
fn a_dash_is_a_separator() {
    let items = file_menu();
    assert!(items[3].is_separator());
    assert!(!items[1].is_separator());
}

#[test]
fn a_well_formed_menu_passes() {
    assert!(is_well_formed(&file_menu()));
    assert!(is_well_formed(&[]));
}

#[test]
fn a_dangling_parent_is_not_well_formed() {
    let mut items = file_menu();
    items[1].parent = Some(99);
    assert!(!is_well_formed(&items));
}

#[test]
fn a_cycle_is_not_well_formed() {
    let mut items = file_menu();
    items[0].parent = Some(1); // File under Open, Open under File
    assert!(!is_well_formed(&items));
    let mut self_parent = file_menu();
    self_parent[4].parent = Some(4);
    assert!(!is_well_formed(&self_parent));
}

#[test]
fn a_click_is_named_for_its_item_and_found_by_that_name() {
    use crate::menu::{click_handler, item_for_handler};
    let items = file_menu();
    assert_eq!(click_handler(&items[1]), "mnuOpen_Click");
    assert_eq!(item_for_handler(&items, "mnuOpen_Click"), Some(1));
    assert_eq!(item_for_handler(&items, "MNUSAVE_CLICK"), Some(2), "case-blind");
    assert_eq!(item_for_handler(&items, "mnuNone_Click"), None);
}

#[test]
fn shortcuts_are_checked_and_turned_into_key_bindings() {
    use crate::menu::{keystroke_for, shortcut_is_valid};
    assert!(shortcut_is_valid(""), "no shortcut is fine");
    assert!(shortcut_is_valid("Ctrl+O"));
    assert!(shortcut_is_valid("Ctrl+Shift+S"));
    assert!(shortcut_is_valid("F5"));
    assert!(!shortcut_is_valid("Ctrl+"), "a modifier with no key");
    assert!(!shortcut_is_valid("Hyper+O"), "an unknown modifier");
    assert!(!shortcut_is_valid("F13"), "no such function key");

    assert_eq!(keystroke_for("Ctrl+O").as_deref(), Some("cmd-o"));
    assert_eq!(keystroke_for("Ctrl+Shift+S").as_deref(), Some("cmd-shift-s"));
    assert_eq!(keystroke_for("F5").as_deref(), Some("f5"));
    assert_eq!(keystroke_for("Cmd+Alt+X").as_deref(), Some("cmd-alt-x"));
    assert_eq!(keystroke_for(""), None);
    assert_eq!(keystroke_for("Hyper+O"), None);
}

#[test]
fn a_form_keeps_its_menu_and_an_old_form_has_none() {
    use crate::form::FormSpec;
    let mut f = FormSpec::new("Form1", "Menus", 400.0, 300.0);
    f.menus = file_menu();
    let back = FormSpec::from_oxform(&f.to_oxform()).unwrap();
    assert_eq!(back.menus, file_menu(), "the menu round-trips through .oxform");
    // a form saved before menus existed has none, and its file has no menus key
    let old = FormSpec::new("Old", "Old", 100.0, 100.0);
    assert!(!old.to_oxform().contains("\"menus\""));
}
