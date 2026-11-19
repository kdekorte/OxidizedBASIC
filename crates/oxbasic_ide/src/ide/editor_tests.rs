//! Code-editor tests in the real code window: selecting with Shift and the
//! mouse, the clipboard, and undo/redo, driven by keystrokes and clicks.

use super::*;

/// The IDE with `text` in its code window, which has the keyboard focus;
/// returns the code window's own test context.
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
    // a second frame, so the scroll handle knows where the rows are
    ide.update(&mut code, |_, cx| cx.notify());
    code.run_until_parked();
    // click the first row to give the editor the focus, then go home
    let row = code.debug_bounds("editor-row-0").expect("row 0");
    code.simulate_click(row.center(), gpui::Modifiers::none());
    code.simulate_keystrokes("cmd-up");
    (ide, code)
}

fn text(ide: &Entity<Ide>, cx: &mut gpui::VisualTestContext) -> String {
    ide.read_with(cx, |ide, _| ide.lines.join("\n"))
}

fn selected(ide: &Entity<Ide>, cx: &mut gpui::VisualTestContext) -> Option<String> {
    ide.read_with(cx, |ide, _| ide.selected_text())
}

#[gpui::test]
fn shift_arrows_select_across_lines_and_typing_replaces_it(cx: &mut gpui::TestAppContext) {
    let (ide, mut cx) = editor_with("Sub Main()\n  Print 1\nEnd Sub", cx);
    let cx = &mut cx;
    cx.simulate_keystrokes("shift-right shift-right shift-right shift-right");
    assert_eq!(selected(&ide, cx).as_deref(), Some("Sub "));
    cx.simulate_keystrokes("shift-down");
    assert_eq!(selected(&ide, cx).as_deref(), Some("Sub Main()\n  Pr"));
    // typing replaces the selection with what was typed
    cx.simulate_input("X");
    assert_eq!(text(&ide, cx), "Xint 1\nEnd Sub");
    assert_eq!(selected(&ide, cx), None);
    // a plain arrow after a selection collapses it instead of moving
    cx.simulate_keystrokes("shift-end left");
    ide.read_with(cx, |ide, _| assert_eq!(ide.cursor, (0, 1)));
}

#[gpui::test]
fn backspace_and_delete_remove_the_whole_selection(cx: &mut gpui::TestAppContext) {
    let (ide, mut cx) = editor_with("one\ntwo\nthree", cx);
    let cx = &mut cx;
    cx.simulate_keystrokes("right shift-down shift-down");
    cx.simulate_keystrokes("backspace");
    assert_eq!(text(&ide, cx), "ohree");
    cx.simulate_keystrokes("shift-end delete");
    assert_eq!(text(&ide, cx), "o");
}

#[gpui::test]
fn cut_copy_and_paste_go_through_the_clipboard(cx: &mut gpui::TestAppContext) {
    let (ide, mut cx) = editor_with("Dim A\nDim B", cx);
    let cx = &mut cx;
    // copy the first line with its line break, paste it at the end
    cx.simulate_keystrokes("shift-down cmd-c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("Dim A\n")
    );
    cx.simulate_keystrokes("cmd-down enter cmd-v");
    assert_eq!(text(&ide, cx), "Dim A\nDim B\nDim A\n");
    // cut the whole text: the editor is left with one empty line
    cx.simulate_keystrokes("cmd-a cmd-x");
    assert_eq!(text(&ide, cx), "");
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("Dim A\nDim B\nDim A\n")
    );
    // text from elsewhere pastes too, Windows line breaks and all
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("Sub X()\r\nEnd Sub".into()));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(text(&ide, cx), "Sub X()\nEnd Sub");
    ide.read_with(cx, |ide, _| assert_eq!(ide.cursor, (1, 7)));
}

#[gpui::test]
fn undo_and_redo_step_through_the_edits(cx: &mut gpui::TestAppContext) {
    let (ide, mut cx) = editor_with("x", cx);
    let cx = &mut cx;
    cx.simulate_keystrokes("end");
    // a word of typing, a new line, more typing: three steps
    cx.simulate_input("yz");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("Print");
    assert_eq!(text(&ide, cx), "xyz\nPrint");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&ide, cx), "xyz\n", "the word typed undoes as one step");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&ide, cx), "xyz");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&ide, cx), "x");
    ide.read_with(cx, |ide, _| assert_eq!(ide.cursor, (0, 1)));
    // nothing more to undo
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&ide, cx), "x");
    // redo all the way forward again
    cx.simulate_keystrokes("cmd-shift-z cmd-shift-z cmd-shift-z");
    assert_eq!(text(&ide, cx), "xyz\nPrint");
    // a cut undoes too
    cx.simulate_keystrokes("cmd-a cmd-x");
    assert_eq!(text(&ide, cx), "");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&ide, cx), "xyz\nPrint");
}

