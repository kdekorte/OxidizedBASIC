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
