//! The merged file I/O example (`examples/fileio`) opens as a project, and its
//! form and module are both there, with their code.

use super::workspace::{DocKind, Workspace};
use std::path::PathBuf;

fn fileio_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/fileio")
}

#[test]
fn the_fileio_project_opens_with_its_form_and_module() {
    let file = fileio_dir().join("fileio.oxproj");
    let ws = Workspace::open(&file).expect("examples/fileio/fileio.oxproj opens");
    let form = ws
        .docs
        .iter()
        .find(|d| d.name == "frmRecords")
        .expect("the form frmRecords is in the project");
    assert!(matches!(form.kind, DocKind::Form), "frmRecords is a form");
    assert!(!form.lines.is_empty(), "the form has its code");
    let module = ws
        .docs
        .iter()
        .find(|d| d.name == "Module1")
        .expect("the module Module1 is in the project");
    assert!(matches!(module.kind, DocKind::Module), "Module1 is a module");
    assert!(
        module.lines.iter().any(|l| l.contains("Function SequentialDemo")),
        "the module has its demos"
    );
}

/// The DataGrid example opens, and its form is read by the real loader.
#[test]
fn the_datagrid_example_opens_and_its_grid_loads() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/datagrid/datagrid.oxproj");
    let ws = Workspace::open(&file).expect("examples/datagrid/datagrid.oxproj opens");
    let form = ws
        .docs
        .iter()
        .find(|d| d.name == "frmGrid")
        .expect("the form frmGrid is in the project");
    assert!(matches!(form.kind, DocKind::Form), "frmGrid is a form");
    assert!(form.lines.iter().any(|l| l.contains("Grid1_ValidateEdit")), "the form has its ValidateEdit");
    let grid = form
        .form
        .controls
        .iter()
        .find(|c| c.name == "Grid1")
        .expect("the form has Grid1")
        .grid
        .clone();
    assert_eq!((grid.rows, grid.cols), (9, 4), "the grid is 9 rows by 4 columns");
    assert!(grid.editable, "the grid accepts typing");
    assert_eq!(grid.cell(0, 1), Some("Item"), "the header row loads");
    assert_eq!(grid.cell(1, 1), Some("Apples"), "the body loads");
    assert_eq!(grid.col_width(3), Some(220.0), "the column widths load");
}
