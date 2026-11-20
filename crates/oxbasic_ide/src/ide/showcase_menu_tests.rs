//! The showcase's menu bar: its items are the menu data of Form1, and the
//! handlers they name run the program's code (Name_Click).

use super::debug_session::{DebugCommand, DebugEvent};
use super::showcase::showcase;
use super::showcase_run::Run;
use oxbasic_ui::menu::{click_handler, MenuItem};

/// Form1's menu items, as the showcase builds them (see `main_form`).
fn form1_menus() -> Vec<MenuItem> {
    let w = showcase(|name| match name {
        "Form1" => "Sub Form_Load()\nEnd Sub\n".into(),
        _ => String::new(),
    });
    w.docs
        .iter()
        .find(|d| d.name == "Form1")
        .expect("Form1")
        .form
        .menus
        .clone()
}

fn index_of(menus: &[MenuItem], name: &str) -> usize {
    menus
        .iter()
        .position(|m| m.name == name)
        .unwrap_or_else(|| panic!("no menu item {}", name))
}

#[test]
fn the_menu_has_file_and_help_with_the_items_under_them() {
    let menus = form1_menus();
    let tops: Vec<&str> = menus
        .iter()
        .filter(|m| m.parent.is_none())
        .map(|m| m.caption.as_str())
        .collect();
    assert_eq!(tops, vec!["&File", "&Help"]);
    let drawing = &menus[index_of(&menus, "mnuDrawing")];
    assert_eq!(drawing.shortcut, "Ctrl+D");
    assert!(oxbasic_ui::menu::shortcut_is_valid(&drawing.shortcut));
}

#[test]
fn a_separator_is_not_a_command() {
    let menus = form1_menus();
    assert!(menus[index_of(&menus, "mnuSep")].is_separator());
}

#[test]
fn about_is_under_help() {
    let menus = form1_menus();
    let about = index_of(&menus, "mnuAbout");
    let parent = menus[about].parent.expect("About has a parent");
    assert_eq!(menus[parent].caption, "&Help");
}

#[test]
fn choosing_drawing_runs_its_handler_and_shows_the_drawing_form() {
    let run = Run::start();
    let menus = form1_menus();
    let handler = click_handler(&menus[index_of(&menus, "mnuDrawing")]);
    assert_eq!(handler, "mnuDrawing_Click");
    run.session.send(DebugCommand::Call(0, handler, Vec::new()));
    run.until(|e| matches!(e, DebugEvent::FormShown(s) if s.form_index == 2));
}

#[test]
fn choosing_about_runs_its_handler_and_shows_the_about_form() {
    let run = Run::start();
    let menus = form1_menus();
    let handler = click_handler(&menus[index_of(&menus, "mnuAbout")]);
    run.session.send(DebugCommand::Call(0, handler, Vec::new()));
    run.until(|e| matches!(e, DebugEvent::FormShown(s) if s.form_index == 1));
}
