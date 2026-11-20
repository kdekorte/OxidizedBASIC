//! The bars between the main window's panes: dragging one resizes the pane
//! beside it in the real layout, a pane stops at its limits, and the
//! Properties labels fit on one line in their smaller size.

use super::splitters::Split;
use super::toolbox_tests::{drag_by, ide_window, sel};
use super::*;

fn bounds(cx: &mut gpui::VisualTestContext, id: &str) -> gpui::Bounds<gpui::Pixels> {
    cx.debug_bounds(sel(id.to_string()))
        .unwrap_or_else(|| panic!("no {}", id))
}

/// Drag bar `id` by `(dx, dy)` and let the window redraw.
fn drag_bar(cx: &mut gpui::VisualTestContext, id: &str, dx: f32, dy: f32) {
    let b = bounds(cx, id);
    drag_by(cx, b.center(), dx, dy);
    cx.run_until_parked();
}

#[gpui::test]
fn the_immediate_window_grows_when_its_bar_is_dragged_up(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let before = ide.read_with(cx, |ide, _| ide.panes.output_h);
    drag_bar(cx, "split-output", 0.0, -120.0);
    let after = ide.read_with(cx, |ide, _| ide.panes.output_h);
    assert_eq!(
        after,
        before + 120.0,
        "dragging up by 120 gives it 120 more"
    );
    // and down again shrinks it
    drag_bar(cx, "split-output", 0.0, 50.0);
    assert_eq!(ide.read_with(cx, |ide, _| ide.panes.output_h), after - 50.0);
}

#[gpui::test]
fn the_project_and_properties_column_widens_when_its_bar_is_dragged_left(
    cx: &mut gpui::TestAppContext,
) {
    let (ide, cx) = ide_window(cx);
    let before = ide.read_with(cx, |ide, _| ide.panes.side_w);
    drag_bar(cx, "split-side", -100.0, 0.0);
    assert_eq!(ide.read_with(cx, |ide, _| ide.panes.side_w), before + 100.0);
    // the Properties fields really are wider in the layout
    let field = bounds(cx, "prop-name");
    assert!(
        f32::from(field.size.width) > 250.0,
        "the (Name) field is {} wide",
        f32::from(field.size.width)
    );
}

#[gpui::test]
fn the_toolbox_and_the_project_panel_have_bars_too(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let (tw, ph) = ide.read_with(cx, |ide, _| (ide.panes.toolbox_w, ide.panes.project_h));
    drag_bar(cx, "split-toolbox", 40.0, 0.0);
    drag_bar(cx, "split-project", 0.0, 60.0);
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.panes.toolbox_w, tw + 40.0);
        assert_eq!(ide.panes.project_h, ph + 60.0);
    });
}

#[gpui::test]
fn a_pane_stops_at_its_limit(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    // drag the Immediate window's bar to the bottom edge of the window
    let bar = bounds(cx, "split-output");
    let view = ide.read_with(cx, |ide, _| ide.viewport());
    let to_bottom = view.1 - 2.0 - f32::from(bar.center().y);
    drag_by(cx, bar.center(), 0.0, to_bottom);
    cx.run_until_parked();
    let h = ide.read_with(cx, |ide, _| ide.panes.output_h);
    assert_eq!(h, 56.0, "it keeps its title in sight");
    assert!(
        cx.debug_bounds("split-output").is_some(),
        "its bar can still be reached"
    );
    // and to the top edge: never the whole window
    let bar = bounds(cx, "split-output");
    drag_by(cx, bar.center(), 0.0, 2.0 - f32::from(bar.center().y));
    cx.run_until_parked();
    let h = ide.read_with(cx, |ide, _| ide.panes.output_h);
    assert_eq!(h, splitters::limits(Split::Output, view).1);
    assert!(h < view.1, "{} of {}", h, view.1);
}

#[gpui::test]
fn property_labels_are_smaller_and_on_one_line(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    // a text box has the long names: ToolTipText, MousePointer, Font.Name
    ide.update(cx, |ide, cx| {
        ide.add_control(ControlKind::TextBox);
        cx.notify();
    });
    cx.run_until_parked();
    let field = bounds(cx, "prop-tooltiptext");
    let name = bounds(cx, "prop-name");
    assert_eq!(
        field.size.height, name.size.height,
        "the ToolTipText row is no taller than (Name): its label fits one line"
    );
    // every field starts at the same x, after the label column
    for p in ["prop-mousepointer", "prop-tooltiptext", "prop-left"] {
        assert_eq!(bounds(cx, p).origin.x, name.origin.x, "{}", p);
    }
}
