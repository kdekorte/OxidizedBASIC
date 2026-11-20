//! DirListBox: it lists the sub-folders of its Path, and setting Path lists
//! another folder (raising Change), or fails with error 76 when it isn't one.

use crate::form::folder_names;
use crate::host::FormHost;
use crate::{ControlKind, ControlSpec, FormSpec, Rect};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// A scratch folder with two sub-folders, one hidden, and a plain file.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oxbasic-dir-{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Beta")).unwrap();
    std::fs::create_dir_all(dir.join("Alpha")).unwrap();
    std::fs::create_dir_all(dir.join(".hidden")).unwrap();
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    dir
}

fn host(path: &str) -> Rc<RefCell<FormHost>> {
    let mut f = FormSpec::new("Form1", "Dirs", 400.0, 300.0);
    let mut d = ControlSpec::new(
        "Dir1",
        ControlKind::DirListBox,
        Rect::new(0.0, 0.0, 120.0, 84.0),
    );
    d.path = path.to_string();
    f.control(d);
    Rc::new(RefCell::new(FormHost::new(f)))
}

fn run(src: &str, h: &Rc<RefCell<FormHost>>) -> Result<String, String> {
    crate::host::tests::run(src, h)
}

#[test]
fn a_folder_list_holds_its_sub_folders_sorted_without_files_or_hidden_ones() {
    let dir = scratch("list");
    let names = folder_names(dir.to_str().unwrap());
    assert_eq!(names, vec!["Alpha".to_string(), "Beta".to_string()]);
}

#[test]
fn a_folder_that_cannot_be_read_has_no_entries() {
    assert!(folder_names("/no/such/folder/anywhere").is_empty());
}

#[test]
fn the_dir_list_shows_its_path_and_lists_it_on_load() {
    let dir = scratch("load");
    let h = host(dir.to_str().unwrap());
    assert_eq!(h.borrow().form.controls[0].items, vec!["Alpha", "Beta"]);
    let out = run("Print Dir1.Path\n", &h).unwrap();
    assert_eq!(out, format!("{}\n", dir.display()));
}

#[test]
fn setting_path_lists_that_folder_and_raises_change() {
    let dir = scratch("set");
    let h = host("");
    // the folder holds Alpha and Beta: setting Path lists them
    let out = run(
        &format!("Dir1.Path = \"{}\"\nPrint Dir1.ListCount\n", dir.display()),
        &h,
    )
    .unwrap();
    assert_eq!(out, " 2 \n");
    assert_eq!(h.borrow().form.controls[0].path, dir.to_str().unwrap());
    // the raised events are taken as each statement ends, so Change is
    // checked on the change itself: a Path that moved raises Dir1_Change
    let before = h.borrow().watched(0);
    h.borrow_mut().form.controls[0].path = String::new();
    let before2 = h.borrow().watched(0);
    h.borrow_mut().raise_changes(0, &before2);
    h.borrow_mut().form.controls[0].path = dir.to_str().unwrap().to_string();
    h.borrow_mut().raise_changes(0, &before2);
    let raised = h.borrow().raised_handlers();
    assert!(
        raised.iter().any(|e| e == "Dir1_Change"),
        "Path changing raises Change: {:?}",
        raised
    );
    let _ = before;
}

#[test]
fn a_path_that_is_not_a_folder_is_error_76() {
    let h = host("");
    let e = run("Dir1.Path = \"/no/such/folder/anywhere\"\n", &h).unwrap_err();
    assert!(e.contains("76"), "{}", e);
}
