//! The find bar in the code window: opening it with ⌘F and ⌥⌘F, typing a
//! search, stepping through the matches, replacing one and all, and closing
//! it. Each test works on the real code window with keystrokes.

use super::toolbox_tests::{ide_window, sel};
use super::*;

/// The code window opened by the IDE, holding `text`, as a test context.
fn code_window_with(
    ide: &Entity<Ide>,
    cx: &mut gpui::VisualTestContext,
    text: &str,
) -> gpui::VisualTestContext {
    // the same shortcuts a user has (the app registers them at start-up)
    cx.update(|_, app| bind_shortcuts(app));
    ide.update(cx, |ide, _| {
        ide.lines = text.split('\n').map(|l| l.to_string()).collect();
        ide.cursor = (0, 0);
    });
    ide.update(cx, |ide, cx| ide.view_code(cx));
    cx.run_until_parked();
    let handle = ide
        .read_with(cx, |ide, _| ide.code_window)
        .expect("the code window must be open");
    let mut code = gpui::VisualTestContext::from_window(handle, cx);
    code.simulate_resize(size(px(900.0), px(640.0)));
    code.run_until_parked();
    // click the first row to give the editor the focus, as a user would
    let row = code.debug_bounds("editor-row-0").expect("row 0");
    code.simulate_click(row.center(), gpui::Modifiers::none());
    code.run_until_parked();
    code
}

#[gpui::test]
fn cmd_f_opens_the_find_bar_and_typing_finds_the_matches(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let mut code = code_window_with(&ide, cx, "Print a\nprint b\nno print");
    ide.read_with(cx, |ide, _| assert!(ide.find.is_none(), "closed at first"));
    code.simulate_keystrokes("cmd-f");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        let bar = ide.find.as_ref().expect("⌘F opens the bar");
        assert!(!bar.replacing, "Find only");
    });
    assert!(
        code.debug_bounds(sel("find-bar".into())).is_some(),
        "the bar is drawn"
    );
    code.simulate_input("print");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        let bar = ide.find.as_ref().unwrap();
        assert_eq!(bar.found.len(), 3, "three matches");
        // the search starts at the caret (the end of the first line), so the
        // first match it finds is the second "print", as in any find bar
        assert_eq!(bar.current, Some(1), "the first match after the caret");
        assert_eq!(
            ide.selected_text().as_deref(),
            Some("print"),
            "the second print"
        );
    });
}

#[gpui::test]
fn next_and_previous_step_through_the_matches(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let mut code = code_window_with(&ide, cx, "x y x y x");
    code.simulate_keystrokes("cmd-f");
    code.simulate_input("x");
    code.run_until_parked();
    code.simulate_keystrokes("cmd-g");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        // typing selects the last x (the caret is at the end of the line),
        // so ⌘G wraps to the first one
        assert_eq!(
            ide.find.as_ref().unwrap().current,
            Some(0),
            "⌘G wraps to the first"
        );
        assert_eq!(ide.selected_text().as_deref(), Some("x"));
    });
    code.simulate_keystrokes("cmd-shift-g");
    code.simulate_keystrokes("cmd-shift-g");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        // from match 0, two steps back wrap round: 0 -> 2 -> 1
        assert_eq!(
            ide.find.as_ref().unwrap().current,
            Some(1),
            "⇧⌘G steps back, wrapping"
        );
    });
}

#[gpui::test]
fn the_selection_becomes_the_search_text(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let mut code = code_window_with(&ide, cx, "Label1.Caption\nLabel1 again");
    ide.update(cx, |ide, _| {
        ide.anchor = Some((0, 0));
        ide.cursor = (0, 6);
    });
    code.simulate_keystrokes("cmd-f");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        let bar = ide.find.as_ref().unwrap();
        assert_eq!(bar.find, "Label1", "the selected word is the search");
        assert_eq!(bar.found.len(), 2);
    });
}

#[gpui::test]
fn replace_all_is_one_undo_step(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let _code = code_window_with(&ide, cx, "a a a");
    ide.update(cx, |ide, _| {
        ide.find = Some(find_bar::FindBar {
            find: "a".into(),
            replace: "b".into(),
            match_case: true,
            ..Default::default()
        });
        ide.refresh_matches();
    });
    ide.update(cx, |ide, _| {
        assert_eq!(ide.replace_everything(), 3);
        assert_eq!(ide.lines, vec!["b b b".to_string()]);
        ide.undo();
        assert_eq!(
            ide.lines,
            vec!["a a a".to_string()],
            "one undo brings it all back"
        );
    });
}

#[gpui::test]
fn escape_closes_the_bar_and_returns_to_the_text(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let mut code = code_window_with(&ide, cx, "abc");
    code.simulate_keystrokes("cmd-f");
    code.run_until_parked();
    code.simulate_keystrokes("escape");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| assert!(ide.find.is_none(), "Escape closes it"));
    assert!(code.debug_bounds(sel("find-bar".into())).is_none());
}

#[gpui::test]
fn no_matches_leaves_nothing_selected(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let mut code = code_window_with(&ide, cx, "abc");
    code.simulate_keystrokes("cmd-f");
    code.simulate_input("zzz");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        let bar = ide.find.as_ref().unwrap();
        assert!(bar.found.is_empty());
        assert_eq!(bar.current, None);
        assert_eq!(ide.selected_text(), None, "a miss selects nothing");
    });
}

