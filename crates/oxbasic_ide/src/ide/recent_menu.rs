//! The File menu's recent projects, as the IDE uses them: a project is
//! recorded when it is opened or saved to a file, the menu lists them, and
//! choosing one opens it. The list itself is in `recent.rs` and its storage in
//! `recent_store.rs`.

use super::*;

impl Ide {
    /// Record the project that was just opened or saved at `file`, and
    /// store the list. The menu is rebuilt so it shows the new order.
    pub(super) fn record_recent(&mut self, file: &std::path::Path, cx: &mut Context<Self>) {
        let name = self.workspace.name.clone();
        self.recent.record(&name, file);
        self.store_recent();
        self.refresh_file_menu(cx);
    }

    /// Write the list to its file; failing to is not shown to the user.
    pub(super) fn store_recent(&self) {
        recent_store::save(&self.recent_file, &self.recent);
    }

    /// File > Open Recent > entry `i`: open that project, after asking about
    /// unsaved work. A project whose file has gone is dropped from the list
    /// and the status line says so.
    pub(super) fn open_recent(&mut self, i: usize, win: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.recent.items().get(i).cloned() else {
            return;
        };
        if !entry.path.exists() {
            self.recent.forget(&entry.path);
            self.store_recent();
            self.refresh_file_menu(cx);
            self.status = format!("{} is no longer there", entry.path.display());
            cx.notify();
            return;
        }
        self.confirm_discard(win, cx, move |ide, _win, cx| {
            ide.open_project_file(&entry.path, cx);
        });
    }

    /// File > Open Recent > Clear Menu.
    pub(super) fn clear_recent(&mut self, cx: &mut Context<Self>) {
        self.recent.clear();
        self.store_recent();
        self.refresh_file_menu(cx);
        cx.notify();
    }

    /// Set the menus again so the File menu shows the current list. GPUI
    /// keeps the menus it was last given, so this runs after every change.
    pub(super) fn refresh_file_menu(&self, cx: &mut Context<Self>) {
        cx.set_menus(file_menu_bar(&self.recent));
    }
}

/// The whole menu bar: the app menu, File (with Open Recent), Edit, View, and
/// Run. Built here so the start-up menus and a refresh use the same list.
pub(super) fn file_menu_bar(recent: &recent::RecentList) -> Vec<gpui::Menu> {
    vec![
        gpui::Menu {
            name: "OxidizedBASIC".into(),
            items: vec![gpui::MenuItem::action("Quit OxidizedBASIC", Quit)],
        },
        gpui::Menu {
            name: "File".into(),
            items: file_menu(recent),
        },
        gpui::Menu {
            name: "Edit".into(),
            items: vec![
                gpui::MenuItem::action("Undo", EditUndo),
                gpui::MenuItem::action("Redo", EditRedo),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("Cut", EditCut),
                gpui::MenuItem::action("Copy", EditCopy),
                gpui::MenuItem::action("Paste", EditPaste),
                gpui::MenuItem::action("Select All", EditSelectAll),
                gpui::MenuItem::separator(),
                gpui::MenuItem::action("Find…", EditFind),
                gpui::MenuItem::action("Replace…", EditReplace),
                gpui::MenuItem::action("Find Next", EditFindNext),
                gpui::MenuItem::action("Find Previous", EditFindPrevious),
            ],
        },
        gpui::Menu {
            name: "View".into(),
            items: vec![
                gpui::MenuItem::action("Code", ViewCode),
                gpui::MenuItem::action("Object", ViewObject),
            ],
        },
        gpui::Menu {
            name: "Run".into(),
            items: vec![
                gpui::MenuItem::action("Run / Continue", RunProgram),
                gpui::MenuItem::action("Step Over", StepProgram),
                gpui::MenuItem::action("Stop", StopProgram),
            ],
        },
    ]
}
