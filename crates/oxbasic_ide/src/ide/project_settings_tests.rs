//! The project's package settings in the IDE: selecting the project in the
//! Project panel shows its version, icon and bundle id in Properties; editing
//! them changes the open project, which is saved with it; choosing a control or
//! the form brings back its own properties.

use super::toolbox_tests::{ide_window, sel};
use super::*;

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
