//! The showcase sample project: a two-form program that uses every
//! control the toolbox offers. It is built here from code, written with
//! the real `Workspace::save_all`, and kept in `examples/showcase/`; a test
//! checks the checked-in files still match, so the sample can't rot.
//!
//! Regenerate it with:
//!
//! ```text
//! OXBASIC_WRITE_SHOWCASE=1 cargo test -p oxbasic_ide showcase
//! ```

use super::workspace::{to_lines, Doc, DocKind, Workspace};
use oxbasic_ui::{ControlKind, ControlSpec, FormSpec, Rect};

fn control(name: &str, kind: ControlKind, rect: (f32, f32, f32, f32)) -> ControlSpec {
    ControlSpec::new(name, kind, Rect::new(rect.0, rect.1, rect.2, rect.3))
}

fn captioned(
    name: &str,
    kind: ControlKind,
    caption: &str,
    rect: (f32, f32, f32, f32),
) -> ControlSpec {
    let mut c = control(name, kind, rect);
    c.caption = caption.into();
    c
}

fn items(name: &str, kind: ControlKind, rect: (f32, f32, f32, f32), list: &[&str]) -> ControlSpec {
    let mut c = control(name, kind, rect);
    c.items = list.iter().map(|s| s.to_string()).collect();
    if kind.has_text() {
        c.text = Some(String::new());
    }
    c
}

/// Form1: one of each control, and the code that drives them.
fn main_form() -> FormSpec {
    use ControlKind::*;
    let mut f = FormSpec::new("Form1", "OxidizedBASIC Control Showcase", 620.0, 470.0);

    // -- left column: text -------------------------------------------------
    let mut title = captioned(
        "lblTitle",
        Label,
        "Control Showcase",
        (16.0, 12.0, 300.0, 26.0),
    );
    title.font.bold = true;
    title.font.size = 14.0;
    title.fore_color = Some(0x0A3D91);
    f.control(title);
    f.control(captioned(
        "lblName",
        Label,
        "Your name:",
        (16.0, 52.0, 80.0, 20.0),
    ));
    let mut name = control("txtName", TextBox, (100.0, 48.0, 190.0, 24.0));
    name.text = Some(String::new());
    f.control(name);
    f.control(captioned(
        "cmdGreet",
        CommandButton,
        "Greet",
        (300.0, 46.0, 90.0, 28.0),
    ));
    let mut greeting = captioned(
        "lblGreeting",
        Label,
        "Type a name and press Greet.",
        (16.0, 84.0, 374.0, 22.0),
    );
    greeting.color = Some(0xFFF6D6);
    f.control(greeting);

    // -- options: CheckBox and OptionButtons in a Frame ---------------------
    f.control(captioned(
        "fraStyle",
        Frame,
        "Greeting style",
        (16.0, 118.0, 180.0, 118.0),
    ));
    f.control(captioned(
        "chkShout",
        CheckBox,
        "Shout it",
        (28.0, 142.0, 150.0, 22.0),
    ));
    let mut formal = captioned(
        "optFormal",
        OptionButton,
        "Formal",
        (28.0, 170.0, 150.0, 22.0),
    );
    formal.value = 1;
    f.control(formal);
    f.control(captioned(
        "optCasual",
        OptionButton,
        "Casual",
        (28.0, 196.0, 150.0, 22.0),
    ));

    // -- lists --------------------------------------------------------------
    f.control(captioned(
        "lblColour",
        Label,
        "Colour:",
        (212.0, 124.0, 60.0, 20.0),
    ));
    f.control(items(
        "cboColour",
        ComboBox,
        (212.0, 146.0, 178.0, 24.0),
        &["Red", "Green", "Blue", "Gold"],
    ));
    f.control(captioned(
        "lblFruit",
        Label,
        "Fruit basket:",
        (16.0, 248.0, 100.0, 20.0),
    ));
    f.control(items(
        "lstFruit",
        ListBox,
        (16.0, 270.0, 180.0, 110.0),
        &["Apple", "Banana", "Cherry"],
    ));
    f.control(captioned(
        "cmdAdd",
        CommandButton,
        "Add fruit",
        (212.0, 270.0, 100.0, 28.0),
    ));
    f.control(captioned(
        "cmdRemove",
        CommandButton,
        "Remove",
        (320.0, 270.0, 70.0, 28.0),
    ));
    f.control(captioned(
        "cmdClear",
        CommandButton,
        "Clear list",
        (212.0, 306.0, 178.0, 28.0),
    ));

    // -- a control array: three buttons, one handler ------------------------
    f.control(captioned(
        "lblPick",
        Label,
        "Control array:",
        (410.0, 52.0, 120.0, 20.0),
    ));
    for i in 0..3 {
        let mut b = captioned(
            "cmdSwatch",
            CommandButton,
            &format!("Swatch {}", i + 1),
            (410.0 + 68.0 * i as f32, 76.0, 64.0, 28.0),
        );
        b.index = Some(i);
        f.control(b);
    }
    let mut swatch = control("picSwatch", PictureBox, (410.0, 118.0, 196.0, 118.0));
    swatch.color = Some(0xFFFFFF);
    f.control(swatch);

    // -- clock and status ---------------------------------------------------
    f.control(captioned(
        "lblClock",
        Label,
        "Clock:",
        (410.0, 248.0, 196.0, 22.0),
    ));
    f.control(captioned(
        "cmdClock",
        CommandButton,
        "Start / stop clock",
        (410.0, 274.0, 196.0, 28.0),
    ));
    let mut tick = control("tmrClock", Timer, (0.0, 0.0, 0.0, 0.0));
    tick.interval_ms = 1000;
    tick.enabled = false;
    tick.visible = false;
    f.control(tick);
    f.control(captioned(
        "cmdAbout",
        CommandButton,
        "About…",
        (410.0, 310.0, 96.0, 28.0),
    ));
    f.control(captioned(
        "cmdQuit",
        CommandButton,
        "Quit",
        (510.0, 310.0, 96.0, 28.0),
    ));
    let mut status = captioned("lblStatus", Label, "Ready", (16.0, 400.0, 590.0, 22.0));
    status.fore_color = Some(0x555555);
    f.control(status);
    f
}