fn mouse_down(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>, clicks: usize) {
    cx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::none(),
        click_count: clicks,
        first_mouse: false,
    });
}

fn mouse_up(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_event(gpui::MouseUpEvent {
        button: gpui::MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::none(),
        click_count: 1,
    });
}

fn mouse_move(
    cx: &mut gpui::VisualTestContext,
    at: gpui::Point<gpui::Pixels>,
    held: Option<gpui::MouseButton>,
) {
    cx.simulate_event(gpui::MouseMoveEvent {
        position: at,
        pressed_button: held,
        modifiers: gpui::Modifiers::none(),
    });
}

/// A click of `clicks` (1 = single, 2 = double, ...) at `at`.
fn press(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>, clicks: usize) {
    mouse_down(cx, at, clicks);
    mouse_up(cx, at);
}

#[gpui::test]
fn dragging_selects_and_double_and_triple_clicks_pick_a_word_and_a_line(
    cx: &mut gpui::TestAppContext,
) {
    let (ide, mut cx) = editor_with("Label1.Caption = x\nEnd Sub\nLast", cx);
    let cx = &mut cx;
    let row0 = cx.debug_bounds("editor-row-0").unwrap();
    let row1 = cx.debug_bounds("editor-row-1").unwrap();
    let at = |row: gpui::Bounds<gpui::Pixels>, dx: f32| {
        gpui::point(row.origin.x + px(CODE_X + dx), row.center().y)
    };
    // from the start of the code on row 0, dragged down into row 1
    let start = at(row0, 1.0);
    let end = at(row1, 40.0);
    mouse_down(cx, start, 1);
    mouse_move(cx, end, Some(gpui::MouseButton::Left));
    mouse_up(cx, end);
    let s = selected(&ide, cx).expect("a drag selects");
    assert!(s.starts_with("Label1.Caption = x\nEnd"), "selected {:?}", s);
    // moving afterwards, button up, leaves the selection alone
    mouse_move(cx, start, None);
    assert_eq!(selected(&ide, cx), Some(s));

    // a double click on "Caption" selects the word, a triple the line
    let caption = at(row0, 70.0);
    press(cx, caption, 1);
    press(cx, caption, 2);
    assert_eq!(selected(&ide, cx).as_deref(), Some("Caption"));
    press(cx, caption, 3);
    assert_eq!(selected(&ide, cx).as_deref(), Some("Label1.Caption = x\n"));

    // Shift-click extends from the caret to the click
    press(cx, start, 1);
    let last = cx.debug_bounds("editor-row-2").unwrap();
    cx.simulate_click(at(last, 400.0), gpui::Modifiers::shift());
    assert_eq!(
        selected(&ide, cx).as_deref(),
        Some("Label1.Caption = x\nEnd Sub\nLast")
    );
}

#[gpui::test]
fn switching_documents_forgets_the_selection_and_the_undo_history(cx: &mut gpui::TestAppContext) {
    let (ide, mut cx) = editor_with("first", cx);
    let cx = &mut cx;
    cx.simulate_input("abc");
    cx.simulate_keystrokes("cmd-a");
    ide.read_with(cx, |ide, _| assert!(ide.selection().is_some()));
    ide.update(cx, |ide, cx| ide.add_module(cx));
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.selection(), None);
        assert!(!ide.history.can_undo(), "the new module has no history");
    });
}

#[gpui::test]
fn an_event_placeholder_from_the_event_box_undoes_in_one_step(cx: &mut gpui::TestAppContext) {
    let (ide, mut cx) = editor_with("' top", cx);
    let cx = &mut cx;
    ide.update(cx, |ide, cx| {
        ide.form.button(
            "Command1",
            "OK",
            oxbasic_ui::Rect::new(10.0, 10.0, 80.0, 24.0),
        );
        ide.pick_event("Command1", "Click");
        cx.notify();
    });
    assert!(text(&ide, cx).contains("Private Sub Command1_Click()"));
    // typing into the new handler, then undoing twice: back to the start
    cx.simulate_input("Beep");
    cx.simulate_keystrokes("cmd-z");
    assert!(
        text(&ide, cx).contains("Command1_Click"),
        "the typing first"
    );
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&ide, cx), "' top", "then the placeholder");
}
