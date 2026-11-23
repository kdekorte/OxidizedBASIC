//! The DataGrid as drawn in a live window: the cells in view, the fixed
//! header staying put while the body scrolls, and the scrollbars showing
//! only when the cells don't fit.

use super::toolbox_tests::{ide_window, open_live_window, settle};
use super::*;
use oxbasic_ui::ControlKind as K;

/// Form1 with Grid1 (20 rows x 5 cols, ScrollBars both) in a 300 x 120 box.
fn grid_form(top_row: usize) -> FormSpec {
    let mut f = FormSpec::new("Form1", "Grid", 400.0, 300.0);
    let mut g = ControlSpec::new("Grid1", K::DataGrid, Rect::new(10.0, 10.0, 300.0, 120.0));
    g.grid = oxbasic_ui::grid::GridSpec::new(20, 5);
    g.grid.set_cell(0, 1, "Name");
    g.grid.set_cell(7, 1, "Seven");
    g.grid.top_row = top_row;
    f.control(g);
    f
}

#[gpui::test]
fn the_visible_cells_are_drawn_and_the_far_ones_are_not(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let (form, session, rx) = interactive_grid_session();
    let (_handle, mut win) = open_live_window(&ide, form, &session, &rx, cx);
    settle(&ide, &rx, &mut win, cx);
    // the header and the first body row are drawn
    assert!(win.debug_bounds("Grid1-cell-0-1").is_some(), "header drawn");
    assert!(win.debug_bounds("Grid1-cell-1-1").is_some(), "row 1 drawn");
    // row 19 is far below the 120 pixel view and is not made into an element
    assert!(win.debug_bounds("Grid1-cell-19-1").is_none(), "row 19 not drawn");
    assert!(win.debug_bounds("run-Grid1").is_some(), "the grid drawn");
}

#[gpui::test]
fn the_header_stays_while_the_body_scrolls(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let (form, session, rx) = interactive_grid_session();
    let (_handle, mut win) = open_live_window(&ide, form, &session, &rx, cx);
    settle(&ide, &rx, &mut win, cx);
    let header = win.debug_bounds("Grid1-cell-0-1").expect("header drawn");
    let row3 = win.debug_bounds("Grid1-cell-3-1").expect("row 3 drawn");
    // the header is above the body: scrolling the body never moves it
    assert!(header.origin.y < row3.origin.y);
    assert!(header.origin.y < px(20.0), "the header is at the top of the grid");
}

#[gpui::test]
fn the_grid_draws_a_scrollbar_when_the_cells_do_not_fit(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let (form, session, rx) = interactive_grid_session();
    let (_handle, mut win) = open_live_window(&ide, form, &session, &rx, cx);
    settle(&ide, &rx, &mut win, cx);
    // 20 rows of 22 pixels cannot fit in 120 pixels: a vertical bar is drawn.
    // 5 columns of 80 are 400 wide, more than the 300 less the bar: so is a
    // horizontal one
    assert!(win.debug_bounds("Grid1-vbar").is_some(), "vertical bar drawn");
    assert!(win.debug_bounds("Grid1-hbar").is_some(), "horizontal bar drawn");
}

/// `TopRow` set by the program scrolls the open window: row 7 sits just under
/// the header, and rows above it are gone.
#[gpui::test]
fn a_programs_top_row_scrolls_the_open_window(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let (form, session, rx) = interactive_grid_session();
    let (_handle, mut win) = open_live_window(&ide, form, &session, &rx, cx);
    settle(&ide, &rx, &mut win, cx);
    assert!(win.debug_bounds("Grid1-cell-1-1").is_some(), "row 1 shows at first");
    session.send(DebugCommand::Call(0, "ScrollDown".into(), vec![]));
    settle(&ide, &rx, &mut win, cx);
    // `debug_bounds` reads the last drawn frame: draw the one the change asked for
    win.update(|window, cx| {
        window.draw(cx).clear();
    });
    win.run_until_parked();
    assert!(win.debug_bounds("Grid1-cell-7-1").is_some(), "row 7 shows after the scroll");
    // (`debug_bounds` keeps entries of earlier frames, so a scrolled-away row
    // can't be shown absent: the position below is the proof it scrolled)
    let header = win.debug_bounds("Grid1-cell-0-1").expect("the header stays");
    let row7 = win.debug_bounds("Grid1-cell-7-1").unwrap();
    assert_eq!(
        row7.origin.y - header.origin.y,
        header.size.height,
        "row 7 sits directly under the header"
    );
}


/// An interactive session with `grid_form`, for the window tests.
fn interactive_grid_session() -> (FormSpec, DebugSession, std::sync::mpsc::Receiver<DebugEvent>) {
    let form = grid_form(0);
    let (tx, rx) = std::sync::mpsc::channel();
    let session = DebugSession::start_with_form(
        "Sub Grid1_Click()\nEnd Sub\nSub ScrollDown()\n  Grid1.TopRow = 7\nEnd Sub\n".to_string(),
        Some(form.clone()),
        Arc::new(Mutex::new(HashSet::new())),
        tx,
    );
    session.send(DebugCommand::Run);
    (form, session, rx)
}
