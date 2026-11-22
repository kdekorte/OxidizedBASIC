//! Code folding in the real code window: a block's marker hides its body
//! and shows again on a second click, and the caret never sits on a hidden
//! row.

use super::*;

/// The IDE with `text` in its code window, ready for clicks (as the editor
/// tests set it up).
fn editor_with(
    text: &str,
    cx: &mut gpui::TestAppContext,
) -> (Entity<Ide>, gpui::VisualTestContext) {
    let (ide, cx) = cx.add_window_view(|_, cx| Ide::new(None, None, cx));
    cx.simulate_resize(size(px(1200.0), px(800.0)));
    ide.update(cx, |ide, cx| {
        ide.lines = text.split('\n').map(String::from).collect();
        ide.cursor = (0, 0);
        ide.show_code_window(cx);
    });
    cx.run_until_parked();
    let handle = ide
        .read_with(cx, |ide, _| ide.code_window)
        .expect("code window");
    let mut code = gpui::VisualTestContext::from_window(handle, cx);
    code.simulate_resize(size(px(900.0), px(640.0)));
    code.run_until_parked();
    ide.update(&mut code, |_, cx| cx.notify());
    code.run_until_parked();
    (ide, code)
}

/// Click the fold marker of `row`, then wait for the frame it causes to be
/// drawn: `debug_bounds` reads the last drawn frame, so a read taken before
/// that frame would still see the old rows.
fn click_fold(row: usize, cx: &mut gpui::VisualTestContext) {
    let marker = cx
        .debug_bounds(Box::leak(format!("fold-{}", row).into_boxed_str()))
        .expect("fold marker");
    cx.simulate_click(marker.center(), gpui::Modifiers::none());
    settle(cx);
}

/// Let the window draw what its state now says, and draw it.
fn settle(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_fold_marker_hides_the_body_and_a_second_click_shows_it(cx: &mut gpui::TestAppContext) {
    let (ide, mut cx) = editor_with("Sub Main()\n  Print 1\n  Print 2\nEnd Sub", cx);
    let cx = &mut cx;
    let before = cx.debug_bounds("editor-row-3").expect("End Sub drawn");
    click_fold(0, cx);
    ide.read_with(cx, |ide, _| assert!(ide.folded.contains(&0)));
    // the closing line moves up over the folded body: the layout really changed
    let after = cx.debug_bounds("editor-row-3").expect("End Sub drawn");
    assert!(after.origin.y < before.origin.y, "End Sub should move up: {:?} -> {:?}", before, after);
    // a second click opens the block again, and End Sub moves back down
    click_fold(0, cx);
    ide.read_with(cx, |ide, _| assert!(!ide.folded.contains(&0)));
    let reopened = cx.debug_bounds("editor-row-3").expect("End Sub drawn");
    assert_eq!(reopened.origin.y, before.origin.y);
}

#[gpui::test]
fn the_caret_is_never_left_on_a_hidden_row(cx: &mut gpui::TestAppContext) {
    let (ide, mut cx) = editor_with("Sub Main()\n  Print 1\nEnd Sub", cx);
    let cx = &mut cx;
    let open = cx.debug_bounds("editor-row-2").expect("End Sub drawn");
    click_fold(0, cx);
    ide.read_with(cx, |ide, _| assert!(ide.folded.contains(&0)));
    // the caret is moved into the folded body: the block opens to show it
    ide.update(cx, |ide, cx| {
        ide.cursor = (1, 0);
        cx.notify();
    });
    settle(cx);
    ide.read_with(cx, |ide, _| assert!(!ide.folded.contains(&0)));
    let reopened = cx.debug_bounds("editor-row-2").expect("End Sub drawn");
    assert_eq!(reopened.origin.y, open.origin.y);
}
