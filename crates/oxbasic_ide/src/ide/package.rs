//! Packaging a project as a macOS app: the files a packaged project needs,
//! and the bundle that carries them. The app runs `oxbasic play` on a copy of
//! the project, so the user's own files are never touched and editing the
//! project later doesn't change an app already built (package it again).
//!
//! What goes in: the `.oxproj`, every form's `.oxform` and `.bas`, every
//! module's `.bas`, and every picture a form names (`Picture`, `MouseIcon`).
//! A picture the program loads by a name it builds while it runs can't be
//! found from the project, so it isn't copied.

use oxbasic_ui::project::Project;
use std::path::{Path, PathBuf};

/// The files of the project at `file`, as paths relative to its folder, in a
/// stable order, each once. Errors if the project can't be read.
pub fn project_files(file: &Path) -> Result<Vec<PathBuf>, String> {
    let project = Project::load(file)?;
    let dir = file.parent().unwrap_or(Path::new("."));
    let mut names: Vec<String> = Vec::new();
    names.push(
        file.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| "the project file has no name".to_string())?,
    );
    for f in &project.forms {
        names.push(f.layout.clone());
        names.push(f.code.clone());
    }
    for m in &project.modules {
        names.push(m.code.clone());
    }
    // the pictures a form names, read from its layout
    for f in &project.forms {
        let layout = dir.join(&f.layout);
        let text = std::fs::read_to_string(&layout)
            .map_err(|e| format!("cannot read {}: {}", layout.display(), e))?;
        let spec: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("{} is not a form layout: {}", layout.display(), e))?;
        for c in spec["controls"].as_array().into_iter().flatten() {
            for key in ["picture", "mouseIcon"] {
                if let Some(p) = c.get(key).and_then(|v| v.as_str()) {
                    if !p.is_empty() {
                        names.push(p.to_string());
                    }
                }
            }
        }
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for n in names {
        let p = PathBuf::from(n);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    Ok(out)
}

/// What a project's package is made of: the settings the project file gives,
/// with the defaults for any it leaves out.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageSettings {
    pub version: String,
    /// The bundle identifier: the project's, or one derived from its name.
    pub bundle_id: String,
    /// The icon file, resolved against the project's folder, if the project
    /// names one.
    pub icon: Option<PathBuf>,
}

/// The packaging settings of the project at `project`. An icon named in the
/// project is resolved against the project's folder; a missing icon is an
/// error, as any missing project file is.
pub fn package_settings(project: &Path) -> Result<PackageSettings, String> {
    let p = oxbasic_ui::project::Project::load(project)?;
    let dir = project.parent().unwrap_or(Path::new("."));
    let icon = if p.icon.is_empty() {
        None
    } else {
        let path = dir.join(&p.icon);
        if !path.is_file() {
            return Err(format!("icon not found: {}", path.display()));
        }
        Some(path)
    };
    let bundle_id = if p.bundle_id.trim().is_empty() {
        bundle_identifier(&p.name)
    } else {
        p.bundle_id.trim().to_string()
    };
    let version = if p.version.trim().is_empty() {
        oxbasic_ui::project::default_version()
    } else {
        p.version.trim().to_string()
    };
    Ok(PackageSettings {
        version,
        bundle_id,
        icon,
    })
}

/// What a package needs to be written: the project to carry, where the
/// bundle goes, the app's name, and the settings (icon, version, identifier)
/// the project gives.
#[derive(Debug, Clone)]
pub struct PackageSpec {
    /// The `.oxproj` to package.
    pub project: PathBuf,
    /// The `.app` folder to write (e.g. `~/Desktop/Hello.app`).
    pub bundle: PathBuf,
    /// The app's name, from the bundle's name.
    pub name: String,
    /// The app icon (`.icns`), copied into the bundle.
    pub icon: Option<PathBuf>,
    /// The `oxbasic` executable the app runs.
    pub binary: PathBuf,
    /// The app's version, and its bundle identifier.
    pub version: String,
    pub bundle_id: String,
}

