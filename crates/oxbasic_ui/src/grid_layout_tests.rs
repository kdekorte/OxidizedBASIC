//! Tests for the DataGrid's layout: what is visible, when the scrollbars
//! appear, where their thumbs sit, and which cell a point is over.

use crate::grid::GridSpec;
use crate::grid_layout::{reveal, reveal_axis, GridLayout, BAR};

/// A grid of `rows` x `cols` where every column is 100 wide and every row 20
/// high, with one fixed row and column.
fn grid(rows: usize, cols: usize) -> GridSpec {
    let mut g = GridSpec::new(rows, cols);
    for c in 0..cols {
        g.set_col_width(c, 100.0);
    }
    for r in 0..rows {
        g.set_row_height(r, 20.0);
    }
    g
}

fn indexes(v: &[(usize, f32, f32)]) -> Vec<usize> {
    v.iter().map(|s| s.0).collect()
}

#[test]
fn a_grid_that_fits_shows_every_cell_and_no_bars() {
    let l = GridLayout::new(&grid(3, 3), 400.0, 200.0);
    assert_eq!(indexes(&l.rows), vec![0, 1, 2]);
    assert_eq!(indexes(&l.cols), vec![0, 1, 2]);
    assert!(!l.vbar && !l.hbar);
    assert_eq!((l.view_w, l.view_h), (400.0, 200.0));
    assert_eq!(l.rows[1], (1, 20.0, 20.0));
    assert_eq!(l.cols[2], (2, 200.0, 100.0));
}

#[test]
fn too_many_rows_adds_a_vertical_bar_and_narrows_the_view() {
    // 20 rows x 20 = 400 high in a 100 high view
    let l = GridLayout::new(&grid(20, 3), 400.0, 100.0);
    assert!(l.vbar && !l.hbar);
    assert_eq!(l.view_w, 400.0 - BAR);
    assert_eq!(indexes(&l.rows), vec![0, 1, 2, 3, 4]);
}

#[test]
fn a_bar_that_takes_room_can_make_the_other_bar_needed() {
    // 5 cols x 100 = 500 wide fits 510 only until the vertical bar takes 16
    let l = GridLayout::new(&grid(20, 5), 510.0, 100.0);
    assert!(l.vbar && l.hbar);
    assert_eq!((l.view_w, l.view_h), (510.0 - BAR, 100.0 - BAR));
}

#[test]
fn scroll_bars_setting_hides_a_bar_that_would_be_needed() {
    let mut g = grid(20, 3);
    g.scroll_bars = 0;
    let l = GridLayout::new(&g, 400.0, 100.0);
    assert!(!l.vbar && !l.hbar);
    g.scroll_bars = 1;
    assert!(!GridLayout::new(&g, 400.0, 100.0).vbar);
    g.scroll_bars = 2;
    assert!(GridLayout::new(&g, 400.0, 100.0).vbar);
}

#[test]
fn fixed_rows_stay_while_the_rest_scroll() {
    let mut g = grid(20, 3);
    g.top_row = 7;
    let l = GridLayout::new(&g, 400.0, 100.0);
    assert_eq!(indexes(&l.rows), vec![0, 7, 8, 9, 10]);
    assert_eq!(l.rows[1].1, 20.0, "row 7 sits just under the header");
}

#[test]
fn fixed_columns_stay_while_the_rest_scroll() {
    let mut g = grid(3, 10);
    g.left_col = 4;
    let l = GridLayout::new(&g, 300.0, 200.0);
    assert_eq!(indexes(&l.cols)[0], 0);
    assert_eq!(indexes(&l.cols)[1], 4);
}

#[test]
fn scrolling_stops_where_the_last_row_is_whole() {
    // 20 rows, 5 fit (100 high): the last scrolling start shows rows 15..19
    let l = GridLayout::new(&grid(20, 3), 400.0, 100.0);
    assert_eq!(l.max_top_row, 16, "header + rows 16..19 fill 100 high");
    let mut g = grid(20, 3);
    g.top_row = 19;
    let l = GridLayout::new(&g, 400.0, 100.0);
    assert_eq!(*indexes(&l.rows).last().unwrap(), 19, "the last row is reached");
    assert_eq!(indexes(&l.rows)[1], 16, "but the view stops at the last page");
}

