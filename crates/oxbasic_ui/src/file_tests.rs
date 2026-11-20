//! FileListBox: it lists the files in its Path that match its Pattern; setting
//! Path or Pattern reloads it, a Path that isn't a folder is error 76, and
//! FileName is the selected file's full path.

use crate::form::file_names;
use crate::host::FormHost;
use crate::{ControlKind, ControlSpec, FormSpec, Rect};
use oxbasic_runtime::pattern_matches;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// A scratch folder: two programs, a project, a note, a hidden file and a
/// sub-folder (which is not a file).
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oxbasic-file-{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Sub")).unwrap();
    for f in [
        "game.bas",
        "Alpha.bas",
        "proj.oxproj",
        "notes.txt",
        ".secret.bas",
    ] {
        std::fs::write(dir.join(f), "x").unwrap();
    }
    dir
}

fn host(path: &str, pattern: &str) -> Rc<RefCell<FormHost>> {
    let mut f = FormSpec::new("Form1", "Files", 400.0, 300.0);
    let mut l = ControlSpec::new(
        "File1",
        ControlKind::FileListBox,
        Rect::new(0.0, 0.0, 120.0, 84.0),
    );
    l.path = path.to_string();
    l.pattern = pattern.to_string();
    f.control(l);
    Rc::new(RefCell::new(FormHost::new(f)))
}

fn run(src: &str, h: &Rc<RefCell<FormHost>>) -> Result<String, String> {
    crate::host::tests::run(src, h)
}

#[test]
fn a_pattern_matches_listed_extensions_and_exact_names_ignoring_case() {
    assert!(pattern_matches("Game.BAS", "*.bas;*.oxproj"));
    assert!(pattern_matches("proj.oxproj", "*.bas;*.oxproj"));
    assert!(!pattern_matches("notes.txt", "*.bas;*.oxproj"));
    assert!(pattern_matches("readme", "readme"));
    assert!(
        pattern_matches("anything.zip", ""),
        "an empty pattern lists every file"
    );
}

#[test]
fn a_file_list_holds_only_matching_files_sorted_without_folders_or_hidden_ones() {
    let dir = scratch("match");
    assert_eq!(
        file_names(dir.to_str().unwrap(), "*.bas"),
        vec!["Alpha.bas".to_string(), "game.bas".to_string()]
    );
    // every file, but still no folder and no hidden file
    let all = file_names(dir.to_str().unwrap(), "");
    assert!(
        all.iter().all(|n| n != "Sub" && !n.starts_with('.')),
        "{:?}",
        all
    );
    assert_eq!(all.len(), 4, "{:?}", all);
}

#[test]
fn the_file_list_loads_with_its_path_and_pattern() {
    let dir = scratch("load");
    let h = host(dir.to_str().unwrap(), "*.bas");
    assert_eq!(
        h.borrow().form.controls[0].items,
        vec!["Alpha.bas", "game.bas"]
    );
    let out = run("Print File1.ListCount\n", &h).unwrap();
    assert_eq!(out, " 2 \n");
}

#[test]
fn setting_pattern_reloads_the_list() {
    let dir = scratch("pattern");
    let h = host(dir.to_str().unwrap(), "*.bas");
    let out = run(
        "File1.Pattern = \"*.txt\"\nPrint File1.ListCount; File1.Pattern\n",
        &h,
    )
    .unwrap();
    assert_eq!(out, " 1 *.txt\n");
    assert_eq!(h.borrow().form.controls[0].items, vec!["notes.txt"]);
}

#[test]
fn setting_path_reloads_the_files_in_it() {
    let dir = scratch("path");
    let h = host("", "");
    let out = run(
        &format!(
            "File1.Path = \"{}\"\nPrint File1.ListCount\n",
            dir.display()
        ),
        &h,
    )
    .unwrap();
    assert_eq!(out, " 4 \n");
}

#[test]
fn a_file_list_path_that_is_not_a_folder_is_error_76() {
    let h = host("", "");
    let e = run("File1.Path = \"/no/such/folder/anywhere\"\n", &h).unwrap_err();
    assert!(e.contains("76"), "{}", e);
}

#[test]
fn the_file_name_is_the_selected_files_full_path() {
    let dir = scratch("filename");
    let h = host(dir.to_str().unwrap(), "*.bas");
    let out = run("File1.ListIndex = 1\nPrint File1.FileName\n", &h).unwrap();
    assert_eq!(out, format!("{}\n", dir.join("game.bas").display()));
    let none = run(
        "File1.ListIndex = -1\nPrint \"[\" & File1.FileName & \"]\"\n",
        &h,
    );
    assert_eq!(none.unwrap(), "[]\n");
}
