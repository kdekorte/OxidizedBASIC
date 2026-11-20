//! The Menu Editor window's rules: each row's label shows the caption at its
//! depth without the `&` mnemonics, and a separator is drawn as a rule.

use super::menu_editor_window::MenuEditorView;
use oxbasic_ui::menu::MenuItem;

#[test]
fn a_row_shows_its_caption_at_its_depth_without_mnemonics() {
    let open = MenuItem::new("mnuOpen", "&Open…", Some(0));
    assert_eq!(MenuEditorView::row_label(&open, 0), "Open…");
    assert_eq!(MenuEditorView::row_label(&open, 2), "        Open…");
}

#[test]
fn a_separator_is_drawn_as_a_rule() {
    let sep = MenuItem::new("mnuSep", "-", Some(0));
    assert_eq!(MenuEditorView::row_label(&sep, 1), "    ────");
}
