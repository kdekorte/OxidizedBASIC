//! Syntax diagnostics in the real code window: a line that does not parse
//! is underlined, and its message shows in the status line while the caret
//! is on it.

use super::*;

/// The IDE with `text` in its code window (as the editor tests set it up).
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

#[gpui::test]
fn a_bad_line_is_found_and_drawn(cx: &mut gpui::TestAppContext) {
    // line 2 has an unclosed parenthesis
    let (ide, mut cx) = editor_with("Print 1\nPrint (2\nPrint 3", cx);
    let cx = &mut cx;
    ide.read_with(cx, |ide, _| {
        let d = code_model::diagnostic(&ide.lines).expect("an error on row 1");
        assert_eq!(d.row, 1);
    });
    // the bad row is drawn like the others
    assert!(cx.debug_bounds("editor-row-1").is_some());
}

#[gpui::test]
fn a_clean_module_has_no_diagnostic(cx: &mut gpui::TestAppContext) {
    let (ide, mut cx) = editor_with("Sub A()\n  Print 1\nEnd Sub", cx);
    let cx = &mut cx;
    ide.read_with(cx, |ide, _| {
        assert_eq!(code_model::diagnostic(&ide.lines), None);
    });
}
