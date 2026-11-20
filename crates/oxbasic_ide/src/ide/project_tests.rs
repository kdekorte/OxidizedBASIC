//! File-menu and Project-panel tests: switching between a project's forms
//! and modules, the unsaved-changes prompt, and saving through the native
//! dialog.

use super::toolbox_tests::{ide_window, sel};
use super::*;

/// A folder for one test, removed afterwards.
pub(super) struct Dir(pub(super) std::path::PathBuf);

impl Dir {
    pub(super) fn new(tag: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("oxide-{}-{}", tag, std::process::id()));
        std::fs::remove_dir_all(&p).ok();
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

#[gpui::test]
fn the_project_panel_lists_documents_and_switches_between_them(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, cx| {
        ide.add_module(cx);
        ide.add_form(cx);
    });
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        let names: Vec<_> = ide.workspace.docs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Form1", "Module1", "Form2"]);
        assert_eq!(ide.workspace.active, 2, "a new document is shown");
    });

    // edits in one document are kept when another is shown
    ide.update(cx, |ide, cx| {
        ide.lines = vec!["' form two".into()];
        ide.form.caption = "Second".into();
        cx.notify();
    });
    let first = cx
        .debug_bounds(sel("project-doc-0".into()))
        .expect("Form1 row");
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.workspace.active, 0);
        assert_ne!(ide.form.caption, "Second", "Form1's own layout is shown");
    });
    let third = cx
        .debug_bounds(sel("project-doc-2".into()))
        .expect("Form2 row");
    cx.simulate_click(third.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(
            ide.form.caption, "Second",
            "Form2's edits survived the switch"
        );
        assert_eq!(ide.lines, ["' form two"]);
    });
}

#[gpui::test]
fn a_module_keeps_its_code_and_the_forms_keep_their_layout(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, cx| {
        ide.form.caption = "Main".into();
        ide.add_module(cx);
        ide.lines = vec!["Sub Helper()".into(), "End Sub".into()];
        ide.add_form(cx);
        ide.switch_to(0);
    });
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.form.caption, "Main");
        assert_eq!(ide.workspace.docs[1].lines[0], "Sub Helper()");
    });
}

#[gpui::test]
fn unsaved_work_is_flagged_until_the_project_is_saved(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("flag");
    let file = dir.0.join("app.oxproj");
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, _| {
        assert!(ide.has_unsaved(), "a new project is unsaved");
        ide.save_project_to(file.clone()).unwrap();
        assert!(!ide.has_unsaved());
        assert_eq!(ide.workspace.name, "app", "named after its file");
        // an edit in the working copy counts before it is stored
        ide.lines = vec!["Print 1".into()];
        assert!(ide.working_copy_changed());
        assert!(ide.has_unsaved());
        ide.save_project().unwrap();
        assert!(!ide.has_unsaved());
    });
    let code = std::fs::read_to_string(dir.0.join("Form1.bas")).unwrap();
    assert_eq!(code, "Print 1\n");
}

#[gpui::test]
fn new_project_asks_before_discarding_and_obeys_the_answer(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("prompt");
    let file = dir.0.join("app.oxproj");
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, cx| {
        ide.save_project_to(file.clone()).unwrap();
        ide.form.caption = "Edited".into();
        cx.notify();
    });
    let new_project = |ide: &Entity<Ide>, cx: &mut gpui::VisualTestContext| {
        ide.update_in(cx, |ide, win, cx| ide.new_project(win, cx));
        cx.run_until_parked();
    };

    // Cancel: nothing changes
    new_project(&ide, cx);
    assert!(cx.has_pending_prompt(), "the user is asked");
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.form.caption, "Edited");
        assert!(ide.workspace.file.is_some());
    });

    // Don't Save: a fresh project replaces it, and the file is untouched
    new_project(&ide, cx);
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert!(ide.workspace.file.is_none(), "a new, unsaved project");
        assert_eq!(ide.workspace.name, "Project1");
    });
    let saved = std::fs::read_to_string(dir.0.join("Form1.oxform")).unwrap();
    assert!(
        !saved.contains("Edited"),
        "the edit was discarded, not saved"
    );
}

#[gpui::test]
fn saving_at_the_prompt_writes_the_project_first(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("save-first");
    let file = dir.0.join("app.oxproj");
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, _| {
        ide.save_project_to(file.clone()).unwrap();
        ide.form.caption = "Keep me".into();
    });
    ide.update_in(cx, |ide, win, cx| ide.new_project(win, cx));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    let saved = std::fs::read_to_string(dir.0.join("Form1.oxform")).unwrap();
    assert!(
        saved.contains("Keep me"),
        "Save wrote the edit before replacing"
    );
    ide.read_with(cx, |ide, _| assert!(ide.workspace.file.is_none()));
}

