//! The File menu's recent projects in the IDE: opening and saving record a
//! project, the menu lists the last few, choosing one opens it, and a file
//! that has gone is dropped with a message. Each test uses its own recent-
//! projects file, so the user's real list is never touched.

use super::project_tests::Dir;
use super::toolbox_tests::ide_window;
use super::*;

/// Point the IDE at a recent-projects file of its own, empty to start with.
fn own_recent_file(ide: &mut Ide, dir: &Dir) {
    ide.recent_file = dir.0.join("recent.json");
    ide.recent = recent::RecentList::default();
}

#[gpui::test]
fn saving_a_project_records_it_as_most_recent(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("recent-save");
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, _| {
        own_recent_file(ide, &dir);
        ide.save_project_to(dir.0.join("alpha.oxproj")).unwrap();
        ide.save_project_to(dir.0.join("beta.oxproj")).unwrap();
    });
    ide.read_with(cx, |ide, _| {
        let items = ide.recent.items();
        assert_eq!(items.len(), 2);
        // the last saved file is first, identified by its path
        assert!(
            items[0].path.ends_with("beta.oxproj"),
            "{:?}",
            items[0].path
        );
        assert!(items[1].path.ends_with("alpha.oxproj"));
        // the name is the project's: beta was saved from the project named
        // alpha, so it keeps that name (only a new project takes its file's)
        assert_eq!(items[0].name, "alpha");
        assert!(ide.recent_file.exists(), "the list is written to its file");
    });
}

#[gpui::test]
fn the_list_is_kept_between_runs(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("recent-runs");
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, _| {
        own_recent_file(ide, &dir);
        ide.save_project_to(dir.0.join("kept.oxproj")).unwrap();
    });
    // a fresh IDE that reads the same file sees the project
    let loaded = recent_store::load(&dir.0.join("recent.json"));
    assert_eq!(loaded.items().len(), 1);
    assert_eq!(loaded.items()[0].name, "kept");
}

#[gpui::test]
fn opening_a_recent_project_opens_it(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("recent-open");
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, _| {
        own_recent_file(ide, &dir);
        ide.save_project_to(dir.0.join("target.oxproj")).unwrap();
        // start on another project, then open the recent one
        ide.workspace.name = "other".into();
    });
    ide.update(cx, |ide, cx| {
        let path = ide.recent.items()[0].path.clone();
        ide.open_project_file(&path, cx);
    });
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.workspace.name, "target", "the recent project is open");
        assert_eq!(
            ide.recent.items()[0].name,
            "target",
            "and it is still first"
        );
        assert_eq!(ide.recent.items().len(), 1, "no duplicate was added");
    });
}

#[gpui::test]
fn a_project_that_has_gone_is_dropped_with_a_message(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("recent-gone");
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, _| {
        own_recent_file(ide, &dir);
        ide.save_project_to(dir.0.join("gone.oxproj")).unwrap();
        std::fs::remove_file(dir.0.join("gone.oxproj")).unwrap();
    });
    cx.update(|window, app| {
        ide.update(app, |ide, cx| ide.open_recent(0, window, cx));
    });
    ide.read_with(cx, |ide, _| {
        assert!(
            ide.recent.items().is_empty(),
            "the missing project is forgotten"
        );
        assert!(ide.status.contains("no longer there"), "{}", ide.status);
    });
}

#[gpui::test]
fn clear_menu_empties_the_list(cx: &mut gpui::TestAppContext) {
    let dir = Dir::new("recent-clear");
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, _| {
        own_recent_file(ide, &dir);
        ide.save_project_to(dir.0.join("one.oxproj")).unwrap();
    });
    ide.update(cx, |ide, cx| ide.clear_recent(cx));
    ide.read_with(cx, |ide, _| {
        assert!(ide.recent.items().is_empty());
        assert!(
            recent_store::load(&ide.recent_file).items().is_empty(),
            "stored empty too"
        );
    });
}

#[test]
fn the_menu_lists_the_recent_projects_with_name_and_path() {
    let mut list = recent::RecentList::default();
    list.record("alpha", std::path::Path::new("/nowhere/alpha.oxproj"));
    list.record("beta", std::path::Path::new("/nowhere/beta.oxproj"));
    let items = file_menu(&list);
    let recent = items
        .iter()
        .find_map(|i| match i {
            gpui::MenuItem::Submenu(m) if m.name.as_ref() == "Open Recent" => Some(m),
            _ => None,
        })
        .expect("an Open Recent submenu");
    // two projects, a separator and Clear Menu
    assert_eq!(recent.items.len(), 4);
    let labels: Vec<String> = recent
        .items
        .iter()
        .filter_map(|i| match i {
            gpui::MenuItem::Action { name, .. } => Some(name.to_string()),
            _ => None,
        })
        .collect();
    assert!(
        labels[0].starts_with("beta — /nowhere/beta.oxproj"),
        "{:?}",
        labels
    );
    assert!(
        labels[0].contains("(missing)"),
        "a file that is not there is marked"
    );
    assert_eq!(labels[2], "Clear Menu");
}

#[test]
fn an_empty_list_says_so_in_the_menu() {
    let items = file_menu(&recent::RecentList::default());
    let recent = items
        .iter()
        .find_map(|i| match i {
            gpui::MenuItem::Submenu(m) if m.name.as_ref() == "Open Recent" => Some(m),
            _ => None,
        })
        .expect("an Open Recent submenu");
    assert_eq!(recent.items.len(), 1);
}