/// The app's bundle identifier, built from the project's name so it is stable
/// and valid: letters and digits only.
pub fn bundle_identifier(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    format!(
        "com.oxidizedbasic.app.{}",
        if clean.is_empty() {
            "project".into()
        } else {
            clean
        }
    )
}

/// Write the app bundle described by `spec`: the executable (a launcher that
/// plays the bundled project), the project's files, the icon and an
/// `Info.plist`. A bundle already at that path is replaced.
pub fn write_package(spec: &PackageSpec) -> Result<(), String> {
    let files = project_files(&spec.project)?;
    let project_dir = spec.project.parent().unwrap_or(Path::new("."));
    let contents = spec.bundle.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    let project_in_app = resources.join("project");

    if spec.bundle.exists() {
        std::fs::remove_dir_all(&spec.bundle)
            .map_err(|e| format!("cannot replace {}: {}", spec.bundle.display(), e))?;
    }
    for dir in [&macos, &project_in_app] {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {}", dir.display(), e))?;
    }

    // the program: the same oxbasic binary, started with `play`
    let exe = macos.join(&spec.name);
    std::fs::copy(&spec.binary, &exe).map_err(|e| format!("cannot copy the program: {}", e))?;
    std::fs::write(
        macos.join(format!("{}-launch", spec.name)),
        format!(
            "#!/bin/bash\nexec \"$(dirname \"$0\")/{}\" play \"$(dirname \"$0\")/../Resources/project/{}\"\n",
            spec.name,
            spec.project.file_name().unwrap_or_default().to_string_lossy()
        ),
    )
    .map_err(|e| format!("cannot write the launcher: {}", e))?;
    set_executable(&macos.join(format!("{}-launch", spec.name)))?;

    // the project, as it is now: its files, relative to the project folder.
    // The project file itself is written below, naming the icon the bundle
    // holds (the .icns), since the original picture is not copied.
    for f in &files {
        let from = project_dir.join(f);
        let to = project_in_app.join(f);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {}", parent.display(), e))?;
        }
        if f == &files[0] {
            let mut project = oxbasic_ui::project::Project::load(&spec.project)?;
            project.icon = if spec.icon.is_some() {
                format!("{}.icns", spec.name)
            } else {
                String::new()
            };
            project.save(&to)?;
            continue;
        }
        std::fs::copy(&from, &to).map_err(|e| format!("cannot copy {}: {}", from.display(), e))?;
    }

    if let Some(icon) = &spec.icon {
        // Finder reads the app icon from Resources (CFBundleIconFile names it);
        // the bundled project finds the same file beside itself
        let app_icon = resources.join(format!("{}.icns", spec.name));
        write_icns(icon, &app_icon)?;
        std::fs::copy(&app_icon, project_in_app.join(format!("{}.icns", spec.name)))
            .map_err(|e| format!("cannot copy the icon for the project: {}", e))?;
    }

    std::fs::write(contents.join("Info.plist"), info_plist(spec))
        .map_err(|e| format!("cannot write Info.plist: {}", e))?;
    Ok(())
}

/// The `Info.plist` for a packaged project.
pub fn info_plist(spec: &PackageSpec) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>{name}</string>
    <key>CFBundleDisplayName</key>
    <string>{name}</string>
    <key>CFBundleIdentifier</key>
    <string>{id}</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>{name}-launch</string>
    <key>CFBundleIconFile</key>
    <string>{name}</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
"#,
        name = spec.name,
        id = spec.bundle_id,
        version = spec.version,
    )
}

