//! The project's package settings in the IDE: selecting the project in the
//! Project panel shows its version, icon and bundle id in Properties; editing
//! them changes the open project, which is saved with it; choosing a control or
//! the form brings back its own properties.

use super::toolbox_tests::{ide_window, sel};
use super::*;

/// The Project panel marks unsaved items with ●, and the close prompt
/// follows the same rule: nothing unsaved, no question.
#[gpui::test]
fn the_project_panel_marks_unsaved_work_and_close_asks_only_then(cx: &mut gpui::TestAppContext) {
    let dir = super::project_tests::Dir::new("dirtymark");
    let file = dir.0.join("app.oxproj");
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, _| ide.save_project_to(file.clone()).unwrap());
    cx.run_until_parked();

    // saved: nothing unsaved, so closing does not ask
    ide.update(cx, |ide, _| {
        assert!(!ide.working_copy_changed() && !ide.has_unsaved())
    });
    let closes = ide.update_in(cx, |ide, win, cx| ide.may_close(win, cx));
    assert!(closes, "nothing unsaved: close without asking");
    assert!(!cx.has_pending_prompt());

    // an edit to the active form, still in the working copy, counts
    ide.update(cx, |ide, _| ide.form.caption = "Edited".into());
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| assert!(ide.working_copy_changed()));
    let closes = ide.update_in(cx, |ide, win, cx| ide.may_close(win, cx));
    cx.run_until_parked();
    assert!(!closes, "unsaved edits: the close waits for an answer");
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
}

#[gpui::test]
fn selecting_the_project_shows_its_package_settings(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    assert!(cx.debug_bounds(sel("project-row".into())).is_some());
    ide.read_with(cx, |ide, _| assert!(!ide.project_selected));
    let row = cx
        .debug_bounds(sel("project-row".into()))
        .expect("the project row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert!(ide.project_selected, "the project is selected")
    });
    // the project's panel is drawn, with its three package fields
    assert!(cx.debug_bounds(sel("project-settings".into())).is_some());
    assert!(cx.debug_bounds(sel("prop-version".into())).is_some());
    assert!(cx.debug_bounds(sel("prop-icon".into())).is_some());
    assert!(cx.debug_bounds(sel("prop-bundleid".into())).is_some());
}

#[gpui::test]
fn typing_in_a_package_field_changes_the_project(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let row = cx
        .debug_bounds(sel("project-row".into()))
        .expect("the project row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    // the version field starts at the default; replace it
    let field = cx
        .debug_bounds(sel("prop-version".into()))
        .expect("the Version field");
    cx.simulate_click(field.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    cx.simulate_keystrokes("backspace backspace backspace backspace backspace");
    cx.simulate_input("2.5.0");
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(
            ide.workspace.version, "2.5.0",
            "the typed version is the project's"
        );
    });
}

#[gpui::test]
fn choosing_a_control_brings_back_its_properties(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let row = cx
        .debug_bounds(sel("project-row".into()))
        .expect("the project row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    ide.update(cx, |ide, cx| {
        ide.add_control(ControlKind::CommandButton);
        cx.notify();
    });
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert!(!ide.project_selected, "a control replaces the project")
    });
    assert!(
        cx.debug_bounds(sel("prop-caption".into())).is_some(),
        "the control's Caption is shown"
    );
    assert!(cx.debug_bounds(sel("prop-version".into())).is_none());
}

/// The Icon field edits like a text box: paste a path, move the caret back
/// with the arrow key, type in the middle, and the project's icon follows.
#[gpui::test]
fn the_icon_field_takes_paste_and_caret_editing(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let row = cx
        .debug_bounds(sel("project-row".into()))
        .expect("the project row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let field = cx
        .debug_bounds(sel("prop-icon".into()))
        .expect("the Icon field");
    cx.simulate_click(field.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    // clear the field, paste a path, then step the caret back and type
    cx.simulate_keystrokes("cmd-a backspace");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("icons/app.png".into()));
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    cx.simulate_keystrokes("left left left left");
    cx.simulate_input("X");
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.workspace.icon, "icons/appX.png", "pasted, then typed mid-path");
    });
}