/// frmAbout: opened from Form1, with its own labels and buttons.
fn about_form() -> FormSpec {
    use ControlKind::*;
    let mut f = FormSpec::new("frmAbout", "About the Showcase", 360.0, 220.0);
    let mut head = captioned("lblHead", Label, "OxidizedBASIC", (20.0, 16.0, 320.0, 28.0));
    head.font.bold = true;
    head.font.size = 16.0;
    f.control(head);
    f.control(captioned(
        "lblVersion",
        Label,
        "(set by Form1)",
        (20.0, 56.0, 320.0, 22.0),
    ));
    let mut note = captioned("lblNote", Label, "", (20.0, 84.0, 320.0, 22.0));
    note.fore_color = Some(0x0A3D91);
    f.control(note);
    f.control(captioned(
        "cmdHide",
        CommandButton,
        "Hide",
        (20.0, 150.0, 100.0, 30.0),
    ));
    f.control(captioned(
        "cmdClose",
        CommandButton,
        "Close",
        (240.0, 150.0, 100.0, 30.0),
    ));
    f
}

/// The showcase as a workspace: two forms and a module, with the code in
/// `examples/showcase/*.bas`.
pub fn showcase(code: impl Fn(&str) -> String) -> Workspace {
    let mut forms = [("Form1", main_form()), ("frmAbout", about_form())];
    let mut docs = Vec::new();
    for (name, form) in forms.iter_mut() {
        docs.push(Doc::saved(
            DocKind::Form,
            name,
            None,
            None,
            form.clone(),
            to_lines(&code(name)),
        ));
    }
    docs.push(Doc::saved(
        DocKind::Module,
        "Module1",
        None,
        None,
        FormSpec::new("Module1", "Module1", 0.0, 0.0),
        to_lines(&code("Module1")),
    ));
    let mut w = Workspace::new_project();
    w.name = "showcase".into();
    w.startup = "Form1".into();
    w.docs = docs;
    w
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ide::debug_session::*;
    use crate::ide::program::combine;
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// `examples/showcase/` in the repository.
    fn dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase")
    }

    /// The code the sample ships with (`<name>.bas`).
    fn shipped_code(name: &str) -> String {
        std::fs::read_to_string(dir().join(format!("{}.bas", name)))
            .unwrap_or_else(|e| panic!("examples/showcase/{}.bas: {}", name, e))
    }

    /// Write the sample (layouts, project file) next to its code when
    /// `OXBASIC_WRITE_SHOWCASE=1`; the `.bas` files are written by hand.
    #[test]
    fn regenerate_when_asked() {
        if std::env::var("OXBASIC_WRITE_SHOWCASE").ok().as_deref() != Some("1") {
            return;
        }
        let mut w = showcase(shipped_code);
        w.save_all(&dir().join("showcase.oxproj")).unwrap();
    }

    #[test]
    fn the_checked_in_project_opens_and_matches_the_generator() {
        let file = dir().join("showcase.oxproj");
        let opened = Workspace::open(&file).expect("examples/showcase/showcase.oxproj opens");
        let built = showcase(shipped_code);
        assert_eq!(opened.name, "showcase");
        assert_eq!(opened.startup, "Form1");
        let names =
            |w: &Workspace| -> Vec<String> { w.docs.iter().map(|d| d.name.clone()).collect() };
        assert_eq!(names(&opened), names(&built));
        for (a, b) in opened.docs.iter().zip(&built.docs) {
            assert_eq!(a.lines, b.lines, "{}'s code", a.name);
            assert_eq!(
                a.form.to_oxform(),
                b.form.to_oxform(),
                "{}'s layout",
                a.name
            );
        }
    }

    #[test]
    fn it_uses_every_kind_of_control() {
        let w = showcase(shipped_code);
        let mut kinds: Vec<ControlKind> = w
            .docs
            .iter()
            .flat_map(|d| d.form.controls.iter().map(|c| c.kind))
            .collect();
        kinds.sort_by_key(|k| k.basic_name());
        kinds.dedup();
        let want = [
            ControlKind::CommandButton,
            ControlKind::Label,
            ControlKind::TextBox,
            ControlKind::Frame,
            ControlKind::ListBox,
            ControlKind::ComboBox,
            ControlKind::PictureBox,
            ControlKind::Timer,
            ControlKind::CheckBox,
            ControlKind::OptionButton,
        ];
        for k in want {
            assert!(kinds.contains(&k), "the showcase has no {:?}", k);
        }
    }

    #[test]
    fn its_controls_have_distinct_names_and_array_indexes() {
        let w = showcase(shipped_code);
        for d in w.docs.iter().filter(|d| d.kind == DocKind::Form) {
            let mut seen: Vec<(String, Option<i32>)> = Vec::new();
            for c in &d.form.controls {
                let key = (c.name.to_ascii_lowercase(), c.index);
                assert!(!seen.contains(&key), "{}: duplicate {:?}", d.name, key);
                seen.push(key);
            }
            // nothing sits outside the form
            for c in d
                .form
                .controls
                .iter()
                .filter(|c| c.kind != ControlKind::Timer)
            {
                assert!(
                    c.rect.left + c.rect.width <= d.form.width
                        && c.rect.top + c.rect.height <= d.form.height,
                    "{}.{} is outside the form",
                    d.name,
                    c.name
                );
            }
        }
    }

    #[test]
    fn the_combined_program_parses() {
        let w = showcase(shipped_code);
        let program = combine(&w.docs);
        oxbasic_parser::parse(&program.source)
            .unwrap_or_else(|e| panic!("the showcase's code doesn't parse: {}", e));
    }
}