/// Write the app icon to `to` as a real `.icns`, which Finder needs. An
/// `.icns` is copied as it is; any other picture (a PNG) is resized into an
/// icon set with `sips` and built with `iconutil`, both part of macOS.
fn write_icns(from: &Path, to: &Path) -> Result<(), String> {
    let is_icns = from
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("icns"));
    if is_icns {
        return std::fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| format!("cannot copy the icon: {}", e));
    }
    let work = std::env::temp_dir().join(format!(
        "oxbasic-icon-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&work);
    let set = work.join("AppIcon.iconset");
    std::fs::create_dir_all(&set).map_err(|e| format!("cannot make the icon set: {}", e))?;
    // the sizes an .icns holds, and the 2x names of the same sizes
    let sizes: [(u32, &str); 10] = [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ];
    for (px, name) in sizes {
        let status = std::process::Command::new("sips")
            .arg("-z")
            .arg(px.to_string())
            .arg(px.to_string())
            .arg(from)
            .arg("--out")
            .arg(set.join(name))
            .output()
            .map_err(|e| format!("cannot run sips: {}", e))?;
        if !status.status.success() {
            let _ = std::fs::remove_dir_all(&work);
            return Err(format!(
                "cannot make the icon from {}: {}",
                from.display(),
                String::from_utf8_lossy(&status.stderr).trim()
            ));
        }
    }
    let status = std::process::Command::new("iconutil")
        .arg("-c")
        .arg("icns")
        .arg(&set)
        .arg("-o")
        .arg(to)
        .output()
        .map_err(|e| format!("cannot run iconutil: {}", e))?;
    let _ = std::fs::remove_dir_all(&work);
    if !status.status.success() {
        return Err(format!(
            "cannot build the .icns: {}",
            String::from_utf8_lossy(&status.stderr).trim()
        ));
    }
    Ok(())
}


#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("cannot make {} executable: {}", path.display(), e))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name)
    }

    #[test]
    fn the_showcase_packages_its_project_forms_and_modules() {
        let files = project_files(&example("showcase/showcase.oxproj")).expect("reads");
        let names: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"showcase.oxproj".to_string()));
        assert!(names.contains(&"Form1.oxform".to_string()));
        assert!(names.contains(&"Form1.bas".to_string()));
        assert!(names.contains(&"frmAbout.oxform".to_string()));
        assert!(names.contains(&"Module1.bas".to_string()));
    }

    #[test]
    fn a_picture_a_form_names_is_packaged() {
        // a project of its own, whose form shows a picture (and a mouse icon),
        // so the test doesn't depend on which example uses one
        let dir = std::env::temp_dir().join(format!("oxide-pkg-pic-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let layout = serde_json::json!({
            "name": "Form1",
            "caption": "Pics",
            "width": 200.0,
            "height": 200.0,
            "controls": [
                {"name": "picShow", "kind": "PictureBox", "picture": "logo.png",
                 "rect": {"left": 0.0, "top": 0.0, "width": 50.0, "height": 50.0}},
                {"name": "cmdGo", "kind": "CommandButton", "mouseIcon": "cursor.png",
                 "rect": {"left": 0.0, "top": 60.0, "width": 50.0, "height": 20.0}}
            ]
        });
        std::fs::write(dir.join("Form1.oxform"), layout.to_string()).unwrap();
        std::fs::write(dir.join("Form1.bas"), "Sub Form_Load()\nEnd Sub\n").unwrap();
        let project = serde_json::json!({
            "name": "pics",
            "startup": "Form1",
            "forms": [{"name": "Form1", "layout": "Form1.oxform", "code": "Form1.bas"}],
            "modules": []
        });
        std::fs::write(dir.join("pics.oxproj"), project.to_string()).unwrap();

        let files = project_files(&dir.join("pics.oxproj")).expect("reads");
        let names: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"logo.png".to_string()), "{:?}", names);
        assert!(names.contains(&"cursor.png".to_string()), "{:?}", names);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn each_file_is_listed_once() {
        let files = project_files(&example("showcase/showcase.oxproj")).expect("reads");
        let mut sorted = files.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), files.len());
    }

    #[test]
    fn a_missing_project_is_an_error() {
        assert!(project_files(&example("nope.oxproj")).is_err());
    }
}

#[cfg(test)]
mod bundle_tests {
    use super::*;

