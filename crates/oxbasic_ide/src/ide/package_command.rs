//! File > Package Project…: turn the open project into a macOS app that runs
//! it. The project is saved first, so the package carries what the user sees;
//! the app is written where the user chooses. The bundle itself is written by
//! `package.rs`.

use super::package::{package_settings, write_package, PackageSpec};
use super::*;

impl Ide {
    /// File > Package Project…: save the project, then ask where the app goes
    /// and write it. The result is reported in the status line.
    pub(super) fn package_project(&mut self, win: &mut Window, cx: &mut Context<Self>) {
        // the package copies files from disk: save the project to its file
        // first, asking for one if the project has none yet
        if self.workspace.file.is_none() {
            self.save_project_as_then(win, cx, |ide, win, cx| {
                ide.package_project(win, cx);
            });
            return;
        }
        if let Err(e) = self.save_project() {
            self.status = format!("cannot package: {}", e);
            cx.notify();
            return;
        }
        let Some(project) = self.workspace.file.clone() else {
            return;
        };
        let name = self.workspace.name.clone();
        let dir = project
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{}.app", name)));
        cx.spawn_in(win, async move |this, cx| {
            let Ok(Ok(Some(bundle))) = rx.await else {
                return;
            };
            let _ = this.update_in(cx, move |ide, _win, cx| {
                ide.write_app(project, bundle, cx);
            });
        })
        .detach();
    }

    /// Write the app for `project` to `bundle`, reporting what happened.
    pub(super) fn write_app(
        &mut self,
        project: std::path::PathBuf,
        bundle: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        match package_from_here(&project, &bundle) {
            Ok(()) => {
                self.status = format!("packaged {}", bundle.display());
                self.output
                    .push_str(&format!("[packaged {}]\n", bundle.display()));
            }
            Err(e) => {
                self.status = format!("package failed: {}", e);
                self.output.push_str(&format!("[package failed: {}]\n", e));
            }
        }
        cx.notify();
    }
}

/// Package the project at `project` as the app at `bundle`, from this running
/// binary: the same program, icon and name rules the menu command uses. The
/// menu and the `package` command line both go through here.
pub fn package_from_here(
    project: &std::path::Path,
    bundle: &std::path::Path,
) -> Result<(), String> {
    // the version, identifier and icon come from the project file; the IDE's
    // own icon is used only when the project names none
    let settings = package_settings(project)?;
    write_package(&PackageSpec {
        project: project.to_path_buf(),
        bundle: bundle.to_path_buf(),
        name: app_name(bundle),
        icon: settings.icon.or_else(app_icon),
        binary: std::env::current_exe().map_err(|e| format!("cannot find this program: {}", e))?,
        version: settings.version,
        bundle_id: settings.bundle_id,
    })
}

/// The app's name: the bundle's file name without `.app`, so the menu bar and
/// Finder show what the user named it.
fn app_name(bundle: &std::path::Path) -> String {
    bundle
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "OxidizedBASIC".into())
}

/// The OxidizedBASIC icon for a new package. A running IDE app carries it in its
/// own bundle (`Contents/Resources`), so that is looked at first; a development
/// build (`cargo run`) finds it in the repo's packaging folder instead. Returns
/// `None` when neither is there, and the package then has the default icon.
fn app_icon() -> Option<std::path::PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        // Contents/MacOS/<exe> -> Contents/Resources/OxidizedBASIC.icns
        if let Some(contents) = exe.parent().and_then(|m| m.parent()) {
            let in_app = contents.join("Resources").join("OxidizedBASIC.icns");
            if in_app.exists() {
                return Some(in_app);
            }
        }
    }
    let in_repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging/icons/OxidizedBASIC.icns");
    in_repo.exists().then_some(in_repo)
}

#[cfg(test)]
mod tests {
    use super::super::project_tests::Dir;
    use super::super::toolbox_tests::ide_window;
    use super::*;

    #[gpui::test]
    fn packaging_the_open_project_writes_an_app_that_carries_it(cx: &mut gpui::TestAppContext) {
        let dir = Dir::new("package-ide");
        let (ide, cx) = ide_window(cx);
        ide.update(cx, |ide, _| {
            ide.save_project_to(dir.0.join("shown.oxproj")).unwrap();
        });
        let bundle = dir.0.join("Shown.app");
        ide.update(cx, |ide, cx| {
            let project = ide.workspace.file.clone().unwrap();
            ide.write_app(project, bundle.clone(), cx);
        });
        ide.read_with(cx, |ide, _| {
            assert!(ide.status.starts_with("packaged "), "{}", ide.status);
        });
        assert!(
            bundle.join("Contents/MacOS/Shown").exists(),
            "the program is in the app"
        );
        assert!(bundle.join("Contents/MacOS/Shown-launch").exists());
        let inside = bundle.join("Contents/Resources/project");
        assert!(
            inside.join("shown.oxproj").exists(),
            "the project is carried"
        );
        assert!(inside.join("Form1.oxform").exists());
        assert!(inside.join("Form1.bas").exists());
    }

    #[gpui::test]
    fn a_failed_package_says_why(cx: &mut gpui::TestAppContext) {
        let dir = Dir::new("package-fail");
        let (ide, cx) = ide_window(cx);
        ide.update(cx, |ide, _| {
            ide.save_project_to(dir.0.join("nope.oxproj")).unwrap();
        });
        ide.update(cx, |ide, cx| {
            let project = ide.workspace.file.clone().unwrap();
            // a project file that has gone can't be packaged
            std::fs::remove_file(&project).unwrap();
            ide.write_app(project, dir.0.join("Nope.app"), cx);
        });
        ide.read_with(cx, |ide, _| {
            assert!(ide.status.starts_with("package failed"), "{}", ide.status);
        });
    }

    #[test]
    fn the_app_is_named_after_the_bundle() {
        assert_eq!(app_name(std::path::Path::new("/x/My Game.app")), "My Game");
    }
}
