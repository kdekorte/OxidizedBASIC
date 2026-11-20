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

#[test]
fn add_appends_an_item_under_the_parent_with_a_unique_name() {
    use crate::menu::add;
    let mut items = file_menu();
    let n = add(&mut items, Some(0), "&Print");
    assert_eq!(n, 6);
    assert_eq!(items[n].parent, Some(0));
    assert_eq!(items[n].name, "mnuPrint");
    let again = add(&mut items, None, "&Print");
    assert_eq!(items[again].name, "mnuPrint2", "a taken name is numbered");
}

#[test]
fn removing_an_item_removes_its_children_and_keeps_the_rest_pointing_right() {
    use crate::menu::remove;
    let mut items = file_menu();
    // removing File (0) takes Open, Save and the separator with it; Edit and
    // Copy survive, and Copy still sits under Edit at its new place
    remove(&mut items, 0);
    let names: Vec<&str> = items.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["mnuEdit", "mnuCopy"]);
    assert_eq!(items[1].parent, Some(0), "Copy is still under Edit");
    assert!(is_well_formed(&items));
}

#[test]
fn removing_a_middle_item_shifts_later_parents_down() {
    use crate::menu::remove;
    let mut items = file_menu();
    remove(&mut items, 1); // Open goes; Save, the separator, Edit, Copy move up
    let names: Vec<&str> = items.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["mnuFile", "mnuSave", "mnuSep", "mnuEdit", "mnuCopy"]);
    assert_eq!(items[4].parent, Some(3), "Copy's parent, Edit, moved from 4 to 3");
    assert!(is_well_formed(&items));
}

#[test]
fn indent_makes_an_item_the_child_of_the_one_above_and_refuses_bad_moves() {
    use crate::menu::indent;
    // a valid indent: Save (2) goes under Open (1), the item above it
    let mut items = file_menu();
    assert!(indent(&mut items, 2), "Save goes under Open, the item above it");
    assert_eq!(items[2].parent, Some(1));
    assert!(is_well_formed(&items));

    // the first item has nothing above it
    let mut first = file_menu();
    assert!(!indent(&mut first, 0));

    // a separator can't hold children, so nothing indents under it: Edit (4)
    // comes after the separator (3) and is refused
    let mut sep = file_menu();
    assert!(!indent(&mut sep, 4), "the item above is a separator");
    assert_eq!(sep[4].parent, None, "Edit stays on the bar");
}

#[test]
fn outdent_moves_an_item_up_a_level_and_leaves_top_level_items_alone() {
    use crate::menu::outdent;
    let mut items = file_menu();
    assert!(outdent(&mut items, 1), "Open is under File");
    assert_eq!(items[1].parent, None, "Open is now on the bar");
    assert!(!outdent(&mut items, 0), "File is already top level");
    assert!(is_well_formed(&items));
}
