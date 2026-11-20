//! File > Package Project…: turn the open project into a macOS app that runs
//! it. The project is saved first, so the package carries what the user sees;
//! the app is written where the user chooses. The bundle itself is written by
//! `package.rs`.

use super::package::{write_package, PackageSpec};
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
        let spec = PackageSpec {
            project,
            bundle: bundle.clone(),
            name: app_name(&bundle),
            icon: app_icon(),
            binary: std::env::current_exe().unwrap_or_else(|_| "oxbasic".into()),
        };
        match write_package(&spec) {
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

/// The app's name: the bundle's file name without `.app`, so the menu bar and
/// Finder show what the user named it.
fn app_name(bundle: &std::path::Path) -> String {
    bundle
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "OxidizedBASIC".into())
}

/// The OxidizedBASIC icon, if the packaging folder is found next to the IDE's
/// source (a development build). A packaged app then carries the icon; when
/// the icon isn't found the app uses the default one.
fn app_icon() -> Option<std::path::PathBuf> {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging/icons/OxidizedBASIC.icns");
    here.exists().then_some(here)
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
