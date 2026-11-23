//! Running a project without the editor: the program a project file runs,
//! built from the project alone. The IDE builds the same program from its
//! working copy (`launch.rs`); a packaged or played project has no editor, so
//! this reads the project's files and builds it directly.
//!
//! [`project_program`] is what `oxbasic play` and a packaged app run.

use super::debug_session::ProgramSpec;
use super::program::{combine, Program};
use super::workspace::{DocKind, Workspace};
use std::path::Path;

/// The program a project file runs, and its forms: the same program the IDE's
/// Run starts for that project.
pub fn project_program(file: &Path) -> Result<(ProgramSpec, Program), String> {
    let workspace = Workspace::open(file)?;
    let program = combine(&workspace.docs);
    let forms: Vec<_> = workspace
        .docs
        .iter()
        .filter(|d| d.kind == DocKind::Form)
        .map(|d| d.form.clone())
        .collect();
    // the start-up form is found among the forms by name, as the IDE does
    let startup = workspace
        .docs
        .iter()
        .filter(|d| d.kind == DocKind::Form)
        .position(|d| d.name.eq_ignore_ascii_case(&workspace.startup))
        .unwrap_or(0);
    let spec = ProgramSpec {
        source: program.source.clone(),
        forms,
        startup,
        bare: false,
        base_dir: file.parent().map(|p| p.to_path_buf()),
    };
    Ok((spec, program))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn example(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name)
    }

    #[test]
    fn the_showcase_builds_a_multi_form_program_with_form1_at_start() {
        let (spec, program) = project_program(&example("showcase/showcase.oxproj")).expect("opens");
        assert_eq!(
            spec.forms.len(),
            5,
            "Form1, frmAbout, frmDrawing, frmFiles and frmTabs"
        );
        assert_eq!(spec.startup, 0, "Form1 is the start-up form");
        assert!(!spec.bare);
        assert!(program.source.contains("cmdGreet_Click") || program.source.contains("Form1__"));
        assert!(
            spec.base_dir.is_some(),
            "pictures resolve from the project's folder"
        );
    }

    #[test]
    fn the_fractal_builds_its_one_form() {
        let (spec, _) = project_program(&example("fractal/fractal.oxproj")).expect("opens");
        assert_eq!(spec.forms.len(), 1);
        assert_eq!(spec.forms[0].name, "Form1");
        assert!(
            spec.source.contains("DrawFractal"),
            "its module code is in the program"
        );
    }

    #[test]
    fn a_missing_project_is_an_error() {
        assert!(project_program(&example("nope.oxproj")).is_err());
    }

    #[test]
    fn the_datagrid_example_builds_its_form_with_the_grid_handlers() {
        let (spec, _) = project_program(&example("datagrid/datagrid.oxproj")).expect("it builds");
        assert_eq!(spec.forms[spec.startup].name, "frmGrid", "frmGrid starts");
        for handler in ["Grid1_RowColChange", "Grid1_ValidateEdit", "Grid1_AfterEdit", "Grid1_Scroll"] {
            assert!(spec.source.contains(handler), "{} is in the program", handler);
        }
    }
}
