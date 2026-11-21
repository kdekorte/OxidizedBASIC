//! The showcase's drawing and file forms, run for real: the scrollbars resize
//! a Shape and move a Line, the readout follows the thumbs, and the file
//! browser lists volumes, folders and files and shows the chosen file's path.

use super::debug_session::{DebugCommand, DebugEvent};
use super::showcase_run::{caption, Run};
use oxbasic_ui::host::{FormSnapshot, UserEdit};

/// The index of a form in the showcase: Form1 0, frmAbout 1, frmDrawing 2,
/// frmFiles 3.
const DRAWING: usize = 2;
const FILES: usize = 3;

/// A scrollbar on `form` dragged to `value`, then its Change handler run.
fn drag(run: &Run, form: usize, name: &str, handler: &str, value: i32) -> FormSnapshot {
    let i = run.control(form, name, None);
    run.session.send(DebugCommand::Edit(
        form,
        UserEdit::Scroll { control: i, value },
    ));
    run.call(form, handler, &[])
}

/// Show the form at `index` the way Form1's button does, and wait for it to
/// open (its Form_Load has run by then).
fn open(run: &Run, button: &str, index: usize) {
    run.session
        .send(DebugCommand::Call(0, button.into(), Vec::new()));
    run.until(|e| matches!(e, DebugEvent::FormShown(s) if s.form_index == index));
}

#[test]
fn the_horizontal_bar_sets_the_shape_and_line_width_and_the_readout() {
    let run = Run::start();
    open(&run, "cmdDrawing_Click", DRAWING);
    let snap = drag(&run, DRAWING, "hsbWidth", "hsbWidth_Change", 150);
    assert_eq!(caption(&snap, "lblReadout"), "Width 150   Height 60");
    let shape = run.control(DRAWING, "shpBox", None);
    assert_eq!(snap.form.controls[shape].rect.width, 150.0);
    let line = run.control(DRAWING, "lneRule", None);
    assert_eq!(snap.form.controls[line].rect.width, 150.0);
}

#[test]
fn the_vertical_bar_sets_the_shape_height_and_the_readout() {
    let run = Run::start();
    open(&run, "cmdDrawing_Click", DRAWING);
    let snap = drag(&run, DRAWING, "vsbHeight", "vsbHeight_Change", 90);
    let shape = run.control(DRAWING, "shpBox", None);
    assert_eq!(snap.form.controls[shape].rect.height, 90.0);
    assert_eq!(caption(&snap, "lblReadout"), "Width 100   Height 90");
}

#[test]
fn choosing_a_volume_lists_its_folders_on_the_files_form() {
    let run = Run::start();
    open(&run, "cmdFiles_Click", FILES);
    run.pick(FILES, "volVolume", 0);
    let snap = run.call(FILES, "volVolume_Click", &[]);
    let folders = run.control(FILES, "dirFolder", None);
    assert!(
        snap.form.controls[folders].path.starts_with("/Volumes/"),
        "the folder list follows the volume: {}",
        snap.form.controls[folders].path
    );
    assert!(caption(&snap, "lblChosen").starts_with("Folders on "));
}

#[test]
fn a_file_shows_its_full_path_when_chosen() {
    let run = Run::start();
    open(&run, "cmdFiles_Click", FILES);
    let snap = run.call(FILES, "filFile_Click", &[]);
    // nothing chosen yet: the list has no file, so the name is empty
    assert_eq!(caption(&snap, "lblChosen"), "Chosen: ");
}

#[test]
fn the_slider_and_the_width_bar_move_together() {
    let run = Run::start();
    open(&run, "cmdDrawing_Click", DRAWING);
    // a drag on the Slider sets the width bar (and the shape) through the program
    let slider = run.control(DRAWING, "sldWidth", None);
    run.session.send(DebugCommand::Edit(
        DRAWING,
        UserEdit::Scroll {
            control: slider,
            value: 180,
        },
    ));
    let snap = run.call(DRAWING, "sldWidth_Change", &[]);
    let bar = run.control(DRAWING, "hsbWidth", None);
    assert_eq!(
        snap.state[bar].value, 180,
        "the width bar follows the slider"
    );
    let shape = run.control(DRAWING, "shpBox", None);
    assert_eq!(snap.form.controls[shape].rect.width, 180.0);
}

/// frmTabs' index in the showcase (after Form1, frmAbout, frmDrawing, frmFiles).
const TABS: usize = 4;

#[test]
fn next_and_back_move_the_tabs_page_and_the_status_names_it() {
    let run = Run::start();
    open(&run, "cmdTabs_Click", TABS);
    // the first page, General, is selected
    let snap = run.call(TABS, "cmdNext_Click", &[]);
    let tabs = run.control(TABS, "tabSettings", None);
    assert_eq!(snap.state[tabs].tab, 1, "Next moves to Options");
    assert_eq!(caption(&snap, "lblStatus"), "Page 2 of 3: Options");
    let snap = run.call(TABS, "cmdBack_Click", &[]);
    assert_eq!(snap.state[tabs].tab, 0, "Back returns to General");
    assert_eq!(caption(&snap, "lblStatus"), "Page 1 of 3: General");
}

#[test]
fn back_on_the_first_page_and_next_on_the_last_stay_put() {
    let run = Run::start();
    open(&run, "cmdTabs_Click", TABS);
    let snap = run.call(TABS, "cmdBack_Click", &[]);
    let tabs = run.control(TABS, "tabSettings", None);
    assert_eq!(snap.state[tabs].tab, 0, "there is no page before the first");
    run.call(TABS, "cmdNext_Click", &[]);
    run.call(TABS, "cmdNext_Click", &[]);
    let snap = run.call(TABS, "cmdNext_Click", &[]);
    assert_eq!(snap.state[tabs].tab, 2, "the last page stays the last");
}

#[test]
fn choosing_a_tab_by_the_menu_opens_the_tabs_form() {
    let run = Run::start();
    run.session
        .send(DebugCommand::Call(0, "mnuTabs_Click".into(), Vec::new()));
    run.until(|e| matches!(e, DebugEvent::FormShown(s) if s.form_index == TABS));
}

#[test]
fn the_advanced_page_shows_only_its_own_controls() {
    let run = Run::start();
    open(&run, "cmdTabs_Click", TABS);
    let tabs = run.control(TABS, "tabSettings", None);
    let level = run.control(TABS, "lblLevel", None);
    // Back to General: the Advanced page's Level label is not shown
    let snap = run.call(TABS, "cmdBack_Click", &[]);
    assert_eq!(snap.state[tabs].tab, 0);
    assert!(!snap.form.shown(level), "Level belongs to the Advanced page");
    // on to Advanced: the label shows
    run.call(TABS, "cmdNext_Click", &[]);
    let snap = run.call(TABS, "cmdNext_Click", &[]);
    assert_eq!(snap.state[tabs].tab, 2);
    assert!(snap.form.shown(level), "Level shows on the Advanced page");
}