    /// A one-form project in a folder of its own, with a picture.
    fn make_project(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("oxide-pkg-{}-{}", tag, std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let layout = serde_json::json!({
            "name": "Form1",
            "caption": "Hello",
            "width": 200.0,
            "height": 120.0,
            "controls": [
                {"name": "picShow", "kind": "PictureBox", "picture": "logo.png",
                 "rect": {"left": 0.0, "top": 0.0, "width": 50.0, "height": 50.0}}
            ]
        });
        std::fs::write(dir.join("Form1.oxform"), layout.to_string()).unwrap();
        std::fs::write(dir.join("Form1.bas"), "Sub Form_Load()\nEnd Sub\n").unwrap();
        std::fs::write(dir.join("logo.png"), b"not really a png").unwrap();
        let project = serde_json::json!({
            "name": "hello",
            "startup": "Form1",
            "forms": [{"name": "Form1", "layout": "Form1.oxform", "code": "Form1.bas"}],
            "modules": []
        });
        std::fs::write(dir.join("hello.oxproj"), project.to_string()).unwrap();
        (dir.clone(), dir.join("hello.oxproj"))
    }

    /// A stand-in executable, so the test doesn't need a built binary.
    fn fake_binary(dir: &Path) -> PathBuf {
        let p = dir.join("oxbasic-fake");
        std::fs::write(&p, "#!/bin/sh\necho fake\n").unwrap();
        p
    }

    fn spec(dir: &Path, project: PathBuf, bundle: PathBuf, name: &str) -> PackageSpec {
        PackageSpec {
            project,
            bundle,
            name: name.into(),
            icon: None,
            binary: fake_binary(dir),
            version: "1.0.0".into(),
            bundle_id: bundle_identifier(name),
        }
    }

    #[test]
    fn the_bundle_has_the_app_layout_and_the_projects_files() {
        let (dir, project) = make_project("layout");
        let bundle = dir.join("out").join("Hello.app");
        write_package(&spec(&dir, project, bundle.clone(), "hello")).expect("packages");
        assert!(bundle.join("Contents/MacOS/hello").exists(), "the program");
        assert!(
            bundle.join("Contents/MacOS/hello-launch").exists(),
            "the launcher"
        );
        assert!(bundle.join("Contents/Info.plist").exists());
        let inside = bundle.join("Contents/Resources/project");
        assert!(inside.join("hello.oxproj").exists());
        assert!(inside.join("Form1.oxform").exists());
        assert!(inside.join("Form1.bas").exists());
        assert!(inside.join("logo.png").exists(), "the picture is carried");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_launcher_plays_the_bundled_project() {
        let (dir, project) = make_project("launcher");
        let bundle = dir.join("Hello.app");
        write_package(&spec(&dir, project, bundle.clone(), "hello")).unwrap();
        let launch = std::fs::read_to_string(bundle.join("Contents/MacOS/hello-launch")).unwrap();
        assert!(launch.contains("hello\" play"), "{}", launch);
        assert!(
            launch.contains("../Resources/project/hello.oxproj"),
            "{}",
            launch
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_plist_names_the_app_and_its_launcher() {
        let s = PackageSpec {
            project: PathBuf::from("/x/hello.oxproj"),
            bundle: PathBuf::from("/x/Hello.app"),
            name: "Hello World".into(),
            icon: None,
            binary: PathBuf::from("/x/bin"),
            version: "2.3.4".into(),
            bundle_id: "com.acme.hello".into(),
        };
        let plist = info_plist(&s);
        assert!(plist.contains("<string>Hello World</string>"));
        assert!(plist.contains("<string>Hello World-launch</string>"));
        assert!(plist.contains("<key>CFBundleIdentifier</key>"));
        assert!(plist.contains("com.acme.hello"), "{}", plist);
    }

    #[test]
    fn the_identifier_is_letters_and_digits_only() {
        assert_eq!(
            bundle_identifier("My Game 2!"),
            "com.oxidizedbasic.app.mygame2"
        );
        assert_eq!(bundle_identifier("!!!"), "com.oxidizedbasic.app.project");
    }

    #[test]
    fn packaging_again_replaces_the_old_bundle() {
        let (dir, project) = make_project("again");
        let bundle = dir.join("Hello.app");
        let s = spec(&dir, project, bundle.clone(), "hello");
        write_package(&s).unwrap();
        std::fs::write(bundle.join("stale.txt"), "old").unwrap();
        write_package(&s).unwrap();
        assert!(
            !bundle.join("stale.txt").exists(),
            "the old contents are gone"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let (dir, project) = make_project("nobin");
        let mut s = spec(&dir, project, dir.join("Hello.app"), "hello");
        s.binary = dir.join("does-not-exist");
        assert!(write_package(&s).is_err(), "a missing program is reported");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(test)]
    mod manual_bundle {
        use super::*;

        /// Writes the showcase's package to /tmp/oxbasic-showcase-check.app so it
        /// can be run by hand. Ignored by default; run it with
        /// `cargo test -p oxbasic_ide write_showcase_bundle -- --ignored`.
        #[test]
        #[ignore = "writes a bundle to /tmp for a manual check"]
        fn write_showcase_bundle() {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bundle = PathBuf::from("/tmp/oxbasic-showcase-check.app");
            let icon = root.join("packaging/icons/OxidizedBASIC.icns");
            write_package(&PackageSpec {
                project: root.join("examples/showcase/showcase.oxproj"),
                bundle: bundle.clone(),
                name: "Showcase".into(),
                icon: icon.exists().then_some(icon),
                binary: root.join("target/debug/oxbasic"),
                version: "1.0.0".into(),
                bundle_id: bundle_identifier("Showcase"),
            })
            .expect("packages");
            assert!(bundle.join("Contents/MacOS/Showcase-launch").exists());
        }
    }

    #[cfg(test)]
    mod settings_tests {
        use super::*;

        /// A project folder with a one-form project whose file gives `extra` (more
        /// project fields, as JSON text). Returns the project file's path.
        fn project_with(tag: &str, extra: &str) -> PathBuf {
            let dir =
                std::env::temp_dir().join(format!("oxide-set-{}-{}", tag, std::process::id()));
            std::fs::remove_dir_all(&dir).ok();
            std::fs::create_dir_all(&dir).unwrap();
            let layout = serde_json::json!({
                "name": "Form1", "caption": "Hi", "width": 200.0, "height": 120.0,
                "controls": []
            });
            std::fs::write(dir.join("Form1.oxform"), layout.to_string()).unwrap();
            std::fs::write(dir.join("Form1.bas"), "Sub Form_Load()\nEnd Sub\n").unwrap();
            std::fs::write(dir.join("icon.icns"), b"icns").unwrap();
            let text = format!(
                r#"{{"name": "Game", "startup": "Form1",
                "forms": [{{"name": "Form1", "layout": "Form1.oxform", "code": "Form1.bas"}}],
                "modules": []{}}}"#,
                extra
            );
            let file = dir.join("game.oxproj");
            std::fs::write(&file, text).unwrap();
            file
        }

        #[test]
        fn the_version_identifier_and_icon_come_from_the_project() {
            let file = project_with(
                "given",
                r#", "version": "2.1.0", "bundleId": "com.acme.game", "icon": "icon.icns""#,
            );
            let s = package_settings(&file).expect("reads");
            assert_eq!(s.version, "2.1.0");
            assert_eq!(s.bundle_id, "com.acme.game");
            assert_eq!(
                s.icon.as_deref(),
                Some(file.parent().unwrap().join("icon.icns").as_path())
            );
            std::fs::remove_dir_all(file.parent().unwrap()).ok();
        }

        #[test]
        fn a_project_that_says_nothing_gets_the_defaults() {
            let file = project_with("defaults", "");
            let s = package_settings(&file).expect("reads");
            assert_eq!(s.version, "1.0.0", "the default version");
            assert_eq!(
                s.bundle_id, "com.oxidizedbasic.app.game",
                "one derived from the name"
            );
            assert!(s.icon.is_none(), "no icon: the default one");
            std::fs::remove_dir_all(file.parent().unwrap()).ok();
        }

        #[test]
        fn a_named_icon_that_is_missing_is_an_error() {
            let file = project_with("noicon", r#", "icon": "gone.icns""#);
            let err = package_settings(&file).unwrap_err();
            assert!(err.contains("gone.icns"), "{}", err);
            std::fs::remove_dir_all(file.parent().unwrap()).ok();
        }

        #[test]
        fn the_bundle_carries_the_projects_version_and_identifier() {
            let file = project_with(
                "plist",
                r#", "version": "3.0.1", "bundleId": "com.acme.plist""#,
            );
            let bundle = file.parent().unwrap().join("Game.app");
            let s = package_settings(&file).unwrap();
            write_package(&PackageSpec {
                project: file.clone(),
                bundle: bundle.clone(),
                name: "Game".into(),
                icon: s.icon.clone(),
                binary: std::env::current_exe().unwrap(),
                version: s.version.clone(),
                bundle_id: s.bundle_id.clone(),
            })
            .expect("packages");
            let plist = std::fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap();
            assert!(plist.contains("<string>3.0.1</string>"), "{}", plist);
            assert!(
                plist.contains("<string>com.acme.plist</string>"),
                "{}",
                plist
            );
            std::fs::remove_dir_all(file.parent().unwrap()).ok();
        }

        #[test]
        fn the_settings_survive_opening_and_saving_the_project() {
            use crate::ide::workspace::Workspace;
            let file = project_with(
                "roundtrip",
                r#", "version": "4.2.0", "bundleId": "com.acme.keep", "icon": "icon.icns""#,
            );
            let mut w = Workspace::open(&file).expect("opens");
            assert_eq!(w.version, "4.2.0");
            assert_eq!(w.bundle_id, "com.acme.keep");
            assert_eq!(w.icon, "icon.icns");
            // a save writes them back, unchanged
            w.save_all(&file).expect("saves");
            let again = Workspace::open(&file).expect("reopens");
            assert_eq!(again.version, "4.2.0");
            assert_eq!(again.bundle_id, "com.acme.keep");
            assert_eq!(again.icon, "icon.icns");
            std::fs::remove_dir_all(file.parent().unwrap()).ok();
        }
}

#[cfg(test)]
mod icon_tests {
    use super::*;
    use std::path::PathBuf;

    /// A PNG icon becomes a real .icns (macOS only: it needs sips and iconutil).
    #[test]
    #[cfg(target_os = "macos")]
    fn a_png_icon_is_written_as_a_real_icns() {
        let png = example_icon_png();
        let out = std::env::temp_dir().join(format!("oxbasic-test-{}.icns", std::process::id()));
        let _ = std::fs::remove_file(&out);
        write_icns(&png, &out).expect("the PNG becomes an icns");
        let bytes = std::fs::read(&out).expect("the .icns exists");
        assert_eq!(&bytes[0..4], b"icns", "the file starts with the icns header");
        let _ = std::fs::remove_file(&out);
    }

    /// A small PNG to convert: the showcase's icon, which the repo ships.
    #[cfg(target_os = "macos")]
    fn example_icon_png() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/showcase/OxidizedBASIC Showcase Icon.png")
    }

    /// A project with a PNG icon packages, and the bundled project names an
    /// icon that is really in the bundle (else the app stops at start-up).
    #[test]
    #[cfg(target_os = "macos")]
    fn the_bundled_project_names_an_icon_the_bundle_contains() {
        let (dir, project) = make_project("iconbundle");
        let bundle = dir.join("out").join("Icon.app");
        let mut s = spec(&dir, project, bundle.clone(), "icon");
        s.icon = Some(example_icon_png());
        write_package(&s).expect("packages");
        let inside = bundle.join("Contents/Resources/project");
        let bundled = oxbasic_ui::project::Project::load(&inside.join("hello.oxproj"))
            .expect("the bundled project loads");
        assert_eq!(bundled.icon, "icon.icns", "the project names the icon it ships");
        assert!(inside.join(&bundled.icon).exists(), "and that icon is in the bundle");
        // Finder reads the app icon from Resources, the name Info.plist gives
        let plist = std::fs::read_to_string(bundle.join("Contents/Info.plist")).expect("plist");
        assert!(plist.contains("<string>icon</string>"), "Info.plist names the icon");
        assert!(
            bundle.join("Contents/Resources/icon.icns").exists(),
            "Finder finds the app icon in Resources"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

}