#[gpui::test]
fn save_project_as_uses_the_native_save_dialog(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("saveas");
    let target = dir.0.join("chosen.oxproj");
    let (ide, cx) = ide_window(cx);
    ide.update_in(cx, |ide, win, cx| ide.save_project_as(win, cx));
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path(), "the dialog was shown");
    let t = target.clone();
    cx.simulate_new_path_selection(move |_| Some(t.clone()));
    cx.run_until_parked();
    assert!(target.is_file(), "the project file was written");
    assert!(dir.0.join("Form1.oxform").is_file());
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.workspace.file.as_deref(), Some(target.as_path()));
        assert_eq!(ide.workspace.name, "chosen");
    });
}

#[gpui::test]
fn a_saved_project_reopens_with_all_its_forms_and_modules(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("open");
    let file = dir.0.join("app.oxproj");
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, cx| {
        ide.add_module(cx);
        ide.lines = vec!["Sub Helper()".into(), "End Sub".into()];
        ide.add_form(cx);
        ide.save_project_to(file.clone()).unwrap();
        // reopening replaces what is shown with what the file holds
        ide.form.caption = "Unsaved edit".into();
        ide.open_project_file(&file, cx);
    });
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.workspace.docs.len(), 3);
        assert_eq!(ide.workspace.name, "app");
        assert!(!ide.workspace.is_dirty());
        assert_ne!(ide.form.caption, "Unsaved edit");
    });
}

#[gpui::test]
fn the_ide_window_may_close_when_saved_and_asks_when_not(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("close");
    let file = dir.0.join("app.oxproj");
    let (ide, cx) = ide_window(cx);

    // a saved project closes without a question
    ide.update(cx, |ide, _| ide.save_project_to(file.clone()).unwrap());
    let closes = ide.update_in(cx, |ide, win, cx| ide.may_close(win, cx));
    assert!(closes, "nothing to lose");
    assert!(!cx.has_pending_prompt());

    // an edited one is refused, and the user is asked
    ide.update(cx, |ide, _| ide.form.caption = "Edited".into());
    let closes = ide.update_in(cx, |ide, win, cx| ide.may_close(win, cx));
    cx.run_until_parked();
    assert!(!closes, "the close waits for the answer");
    assert!(cx.has_pending_prompt());
    // Cancel keeps the window and the edit
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.form.caption, "Edited");
        assert!(!ide.allow_close);
    });
}

#[gpui::test]
fn quit_asks_first_when_there_is_unsaved_work(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    // a new project is unsaved
    ide.update_in(cx, |ide, win, cx| ide.quit(win, cx));
    cx.run_until_parked();
    assert!(
        cx.has_pending_prompt(),
        "Quit asked about the unsaved project"
    );
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert!(ide.working_copy_changed() || ide.workspace.is_dirty())
    });
}

#[gpui::test]
fn opening_a_broken_project_reports_it_and_keeps_the_current_one(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("broken");
    let file = dir.0.join("bad.oxproj");
    std::fs::write(&file, "{ not json").unwrap();
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, cx| {
        ide.form.caption = "Mine".into();
        ide.open_project_file(&file, cx);
    });
    ide.read_with(cx, |ide, _| {
        assert!(ide.status.starts_with("open failed"), "{}", ide.status);
        assert_eq!(ide.form.caption, "Mine", "the open project is untouched");
    });
}

#[gpui::test]
fn renaming_a_form_updates_the_project_and_other_forms_code(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, cx| {
        // Form2's code opens Form1 by name
        ide.add_form(cx);
        ide.lines = vec![
            "Sub Command1_Click()".into(),
            "  Form1.Show".into(),
            "End Sub".into(),
        ];
        ide.switch_to(0);
        assert_eq!(ide.workspace.startup, "Form1");

        // rename Form1 (the form itself is selected in the designer)
        ide.prop_bufs.insert((FORM_IDX, "name"), "Main".into());
        assert!(ide.commit_rename(FORM_IDX));
    });
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.workspace.docs[0].name, "Main");
        assert_eq!(ide.workspace.startup, "Main", "the startup object follows");
        assert_eq!(ide.form.name, "Main");
        assert_eq!(ide.workspace.docs[1].lines[1], "  Main.Show");
    });
}

#[gpui::test]
fn a_form_cannot_take_another_documents_name(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, cx| {
        ide.add_module(cx);
        ide.switch_to(0);
        ide.prop_bufs.insert((FORM_IDX, "name"), "Module1".into());
        assert!(
            !ide.commit_rename(FORM_IDX),
            "the name is taken by a module"
        );
        assert_eq!(ide.workspace.docs[0].name, "Form1");
        assert!(ide.rename_error.is_some());
    });
}