#[test]
fn the_thumb_is_the_share_in_view_and_moves_with_the_scroll() {
    let mut g = grid(20, 3);
    let top = GridLayout::new(&g, 400.0, 100.0).vthumb.unwrap();
    assert_eq!(top.0, 0.0, "at the top the thumb is at the start");
    assert!(top.1 < 100.0 && top.1 >= 16.0);
    g.top_row = 19;
    let end = GridLayout::new(&g, 400.0, 100.0).vthumb.unwrap();
    assert!((end.0 + end.1 - 100.0).abs() < 0.01, "at the end it reaches the far edge");
    assert_eq!(end.1, top.1, "its length doesn't change");
}

#[test]
fn a_cell_is_found_from_a_point_and_a_bar_is_not_a_cell() {
    let mut g = grid(20, 3);
    g.top_row = 5;
    let l = GridLayout::new(&g, 400.0, 100.0);
    assert_eq!(l.cell_at(5.0, 5.0), Some((0, 0)), "the header corner");
    assert_eq!(l.cell_at(150.0, 25.0), Some((5, 1)), "first scrolling row");
    assert_eq!(l.cell_at(399.0, 25.0), None, "that is the scrollbar");
    assert_eq!(l.cell_at(5.0, 100.0), None, "below the view");
}

#[test]
fn a_zero_sized_grid_lays_out_without_panicking() {
    let g = GridSpec::new(0, 0);
    let l = GridLayout::new(&g, 100.0, 100.0);
    assert!(l.rows.is_empty() && l.cols.is_empty());
    assert!(!l.vbar && !l.hbar);
    GridLayout::new(&grid(5, 5), 0.0, 0.0);
}

#[test]
fn a_target_in_view_leaves_the_view_alone() {
    let sizes = vec![20.0; 20];
    // 5 rows of 20 fit in 100: with the header, rows 1..=4 are in view
    assert_eq!(reveal_axis(&sizes, 1, 1, 100.0, 3), 1);
    assert_eq!(reveal_axis(&sizes, 1, 1, 100.0, 4), 1);
}

#[test]
fn a_target_below_the_view_scrolls_down_just_enough() {
    let sizes = vec![20.0; 20];
    assert_eq!(reveal_axis(&sizes, 1, 1, 100.0, 5), 2, "one row down");
    assert_eq!(reveal_axis(&sizes, 1, 1, 100.0, 10), 7);
}

#[test]
fn a_target_above_the_view_scrolls_up_to_it() {
    let sizes = vec![20.0; 20];
    assert_eq!(reveal_axis(&sizes, 1, 9, 100.0, 4), 4);
}

#[test]
fn a_fixed_target_never_scrolls_and_the_end_stops_at_the_last_page() {
    let sizes = vec![20.0; 20];
    assert_eq!(reveal_axis(&sizes, 1, 9, 100.0, 0), 9, "header is always in view");
    assert_eq!(reveal_axis(&sizes, 1, 1, 100.0, 19), 16, "the last page");
}

#[test]
fn reveal_moves_both_axes_and_says_whether_it_did() {
    let mut g = grid(20, 10);
    g.select(15, 8);
    assert!(reveal(&mut g, 300.0, 100.0));
    let l = GridLayout::new(&g, 300.0, 100.0);
    assert!(l.rows.iter().any(|s| s.0 == 15), "row 15 is in view");
    assert!(l.cols.iter().any(|s| s.0 == 8), "col 8 is in view");
    assert!(!reveal(&mut g, 300.0, 100.0), "already in view: nothing moves");
}

#[test]
fn reveal_in_a_grid_that_fits_does_nothing() {
    let mut g = grid(3, 3);
    g.select(2, 2);
    assert!(!reveal(&mut g, 400.0, 200.0));
}