#[gpui::test]
fn replace_one_changes_the_current_match_and_moves_on(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let _code = code_window_with(&ide, cx, "cat dog cat");
    ide.update(cx, |ide, _| {
        ide.find = Some(find_bar::FindBar {
            find: "cat".into(),
            replace: "bird".into(),
            match_case: true,
            replacing: true,
            ..Default::default()
        });
        ide.refresh_matches();
        ide.find_step(true);
    });
    ide.update(cx, |ide, _| {
        ide.replace_current();
        // the current match is the last "cat": the caret starts at the end
        // of the line, as with the find bar's search from the caret
        assert_eq!(
            ide.lines,
            vec!["cat dog bird".to_string()],
            "the current match changed"
        );
        let bar = ide.find.as_ref().unwrap();
        assert_eq!(bar.found.len(), 1, "one match left");
    });
}

#[gpui::test]
fn the_matches_are_tinted_and_the_current_one_more_strongly(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let mut code = code_window_with(&ide, cx, "cat cat\ndog cat");
    code.simulate_keystrokes("cmd-f");
    code.simulate_input("cat");
    code.run_until_parked();
    // three matches; the search starts at the caret, the end of the first
    // line, so the current one is the match in the second line
    ide.read_with(cx, |ide, _| {
        let bar = ide.find.as_ref().unwrap();
        assert_eq!(bar.found.len(), 3);
        assert_eq!(bar.current, Some(2), "the first match from the caret");
    });
    // the rows show the tints: the first row has two matches
    assert!(code.debug_bounds(sel("editor-row-0".into())).is_some());
    assert!(code.debug_bounds(sel("editor-row-1".into())).is_some());
}

#[gpui::test]
fn the_focused_box_shows_a_caret(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let mut code = code_window_with(&ide, cx, "abc");
    code.simulate_keystrokes("cmd-alt-f");
    code.run_until_parked();
    // the Find box has the focus when the bar opens: its caret is drawn
    assert!(
        code.debug_bounds(sel("find-caret".into())).is_some(),
        "the Find box shows a caret"
    );
    assert!(
        code.debug_bounds(sel("replace-caret".into())).is_none(),
        "the Replace box has no caret while Find has the focus"
    );
    // clicking the Replace box moves the focus and the caret; redraw first,
    // since debug_bounds keeps entries from the frame before
    let replace = code
        .debug_bounds(sel("replace-text".into()))
        .expect("the Replace box");
    code.simulate_click(replace.center(), gpui::Modifiers::none());
    code.run_until_parked();
    ide.update(&mut code, |_, cx| cx.notify());
    code.run_until_parked();
    assert!(code.debug_bounds(sel("replace-caret".into())).is_some());
    assert!(code.debug_bounds(sel("find-caret".into())).is_none());
}

#[gpui::test]
fn arrows_home_and_end_move_the_caret_in_the_find_box(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let mut code = code_window_with(&ide, cx, "abc");
    code.simulate_keystrokes("cmd-f");
    code.simulate_input("ac");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(
            ide.find.as_ref().unwrap().find_caret,
            2,
            "typing leaves the caret at the end"
        );
    });
    code.simulate_keystrokes("left");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.find.as_ref().unwrap().find_caret, 1, "← moves back");
    });
    // typing now goes in at the caret: "a" + "b" + "c" -> "abc"
    code.simulate_input("b");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        let bar = ide.find.as_ref().unwrap();
        assert_eq!(bar.find, "abc", "the text was typed at the caret");
        assert_eq!(bar.find_caret, 2);
        assert_eq!(bar.found.len(), 1, "the matches follow the edited text");
    });
    code.simulate_keystrokes("home");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.find.as_ref().unwrap().find_caret, 0, "Home")
    });
    code.simulate_keystrokes("end");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.find.as_ref().unwrap().find_caret, 3, "End")
    });
}

#[gpui::test]
fn delete_and_backspace_edit_at_the_caret(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let mut code = code_window_with(&ide, cx, "abc");
    code.simulate_keystrokes("cmd-f");
    code.simulate_input("abcd");
    code.run_until_parked();
    code.simulate_keystrokes("home right");
    code.run_until_parked();
    code.simulate_keystrokes("delete");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        let bar = ide.find.as_ref().unwrap();
        assert_eq!(
            bar.find, "acd",
            "Delete removes the character after the caret"
        );
        assert_eq!(bar.find_caret, 1);
    });
    code.simulate_keystrokes("backspace");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        let bar = ide.find.as_ref().unwrap();
        assert_eq!(bar.find, "cd", "Backspace removes the character before it");
        assert_eq!(bar.find_caret, 0);
    });
}

#[gpui::test]
fn option_arrows_move_by_word_in_the_replace_box(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let mut code = code_window_with(&ide, cx, "abc");
    code.simulate_keystrokes("cmd-alt-f");
    code.run_until_parked();
    // the Replace box: click it, type two words, then ⌥← goes back a word
    let replace = code
        .debug_bounds(sel("replace-text".into()))
        .expect("the Replace box");
    code.simulate_click(replace.center(), gpui::Modifiers::none());
    code.simulate_input("bird dog");
    code.run_until_parked();
    code.simulate_keystrokes("alt-left");
    code.run_until_parked();
    ide.read_with(cx, |ide, _| {
        let bar = ide.find.as_ref().unwrap();
        assert_eq!(bar.replace, "bird dog");
        assert_eq!(bar.replace_caret, 5, "⌥← goes back to the start of dog");
    });
}
