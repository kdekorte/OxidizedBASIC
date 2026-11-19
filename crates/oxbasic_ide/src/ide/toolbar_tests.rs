//! The main window's unified toolbar: its buttons follow the program's state,
//! the Object | Code control follows the code window, the activity box shows
//! the project, the document, unsaved work and the status, and the save
//! button saves the project.

use super::toolbox_tests::{ide_window, sel};
use super::*;

fn click(cx: &mut gpui::VisualTestContext, id: &'static str) {
    let b = cx
        .debug_bounds(sel(id.to_string()))
        .unwrap_or_else(|| panic!("no {}", id));
    cx.simulate_click(b.center(), gpui::Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn the_toolbar_has_its_parts_in_one_bar(cx: &mut gpui::TestAppContext) {
    let (_ide, cx) = ide_window(cx);
    let bar = cx.debug_bounds("toolbar").expect("the toolbar");
    assert_eq!(f32::from(bar.size.height), toolbar::TOOLBAR_H);
    // left to right: Run, Stop, Step, Object | Code, the activity box, Save
    let mut x =
        |id: &'static str| f32::from(cx.debug_bounds(sel(id.to_string())).unwrap().origin.x);
    let order = [
        "toolbar-run",
        "toolbar-stop",
        "toolbar-step",
        "toolbar-view-object",
        "toolbar-view-code",
        "toolbar-activity",
        "toolbar-save",
    ];
    for w in order.windows(2) {
        assert!(x(w[0]) < x(w[1]), "{} before {}", w[0], w[1]);
    }
    // the traffic lights have the bar's left end to themselves
    assert!(x("toolbar-run") - f32::from(bar.origin.x) >= 80.0);
    // the old text buttons are gone
    assert!(cx.debug_bounds("toolbar-save-code").is_none());
    assert!(cx.debug_bounds("toolbar-save-form").is_none());
}

#[gpui::test]
fn run_and_stop_follow_the_program(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.run_state(), toolbar::RunState::Idle)
    });
    click(cx, "toolbar-run");
    ide.read_with(cx, |ide, _| {
        assert_ne!(ide.run_state(), toolbar::RunState::Idle)
    });
    click(cx, "toolbar-stop");
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.run_state(), toolbar::RunState::Idle);
        assert_eq!(ide.status, "Stopped");
    });
    // the status shows in the activity box
    assert!(cx.debug_bounds("toolbar-status").is_some());
}

#[gpui::test]
fn the_mode_control_switches_between_the_form_and_its_code(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    ide.read_with(cx, |ide, _| assert!(ide.code_window.is_none()));
    click(cx, "toolbar-view-code");
    ide.read_with(cx, |ide, _| {
        assert!(ide.code_window.is_some(), "Code opens it")
    });
    click(cx, "toolbar-view-object");
    ide.read_with(cx, |ide, _| {
        assert!(ide.code_window.is_none(), "Object closes it")
    });
}

#[gpui::test]
fn the_activity_box_marks_unsaved_work(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, cx| {
        ide.lines.push("' a change".into());
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("toolbar-unsaved").is_some(),
        "an edit shows as Edited"
    );
}

#[gpui::test]
fn the_save_button_saves_the_project(cx: &mut gpui::TestAppContext) {
    let dir = std::env::temp_dir().join(format!("oxide-toolbar-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("Saved.oxproj");
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, _| {
        ide.save_project_to(file.clone()).unwrap();
        ide.lines.push("' after the first save".into());
    });
    click(cx, "toolbar-save");
    let code = std::fs::read_to_string(dir.join("Form1.bas")).unwrap();
    assert!(code.contains("' after the first save"), "{}", code);
    ide.read_with(cx, |ide, _| {
        assert!(!ide.workspace.is_dirty(), "every document saved");
        assert!(ide.status.starts_with("Saved "), "{}", ide.status);
        // what the bar's Edited marker is drawn from (GPUI's debug_bounds
        // keeps entries from earlier frames, so it can't show the marker
        // went away)
        assert!(!ide.has_unsaved_work(), "Edited is cleared");
    });
    std::fs::remove_dir_all(&dir).ok();
}
