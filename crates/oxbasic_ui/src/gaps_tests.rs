//! The known gaps closed in the file controls and shapes: a FileListBox's
//! `Pattern` raises `Change`, its `FileName` can be set (selecting that file),
//! its events include `DblClick`, and a Shape or Line is drawn no further
//! than the form. Each runs the program's own statements, as the other
//! file-list tests do.

use crate::canvas::clamped_size;
use crate::event_params;
use crate::form::{ControlKind, ControlSpec, FormSpec};
use crate::host::FormHost;
use crate::Rect;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// A scratch folder with two files, `apple.bas` and `notes.txt`.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oxbasic-gaps-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("apple.bas"), "").unwrap();
    std::fs::write(dir.join("notes.txt"), "").unwrap();
    dir
}

fn host() -> Rc<RefCell<FormHost>> {
    let mut f = FormSpec::new("Form1", "Files", 400.0, 300.0);
    f.control(ControlSpec::new(
        "File1",
        ControlKind::FileListBox,
        Rect::new(10.0, 10.0, 200.0, 200.0),
    ));
    Rc::new(RefCell::new(FormHost::new(f)))
}

fn run(src: &str, h: &Rc<RefCell<FormHost>>) -> Result<String, String> {
    crate::host::tests::run(src, h)
}

#[test]
fn a_file_list_offers_double_click_and_change() {
    assert!(event_params("FileListBox", "DblClick").is_some());
    assert!(event_params("FileListBox", "Change").is_some());
    assert!(event_params("DirListBox", "Change").is_some());
}

#[test]
fn a_scrollbar_offers_change_and_scroll() {
    assert!(event_params("HScrollBar", "Change").is_some());
    assert!(event_params("VScrollBar", "Scroll").is_some());
}

#[test]
fn a_pattern_change_raises_change_on_the_file_list() {
    let dir = scratch("pattern");
    let h = host();
    run(&format!("File1.Path = \"{}\"\n", dir.display()), &h).unwrap();
    // the raised events are taken as each statement ends, so Change is
    // checked on the change itself, as the DirListBox test does
    let before = h.borrow().watched(0);
    run("File1.Pattern = \"*.bas\"\n", &h).unwrap();
    h.borrow_mut().raise_changes(0, &before);
    assert!(
        h.borrow().raised_handlers().iter().any(|e| e == "File1_Change"),
        "{:?}",
        h.borrow().raised_handlers()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn filename_selects_that_file_in_the_list() {
    let dir = scratch("select");
    let h = host();
    run(&format!("File1.Path = \"{}\"\n", dir.display()), &h).unwrap();
    run("File1.FileName = \"notes.txt\"\n", &h).unwrap();
    let out = run("Print File1.FileName\n", &h).unwrap();
    assert_eq!(out, format!("{}\n", dir.join("notes.txt").display()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn filename_of_a_missing_file_is_error_53() {
    let dir = scratch("missing");
    let h = host();
    run(&format!("File1.Path = \"{}\"\n", dir.display()), &h).unwrap();
    let err = run("File1.FileName = \"nothing.bas\"\n", &h).unwrap_err();
    assert!(err.contains("53"), "{}", err);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_shape_is_drawn_no_further_than_the_form() {
    // at (300, 200) in a 400x300 form, only 100 x 100 is left
    assert_eq!(clamped_size(150.0, 150.0, 300.0, 200.0, 400.0, 300.0), (100.0, 100.0));
    // a shape inside the form keeps its size
    assert_eq!(clamped_size(40.0, 30.0, 10.0, 10.0, 400.0, 300.0), (40.0, 30.0));
    // a negative size draws nothing
    assert_eq!(clamped_size(-5.0, 20.0, 0.0, 0.0, 400.0, 300.0), (0.0, 20.0));
}
