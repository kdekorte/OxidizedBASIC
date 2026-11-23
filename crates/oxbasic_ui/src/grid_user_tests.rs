//! What a user's grid actions do in the program: selecting a cell raises its
//! enter / leave events, scrolling raises Scroll, a program's own move does
//! the same, and an edit stores its text and raises AfterEdit.

use crate::form::{ControlKind, ControlSpec, FormSpec};
use crate::grid::GridSpec;
use crate::host::{FormHost, UserEdit};
use crate::Rect;
use std::cell::RefCell;
use std::rc::Rc;

/// Form1 with Grid1: 20 rows x 5 cols in a 300 x 120 box, so it scrolls.
fn host(editable: bool) -> Rc<RefCell<FormHost>> {
    let mut f = FormSpec::new("Form1", "Grid", 400.0, 300.0);
    let mut g = ControlSpec::new("Grid1", ControlKind::DataGrid, Rect::new(0.0, 0.0, 300.0, 120.0));
    g.grid = GridSpec::new(20, 5);
    g.grid.editable = editable;
    f.control(g);
    Rc::new(RefCell::new(FormHost::new(f)))
}

fn handlers(h: &Rc<RefCell<FormHost>>) -> Vec<String> {
    h.borrow().raised_handlers()
}

fn grid(h: &Rc<RefCell<FormHost>>) -> GridSpec {
    h.borrow().form.controls[0].grid.clone()
}

fn select(h: &Rc<RefCell<FormHost>>, row: usize, col: usize) {
    h.borrow_mut().apply_edit(UserEdit::GridSelect { control: 0, row, col });
}

#[test]
fn selecting_a_cell_moves_the_current_cell_and_raises_the_cell_events_in_order() {
    let h = host(false);
    select(&h, 3, 2);
    let g = grid(&h);
    assert_eq!((g.row, g.col), (3, 2));
    assert_eq!(
        handlers(&h),
        vec!["Grid1_LeaveCell", "Grid1_RowColChange", "Grid1_EnterCell"]
    );
}

#[test]
fn selecting_the_current_cell_again_raises_nothing() {
    let h = host(false);
    select(&h, 1, 1);
    assert!(handlers(&h).is_empty(), "the grid starts on (1, 1)");
}

#[test]
fn a_header_or_a_cell_outside_the_grid_cannot_be_selected() {
    let h = host(false);
    select(&h, 0, 2);
    select(&h, 2, 0);
    select(&h, 99, 1);
    let g = grid(&h);
    assert_eq!((g.row, g.col), (1, 1));
    assert!(handlers(&h).is_empty());
}

#[test]
fn a_grid_select_to_the_next_cell_moves_the_program_and_raises_row_col_change() {
    let h = host(false);
    select(&h, 1, 1);
    h.borrow_mut().raised.clear();
    select(&h, 1, 2);
    let g = grid(&h);
    assert_eq!((g.row, g.col), (1, 2), "the program's current cell moved");
    assert!(
        handlers(&h).contains(&"Grid1_RowColChange".to_string()),
        "{:?}",
        handlers(&h)
    );
}

#[test]
fn selecting_a_cell_below_the_view_scrolls_it_into_view_and_raises_scroll() {
    let h = host(false);
    select(&h, 15, 1);
    let g = grid(&h);
    assert!(g.top_row > 1, "the view moved down: {}", g.top_row);
    assert!(handlers(&h).contains(&"Grid1_Scroll".to_string()), "{:?}", handlers(&h));
}

#[test]
fn a_user_scroll_sets_the_view_and_raises_scroll_once() {
    let h = host(false);
    h.borrow_mut().apply_edit(UserEdit::GridView { control: 0, top_row: 7, left_col: 2 });
    let g = grid(&h);
    assert_eq!((g.top_row, g.left_col), (7, 2));
    assert_eq!(handlers(&h), vec!["Grid1_Scroll"]);
    // the same view again is no scroll
    h.borrow_mut().raised.clear();
    h.borrow_mut().apply_edit(UserEdit::GridView { control: 0, top_row: 7, left_col: 2 });
    assert!(handlers(&h).is_empty());
}

#[test]
fn a_scroll_to_a_fixed_row_is_pulled_to_the_first_scrolling_one() {
    let h = host(false);
    h.borrow_mut().apply_edit(UserEdit::GridView { control: 0, top_row: 0, left_col: 0 });
    let g = grid(&h);
    assert_eq!((g.top_row, g.left_col), (1, 1));
}

#[test]
fn a_program_setting_row_raises_the_cell_events_and_scrolls_it_into_view() {
    use oxbasic_runtime::{ObjectHost, ObjectRef, Variant};
    let h = host(false);
    // through the host's own `set`, which the interpreter calls, so the
    // queue is read before the interpreter takes it as the statement ends
    let obj = ObjectRef { name: "Grid1".into(), index: None };
    h.borrow_mut().set(&obj, "Row", &[], Variant::Integer(15)).unwrap();
    assert_eq!(grid(&h).row, 15);
    assert_eq!(
        handlers(&h),
        vec!["Grid1_LeaveCell", "Grid1_RowColChange", "Grid1_EnterCell", "Grid1_Scroll"],
        "{:?}",
        handlers(&h)
    );
    let g = grid(&h);
    let (w, hh) = crate::grid_layout::inside(&h.borrow().form.controls[0].rect);
    let l = crate::grid_layout::GridLayout::new(&g, w, hh);
    assert!(l.rows.iter().any(|s| s.0 == 15), "row 15 is in view after the program moves to it");
}

#[test]
fn an_edit_stores_the_text_and_raises_after_edit_with_the_cell() {
    let h = host(true);
    h.borrow_mut().apply_edit(UserEdit::GridCell {
        control: 0,
        row: 2,
        col: 3,
        text: "42".into(),
    });
    assert_eq!(grid(&h).cell(2, 3), Some("42"));
    assert_eq!(handlers(&h), vec!["Grid1_AfterEdit"]);
    let ev = h.borrow().raised[0].args.clone();
    assert_eq!((ev[0].as_i64().unwrap(), ev[1].as_i64().unwrap()), (2, 3));
}

#[test]
fn an_edit_that_changes_nothing_raises_no_after_edit() {
    let h = host(true);
    h.borrow_mut().apply_edit(UserEdit::GridCell { control: 0, row: 2, col: 2, text: String::new() });
    assert!(handlers(&h).is_empty());
}

#[test]
fn a_read_only_grid_a_header_or_outside_refuses_an_edit() {
    let ro = host(false);
    ro.borrow_mut().apply_edit(UserEdit::GridCell { control: 0, row: 2, col: 2, text: "x".into() });
    assert_eq!(grid(&ro).cell(2, 2), Some(""));
    let h = host(true);
    for (r, c) in [(0, 2), (2, 0), (50, 1)] {
        h.borrow_mut().apply_edit(UserEdit::GridCell { control: 0, row: r, col: c, text: "x".into() });
    }
    assert!(handlers(&h).is_empty());
    assert_eq!(grid(&h).cell(0, 2), Some(""));
}

#[test]
fn a_grid_edit_reaches_an_open_windows_copy_of_the_form() {
    let h = host(true);
    let mut form = h.borrow().form.clone();
    let mut state = h.borrow().state.clone();
    h.borrow_mut().apply_edit(UserEdit::GridCell { control: 0, row: 2, col: 2, text: "seen".into() });
    let snap = h.borrow_mut().take_changes().unwrap();
    snap.apply_to(&mut form, &mut state);
    assert_eq!(form.controls[0].grid.cell(2, 2), Some("seen"));
}
