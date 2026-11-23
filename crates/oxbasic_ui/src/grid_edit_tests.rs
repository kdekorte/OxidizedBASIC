//! Tests for the DataGrid's keyboard movement and inline cell editing.

use crate::grid::GridSpec;
use crate::grid_edit::{move_by_key, CellEdit, EditEnd};

/// A 6 x 4 grid, one fixed row and column, the current cell (2, 2).
fn grid() -> GridSpec {
    let mut g = GridSpec::new(6, 4);
    g.select(2, 2);
    g
}

#[test]
fn arrows_move_one_cell() {
    let g = grid();
    assert_eq!(move_by_key(&g, "up", false, 3), Some((1, 2)));
    assert_eq!(move_by_key(&g, "down", false, 3), Some((3, 2)));
    assert_eq!(move_by_key(&g, "left", false, 3), Some((2, 1)));
    assert_eq!(move_by_key(&g, "right", false, 3), Some((2, 3)));
}

#[test]
fn the_edges_hold_and_a_header_is_never_entered() {
    let mut g = GridSpec::new(6, 4);
    g.select(1, 1);
    assert_eq!(move_by_key(&g, "up", false, 3), Some((1, 1)), "not into the header row");
    assert_eq!(move_by_key(&g, "left", false, 3), Some((1, 1)), "not into the header column");
    g.select(5, 3);
    assert_eq!(move_by_key(&g, "down", false, 3), Some((5, 3)));
    assert_eq!(move_by_key(&g, "right", false, 3), Some((5, 3)));
}

#[test]
fn page_keys_home_and_end() {
    let mut g = grid();
    assert_eq!(move_by_key(&g, "pagedown", false, 3), Some((5, 2)), "stops at the last row");
    assert_eq!(move_by_key(&g, "pageup", false, 3), Some((1, 2)), "stops under the header");
    assert_eq!(move_by_key(&g, "home", false, 3), Some((2, 1)));
    assert_eq!(move_by_key(&g, "end", false, 3), Some((2, 3)));
    g.select(4, 2);
    assert_eq!(move_by_key(&g, "pageup", false, 2), Some((2, 2)));
}

#[test]
fn tab_walks_the_row_then_wraps_and_shift_tab_goes_back() {
    let mut g = grid();
    assert_eq!(move_by_key(&g, "tab", false, 3), Some((2, 3)));
    g.select(2, 3);
    assert_eq!(move_by_key(&g, "tab", false, 3), Some((3, 1)), "wraps to the next row's first cell");
    assert_eq!(move_by_key(&g, "tab", true, 3), Some((2, 2)));
    g.select(2, 1);
    assert_eq!(move_by_key(&g, "tab", true, 3), Some((1, 3)), "back to the previous row's last");
    g.select(5, 3);
    assert_eq!(move_by_key(&g, "tab", false, 3), Some((5, 3)), "the last cell stays");
    g.select(1, 1);
    assert_eq!(move_by_key(&g, "tab", true, 3), Some((1, 1)), "the first cell stays");
}

#[test]
fn another_key_does_not_move_and_an_empty_grid_cannot() {
    assert_eq!(move_by_key(&grid(), "a", false, 3), None);
    assert_eq!(move_by_key(&GridSpec::new(0, 0), "down", false, 3), None);
}

fn editable() -> GridSpec {
    let mut g = grid();
    g.editable = true;
    g.set_cell(2, 2, "cat");
    g
}

#[test]
fn an_edit_begins_only_on_an_editable_non_header_cell() {
    let mut g = grid();
    assert!(CellEdit::begin(&g, 2, 2, false).is_none(), "read-only");
    g.editable = true;
    assert!(CellEdit::begin(&g, 0, 2, false).is_none(), "a header");
    assert!(CellEdit::begin(&g, 9, 9, false).is_none(), "outside");
    assert!(CellEdit::begin(&g, 2, 2, false).is_some());
}

#[test]
fn begin_keeps_the_text_with_the_caret_at_the_end_or_replaces_it() {
    let g = editable();
    let keep = CellEdit::begin(&g, 2, 2, false).unwrap();
    assert_eq!((keep.text.as_str(), keep.caret), ("cat", 3));
    let over = CellEdit::begin(&g, 2, 2, true).unwrap();
    assert_eq!((over.text.as_str(), over.caret, over.original.as_str()), ("", 0, "cat"));
}

#[test]
fn typing_inserts_at_the_caret_and_backspace_delete_remove() {
    let g = editable();
    let mut e = CellEdit::begin(&g, 2, 2, false).unwrap();
    e.move_caret("left");
    e.insert("o");
    assert_eq!((e.text.as_str(), e.caret), ("caot", 3));
    assert!(e.backspace());
    assert_eq!(e.text, "cat");
    e.move_caret("home");
    assert!(!e.backspace(), "nothing before the start");
    assert!(e.delete());
    assert_eq!(e.text, "at");
    e.move_caret("end");
    assert!(!e.delete(), "nothing after the end");
}

#[test]
fn caret_keys_move_and_report_whether_they_did() {
    let g = editable();
    let mut e = CellEdit::begin(&g, 2, 2, false).unwrap();
    assert!(!e.move_caret("right"), "already at the end");
    assert!(e.move_caret("home"));
    assert_eq!(e.caret, 0);
    assert!(!e.move_caret("left"));
    assert!(!e.move_caret("up"), "not a caret key");
}

#[test]
fn text_with_several_bytes_per_character_edits_by_character() {
    let mut g = editable();
    g.set_cell(2, 2, "né");
    let mut e = CellEdit::begin(&g, 2, 2, false).unwrap();
    assert_eq!(e.caret, 2);
    e.insert("ü");
    assert_eq!(e.text, "néü");
    assert!(e.backspace());
    assert!(e.backspace());
    assert_eq!(e.text, "n");
}

#[test]
fn commit_keeps_the_text_and_cancel_restores_the_original() {
    let g = editable();
    let mut e = CellEdit::begin(&g, 2, 2, false).unwrap();
    e.insert("s");
    assert_eq!(e.result(EditEnd::Commit), "cats");
    assert_eq!(e.result(EditEnd::Cancel), "cat");
}
