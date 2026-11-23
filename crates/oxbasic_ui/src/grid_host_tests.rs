//! DataGrid run-time API: `Rows`, `Cols`, `TextMatrix`, `Row`/`Col`/`Text`,
//! `ColWidth`/`RowHeight`, `Editable`, and the `AddItem`/`RemoveItem`/`Clear`
//! methods, run as BASIC against a form with a grid.

use crate::form::{ControlKind, ControlSpec, FormSpec};
use crate::host::tests::run;
use crate::host::FormHost;
use crate::Rect;
use std::cell::RefCell;
use std::rc::Rc;

/// Form1 with Grid1, a default 5 x 5 DataGrid.
fn host() -> Rc<RefCell<FormHost>> {
    let mut f = FormSpec::new("Form1", "Grid", 400.0, 300.0);
    f.control(ControlSpec::new(
        "Grid1",
        ControlKind::DataGrid,
        Rect::new(0.0, 0.0, 320.0, 140.0),
    ));
    Rc::new(RefCell::new(FormHost::new(f)))
}

fn prints(src: &str) -> String {
    run(src, &host()).unwrap()
}

fn fails(src: &str) -> String {
    run(src, &host()).unwrap_err()
}

#[test]
fn a_new_grid_reports_its_size_and_headers() {
    let out = prints(
        "Print Grid1.Rows; Grid1.Cols; Grid1.FixedRows; Grid1.FixedCols\n\
         Print Grid1.Editable; Grid1.ScrollBars\n",
    );
    assert_eq!(out, " 5  5  1  1 \nFalse 3 \n");
}

#[test]
fn text_matrix_reads_and_writes_a_cell() {
    let out = prints(
        "Grid1.TextMatrix(2, 3) = \"hello\"\n\
         Print Grid1.TextMatrix(2, 3)\n\
         Print Len(Grid1.TextMatrix(0, 0))\n",
    );
    assert_eq!(out, "hello\n 0 \n");
}

#[test]
fn text_matrix_outside_the_grid_is_error_381() {
    assert!(fails("Grid1.TextMatrix(5, 0) = \"x\"\n").contains("381"));
    assert!(fails("Print Grid1.TextMatrix(0, 5)\n").contains("381"));
    assert!(fails("Print Grid1.TextMatrix(-1, 0)\n").contains("381"));
}

#[test]
fn row_col_and_text_address_the_current_cell() {
    let out = prints(
        "Grid1.Row = 2\nGrid1.Col = 3\nGrid1.Text = \"here\"\n\
         Print Grid1.TextMatrix(2, 3); Grid1.Row; Grid1.Col\n",
    );
    assert_eq!(out, "here 2  3 \n");
    assert!(fails("Grid1.Row = 9\n").contains("381"));
}

#[test]
fn rows_and_cols_resize_keeping_the_cells() {
    let out = prints(
        "Grid1.TextMatrix(2, 2) = \"keep\"\nGrid1.Rows = 8\nGrid1.Cols = 3\n\
         Print Grid1.Rows; Grid1.Cols; Grid1.TextMatrix(2, 2)\n",
    );
    assert_eq!(out, " 8  3 keep\n");
    assert!(fails("Grid1.Rows = -1\n").contains("380"));
    assert!(fails("Grid1.Rows = 100001\n").contains("380"));
}

#[test]
fn fixed_rows_and_cols_cannot_exceed_the_size() {
    assert_eq!(prints("Grid1.FixedRows = 2\nPrint Grid1.FixedRows\n"), " 2 \n");
    assert!(fails("Grid1.FixedRows = 6\n").contains("380"));
    assert!(fails("Grid1.FixedCols = -1\n").contains("380"));
}

#[test]
fn editable_toggles_and_scroll_bars_are_range_checked() {
    let out = prints("Grid1.Editable = True\nPrint Grid1.Editable\nGrid1.ScrollBars = 1\nPrint Grid1.ScrollBars\n");
    assert_eq!(out, "True\n 1 \n");
    assert!(fails("Grid1.ScrollBars = 4\n").contains("380"));
}

#[test]
fn col_width_and_row_height_read_and_set() {
    let out = prints(
        "Grid1.ColWidth(1) = 120\nGrid1.RowHeight(2) = 30\n\
         Print Grid1.ColWidth(1); Grid1.RowHeight(2)\n",
    );
    assert_eq!(out, " 120  30 \n");
    assert!(fails("Grid1.ColWidth(9) = 5\n").contains("381"));
}

#[test]
fn add_item_remove_item_and_clear() {
    let out = prints(
        "Grid1.AddItem \"a\" & vbTab & \"b\"\n\
         Print Grid1.Rows; Grid1.TextMatrix(5, 0); Grid1.TextMatrix(5, 1)\n\
         Grid1.RemoveItem 5\nPrint Grid1.Rows\n\
         Grid1.TextMatrix(1, 1) = \"x\"\nGrid1.Clear\nPrint Len(Grid1.TextMatrix(1, 1))\n",
    );
    assert_eq!(out, " 6 ab\n 5 \n 0 \n");
    assert!(fails("Grid1.RemoveItem 0\n").contains("381"), "a fixed row stays");
    assert!(fails("Grid1.RemoveItem 9\n").contains("381"));
}

#[test]
fn top_row_and_left_col_scroll_within_the_grid() {
    let out = prints("Grid1.TopRow = 3\nGrid1.LeftCol = 2\nPrint Grid1.TopRow; Grid1.LeftCol\n");
    assert_eq!(out, " 3  2 \n");
    assert!(fails("Grid1.TopRow = 5\n").contains("381"));
    // a fixed row can't be scrolled to: it is raised to the first scrolling one
    assert_eq!(prints("Grid1.TopRow = 0\nPrint Grid1.TopRow\n"), " 1 \n");
}

#[test]
fn a_program_change_marks_the_grid_touched() {
    let h = host();
    run("Grid1.TextMatrix(1, 1) = \"x\"\n", &h).unwrap();
    assert!(h.borrow().touched[0]);
}

#[test]
fn an_unknown_member_is_error_438() {
    assert!(fails("Grid1.Frobnicate = 1\n").contains("438"));
    assert!(fails("Grid1.Frobnicate\n").contains("438"));
}

/// The program's cell reaches an open window's copy of the form: the whole
/// form is copied across, grid included.
#[test]
fn a_program_cell_change_reaches_an_open_window() {
    let h = host();
    let mut form = h.borrow().form.clone();
    let mut state = h.borrow().state.clone();
    run("Grid1.TextMatrix(2, 2) = \"seen\"\nGrid1.Row = 3\n", &h).unwrap();
    let snap = h.borrow_mut().take_changes().unwrap();
    snap.apply_to(&mut form, &mut state);
    assert_eq!(form.controls[0].grid.cell(2, 2), Some("seen"));
    assert_eq!(form.controls[0].grid.row, 3);
}
