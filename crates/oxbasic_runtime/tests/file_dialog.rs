//! `OpenFileDialog` / `SaveFileDialog`: the path the user chose, or "" when
//! the dialog is cancelled or the choice doesn't match the filter.

use oxbasic_runtime::{run_source, Output};

/// An output whose dialog returns `choice` and records the filter it was given.
struct Dialog {
    out: String,
    choice: Option<String>,
    filters: Vec<(String, bool)>,
}

impl Output for Dialog {
    fn write(&mut self, s: &str) {
        self.out.push_str(s);
    }
    fn choose_file(&mut self, filter: &str, save: bool) -> Option<String> {
        self.filters.push((filter.to_string(), save));
        self.choice.clone()
    }
}

fn run(src: &str, choice: Option<&str>) -> (String, Dialog) {
    let mut d = Dialog {
        out: String::new(),
        choice: choice.map(|s| s.to_string()),
        filters: Vec::new(),
    };
    run_source(src, &mut d).expect("run failed");
    (d.out.clone(), d)
}

#[test]
fn a_chosen_file_that_matches_the_filter_is_returned() {
    let (out, d) = run(
        "Print OpenFileDialog(\"*.bas;*.oxproj\")\n",
        Some("/tmp/Game.oxproj"),
    );
    assert_eq!(out, "/tmp/Game.oxproj\n");
    assert_eq!(d.filters, vec![("*.bas;*.oxproj".to_string(), false)]);
}

#[test]
fn a_cancelled_dialog_gives_an_empty_string() {
    let (out, _) = run("Print \"[\" & OpenFileDialog(\"*.bas\") & \"]\"\n", None);
    assert_eq!(out, "[]\n");
}

#[test]
fn a_choice_the_filter_rejects_gives_an_empty_string() {
    let (out, _) = run(
        "Print \"[\" & OpenFileDialog(\"*.bas\") & \"]\"\n",
        Some("/tmp/notes.txt"),
    );
    assert_eq!(out, "[]\n");
}

#[test]
fn save_dialog_is_marked_as_a_save() {
    let (_, d) = run(
        "Print SaveFileDialog(\"*.oxproj\")\n",
        Some("/tmp/New.oxproj"),
    );
    assert_eq!(d.filters, vec![("*.oxproj".to_string(), true)]);
}

#[test]
fn an_empty_filter_accepts_any_file() {
    let (out, _) = run("Print OpenFileDialog()\n", Some("/tmp/anything.xyz"));
    assert_eq!(out, "/tmp/anything.xyz\n");
}
