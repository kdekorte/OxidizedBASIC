//! The menu editor's rules, without a window: rows show nesting, New adds
//! under the selection's parent, Delete takes a submenu with it, and
//! indent/outdent act on the selection.

use crate::menu::MenuItem;
use crate::menu_editor::MenuEditor;

fn file_menu() -> Vec<MenuItem> {
    vec![
        MenuItem::new("mnuFile", "&File", None),
        MenuItem::new("mnuOpen", "&Open", Some(0)),
        MenuItem::new("mnuSave", "&Save", Some(0)),
        MenuItem::new("mnuEdit", "&Edit", None),
    ]
}

#[test]
fn rows_show_each_item_at_its_depth_with_submenus_after_their_parent() {
    let ed = MenuEditor::new(file_menu());
    assert_eq!(ed.rows(), vec![(0, 0), (1, 1), (2, 1), (3, 0)]);
}

#[test]
fn new_adds_under_the_selections_parent_and_selects_the_new_item() {
    let mut ed = MenuEditor::new(file_menu());
    ed.select(Some(1)); // Open, under File
    let at = ed.add("&Print");
    assert_eq!(ed.items[at].parent, Some(0), "a sibling of Open, under File");
    assert_eq!(ed.selected, Some(at));
}

#[test]
fn new_with_nothing_selected_goes_on_the_bar() {
    let mut ed = MenuEditor::new(file_menu());
    let at = ed.add("&Window");
    assert_eq!(ed.items[at].parent, None);
}

#[test]
fn delete_takes_the_selected_item_and_its_submenu() {
    let mut ed = MenuEditor::new(file_menu());
    ed.select(Some(0)); // File and its items
    assert!(ed.delete());
    assert_eq!(ed.items.len(), 1);
    assert_eq!(ed.items[0].name, "mnuEdit");
    assert_eq!(ed.selected, None);
    assert!(!ed.delete(), "nothing selected to delete");
}

#[test]
fn indent_and_outdent_act_on_the_selection() {
    let mut ed = MenuEditor::new(file_menu());
    ed.select(Some(3)); // Edit, on the bar, after Save
    assert!(ed.indent(), "Edit goes under Save");
    assert_eq!(ed.items[3].parent, Some(2));
    assert!(ed.outdent(), "and back up one level, to Save's level");
    assert_eq!(ed.items[3].parent, Some(0), "Save's parent is File, so Edit sits under File");
}

#[test]
fn selecting_past_the_end_selects_nothing() {
    let mut ed = MenuEditor::new(file_menu());
    ed.select(Some(99));
    assert_eq!(ed.selected, None);
    assert!(!ed.indent() && !ed.outdent());
}
