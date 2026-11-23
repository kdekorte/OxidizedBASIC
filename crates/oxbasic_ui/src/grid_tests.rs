use crate::grid::*;
use crate::{ControlKind, ControlSpec, Rect};

#[test]
fn a_new_grid_is_five_by_five_with_one_header_row_and_column() {
    let g = GridSpec::default();
    assert_eq!((g.rows, g.cols), (5, 5));
    assert_eq!((g.fixed_rows, g.fixed_cols), (1, 1));
    assert_eq!((g.row, g.col), (1, 1));
    assert!(!g.editable);
    assert_eq!(g.cell(4, 4), Some(""));
    assert_eq!(g.cell(5, 0), None);
}

#[test]
fn resizing_keeps_the_cells_that_still_fit() {
    let mut g = GridSpec::default();
    g.set_cell(2, 3, "keep");
    g.set_cell(4, 4, "lost");
    g.resize(3, 4);
    assert_eq!(g.cell(2, 3), Some("keep"));
    assert_eq!(g.cell(4, 4), None);
    assert_eq!((g.col_widths.len(), g.row_heights.len()), (4, 3));
    g.resize(6, 6);
    assert_eq!(g.cell(2, 3), Some("keep"));
    assert_eq!(g.cell(5, 5), Some(""));
}

#[test]
fn shrinking_pulls_the_fixed_counts_and_current_cell_inside() {
    let mut g = GridSpec::default();
    g.set_fixed_rows(3);
    g.select(4, 4);
    g.resize(2, 2);
    assert_eq!((g.fixed_rows, g.fixed_cols), (2, 1));
    assert_eq!((g.row, g.col), (1, 1));
}

#[test]
fn only_non_header_cells_of_an_editable_grid_can_be_edited() {
    let mut g = GridSpec::default();
    assert!(!g.can_edit(2, 2));
    g.editable = true;
    assert!(g.can_edit(2, 2));
    assert!(!g.can_edit(0, 2));
    assert!(!g.can_edit(2, 0));
    assert!(!g.can_edit(9, 9));
}

#[test]
fn selecting_and_setting_outside_the_grid_changes_nothing() {
    let mut g = GridSpec::default();
    assert!(!g.select(5, 0));
    assert!(!g.set_cell(0, 5, "x"));
    assert_eq!((g.row, g.col), (1, 1));
}

#[test]
fn add_item_appends_a_row_split_on_tabs() {
    let mut g = GridSpec::new(2, 3);
    g.add_item("a\tb");
    assert_eq!(g.rows, 3);
    assert_eq!(
        (g.cell(2, 0), g.cell(2, 1), g.cell(2, 2)),
        (Some("a"), Some("b"), Some(""))
    );
    g.add_item("1\t2\t3\t4");
    assert_eq!(g.cell(3, 2), Some("3"));
}

#[test]
fn remove_item_deletes_a_row_but_not_a_fixed_one() {
    let mut g = GridSpec::new(4, 2);
    g.set_cell(2, 0, "two");
    g.set_cell(3, 0, "three");
    assert!(!g.remove_item(0));
    assert!(!g.remove_item(9));
    assert!(g.remove_item(2));
    assert_eq!(g.rows, 3);
    assert_eq!(g.cell(2, 0), Some("three"));
    assert_eq!(g.row_heights.len(), 3);
}

#[test]
fn clear_empties_every_cell_and_keeps_the_size() {
    let mut g = GridSpec::default();
    g.set_cell(0, 0, "h");
    g.set_cell(3, 3, "x");
    g.clear();
    assert_eq!(g.cell(0, 0), Some(""));
    assert_eq!(g.cell(3, 3), Some(""));
    assert_eq!((g.rows, g.cols), (5, 5));
}

#[test]
fn fixed_counts_cannot_exceed_the_size() {
    let mut g = GridSpec::default();
    assert!(!g.set_fixed_rows(6));
    assert!(g.set_fixed_rows(5));
    assert!(!g.set_fixed_cols(6));
}

#[test]
fn widths_and_heights_read_and_set() {
    let mut g = GridSpec::default();
    assert_eq!(g.col_width(0), Some(DEFAULT_COL_WIDTH));
    assert!(g.set_col_width(1, 120.0));
    assert_eq!(g.col_width(1), Some(120.0));
    assert!(!g.set_col_width(9, 5.0));
    assert!(g.set_row_height(2, -4.0));
    assert_eq!(g.row_height(2), Some(0.0));
    assert_eq!(g.row_height(9), None);
}

#[test]
fn a_grid_round_trips_through_json() {
    let mut g = GridSpec::new(3, 3);
    g.editable = true;
    g.set_cell(1, 1, "hi");
    let json = serde_json::to_string(&g).unwrap();
    let back: GridSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(back, g);
    let bare: GridSpec = serde_json::from_str("{}").unwrap();
    assert_eq!(bare, GridSpec::default());
}

#[test]
fn a_datagrid_control_carries_its_grid_through_oxform_json() {
    assert_eq!(
        ControlKind::from_basic_name("DataGrid"),
        Some(ControlKind::DataGrid)
    );
    assert_eq!(ControlKind::DataGrid.basic_name(), "DataGrid");
    let mut c = ControlSpec::new("Grid1", ControlKind::DataGrid, Rect::new(0.0, 0.0, 320.0, 160.0));
    c.grid.editable = true;
    c.grid.set_cell(2, 2, "x");
    let json = serde_json::to_string(&c).unwrap();
    let back: ControlSpec = serde_json::from_str(&json).unwrap();
    assert!(back.grid.editable);
    assert_eq!(back.grid.cell(2, 2), Some("x"));
    // A control that is not a grid doesn't write one.
    let b = ControlSpec::new("B", ControlKind::CommandButton, Rect::new(0.0, 0.0, 9.0, 9.0));
    assert!(!serde_json::to_string(&b).unwrap().contains("grid"));
}
