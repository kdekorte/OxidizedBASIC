//! The IDE's project commands: the File menu (New, Open, Save, Save As,
//! Close), switching between the project's forms and modules, and the
//! "save your changes?" prompt before anything that would lose them.
//!
//! The active document's `form` and `lines` live on [`Ide`] as the working
//! copy the designer, inspector and code window edit. [`Ide::store_active`]
//! copies them back into the [`Workspace`](super::workspace::Workspace)
//! before the document is saved or another is shown.

use super::workspace::{DocKind, Workspace};
use super::*;
use gpui::PromptLevel;
use std::path::PathBuf;

impl Ide {
    /// Copy the working copy into the active document.
    pub(super) fn store_active(&mut self) {
        let (form, lines, cursor) = (self.form.clone(), self.lines.clone(), self.cursor);
        let bps = self.breakpoints.lock().unwrap().clone();
        let doc = self.workspace.active_doc_mut();
        if doc.kind == DocKind::Form {
            doc.form = form;
        }
        doc.lines = lines;
        doc.cursor = cursor;
        doc.breakpoints = bps;
    }

    /// Make the active document the working copy.
    pub(super) fn load_active(&mut self) {
        let doc = self.workspace.active_doc().clone();
        self.form = doc.form;
        self.lines = doc.lines;
        self.cursor = doc.cursor;
        // a different text: its selection and undo history start afresh
        self.anchor = None;
        self.selecting = false;
        self.history.clear();
        *self.breakpoints.lock().unwrap() = doc.breakpoints;
        self.selected = None;
        self.drag = None;
        self.prop_bufs.clear();
        self.rename_error = None;
        self.code_object = None;
        self.reveal_cursor = true;
    }

    /// Show document `index` (storing the current one first).
    pub(super) fn switch_to(&mut self, index: usize) {
        if index >= self.workspace.docs.len() || index == self.workspace.active {
            return;
        }
        self.commit_pending_rename();
        self.store_active();
        self.workspace.activate(index);
        self.load_active();
        self.status = format!("editing {}", self.workspace.active_doc().name);
    }

    /// Whether the working copy (designer and editor) differs from the
    /// active document as it was last stored or saved.
    pub(super) fn working_copy_changed(&self) -> bool {
        let mut copy = self.workspace.active_doc().clone();
        if copy.kind == DocKind::Form {
            copy.form = self.form.clone();
        }
        copy.lines = self.lines.clone();
        copy.is_dirty()
    }

    /// Whether anything is unsaved, counting the working copy.
    pub(super) fn has_unsaved(&mut self) -> bool {
        self.store_active();
        self.workspace.is_dirty()
    }

    /// Save the project file and every document. A project that has no
    /// file yet needs a path (see [`Ide::save_project_as`]).
    pub(super) fn save_project(&mut self) -> Result<(), String> {
        self.commit_pending_rename();
        self.store_active();
        let Some(file) = self.workspace.file.clone() else {
            return Err("the project has no file yet: use Save Project As".into());
        };
        self.workspace.save_all(&file)?;
        self.status = format!("Saved {}", file_name(&file));
        Ok(())
    }

    /// Save the project to `file`, naming its documents' files next to it.
    pub(super) fn save_project_to(&mut self, file: PathBuf) -> Result<(), String> {
        self.commit_pending_rename();
        self.store_active();
        let file = if file.extension().is_none() {
            file.with_extension(oxbasic_ui::project::PROJECT_EXT)
        } else {
            file
        };
        // a new project is named after its file
        if self.workspace.file.is_none() {
            if let Some(stem) = file.file_stem().and_then(|s| s.to_str()) {
                if oxbasic_ui::project::is_identifier(stem) {
                    self.workspace.name = stem.to_string();
                }
            }
        }
        // saved elsewhere: every document gets files in the new folder
        if self.workspace.file.as_ref() != Some(&file) {
            for d in &mut self.workspace.docs {
                d.layout = None;
                d.code = None;
            }
        }
        self.workspace.save_all(&file)?;
        self.status = format!("Saved {}", file_name(&file));
        // a saved project goes to the top of File > Open Recent; the caller
        // refreshes the menu, since this has no context to do it
        self.recent.record(&self.workspace.name.clone(), &file);
        self.store_recent();
        Ok(())
    }

    /// Save just the active document's code (the old `Save Code`).
    pub(super) fn save_active_code(&mut self) -> Result<(), String> {
        self.store_active();
        let i = self.workspace.active;
        let Some(file) = self.workspace.file.clone() else {
            return Err("the project has no file yet: use Save Project As".into());
        };
        let dir = file.parent().map(PathBuf::from).unwrap_or_default();
        self.workspace.ensure_files(i, &dir);
        self.workspace.save_doc_code(i)?;
        self.status = format!("Saved {}", self.workspace.active_doc().name);
        Ok(())
    }

    /// Replace the open project with `workspace` and show its first
    /// document. Stops a running program first.
    pub(super) fn install(&mut self, workspace: Workspace, cx: &mut Context<Self>) {
        self.stop_program(cx);
        self.hide_code_window(cx);
        self.workspace = workspace;
        self.load_active();
        self.output.clear();
        self.status = format!("opened {}", self.workspace.name);
    }

    /// Ask whether to save unsaved work before `then` runs. Save writes the
    /// project (a project with no file asks for one first); Don't Save runs
    /// `then` straight away; Cancel does nothing.
    pub(super) fn confirm_discard(
        &mut self,
        win: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Ide, &mut Window, &mut Context<Ide>) + 'static,
    ) {
        if !self.has_unsaved() {
            then(self, win, cx);
            return;
        }
        let name = self.workspace.name.clone();
        let rx = win.prompt(
            PromptLevel::Warning,
            &format!("Do you want to save the changes to {}?", name),
            Some("Your changes will be lost if you don't save them."),
            &["Save", "Don't Save", "Cancel"],
            cx,
        );
        cx.spawn_in(win, async move |this, cx| {
            let Ok(choice) = rx.await else { return };
            let _ = this.update_in(cx, move |ide, win, cx| match choice {
                0 if ide.workspace.file.is_some() => match ide.save_project() {
                    Ok(()) => then(ide, win, cx),
                    Err(e) => ide.status = format!("save failed: {}", e),
                },
                0 => ide.save_project_as_then(win, cx, then),
                1 => then(ide, win, cx),
                _ => {}
            });
        })
        .detach();
    }

    /// Ask for a project file, save to it, then run `then`.
    pub(super) fn save_project_as_then(
        &mut self,
        win: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Ide, &mut Window, &mut Context<Ide>) + 'static,
    ) {
        let suggested = format!("{}.oxproj", self.workspace.name);
        let dir = std::env::current_dir().unwrap_or_default();
        let rx = cx.prompt_for_new_path(&dir, Some(&suggested));
        cx.spawn_in(win, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let _ = this.update_in(cx, move |ide, win, cx| match ide.save_project_to(path) {
                Ok(()) => {
                    ide.refresh_file_menu(cx);
                    then(ide, win, cx)
                }
                Err(e) => ide.status = format!("save failed: {}", e),
            });
        })
        .detach();
    }
}

/// The File menu's commands.
impl Ide {
    /// File > New Project: a project with one unsaved form.
    pub(super) fn new_project(&mut self, win: &mut Window, cx: &mut Context<Self>) {
        self.confirm_discard(win, cx, |ide, _win, cx| {
            ide.install(Workspace::new_project(), cx);
            cx.notify();
        });
    }

    /// File > Close Project: the same prompt, then an empty project.
    pub(super) fn close_project(&mut self, win: &mut Window, cx: &mut Context<Self>) {
        self.new_project(win, cx);
    }

    /// Quit (⌘Q): with unsaved work, ask first; quit once the answer allows.
    pub(super) fn quit(&mut self, win: &mut Window, cx: &mut Context<Self>) {
        self.confirm_discard(win, cx, |ide, _win, cx| {
            ide.stop_program(cx);
            cx.quit();
        });
    }

    /// Whether the IDE's own window may close now. With unsaved work it
    /// asks, and closes the window itself once the answer allows; the
    /// close that triggered the question is refused for now.
    pub(super) fn may_close(&mut self, win: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.has_unsaved() {
            return true;
        }
        self.confirm_discard(win, cx, |ide, win, cx| {
            ide.stop_program(cx);
            // saved or discarded: close for real. Its project is clean now,
            // or its changes were given up, so mark it so the window agrees.
            ide.allow_close = true;
            win.remove_window();
        });
        false
    }

    /// File > Open Project…: pick an `.oxproj` and open it.
    pub(super) fn open_project(&mut self, win: &mut Window, cx: &mut Context<Self>) {
        self.confirm_discard(win, cx, |_ide, win, cx| {
            let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some("Open Project".into()),
            });
            cx.spawn_in(win, async move |this, cx| {
                let Ok(Ok(Some(paths))) = rx.await else {
                    return;
                };
                let Some(path) = paths.into_iter().next() else {
                    return;
                };
                let _ = this.update_in(cx, move |ide, _win, cx| {
                    ide.open_project_file(&path, cx);
                });
            })
            .detach();
        });
    }

    /// Open the project at `path`, replacing the current one.
    pub(super) fn open_project_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        match Workspace::open(path) {
            Ok(w) => {
                self.install(w, cx);
                // opened projects move to the top of File > Open Recent
                self.record_recent(path, cx);
            }
            Err(e) => self.status = format!("open failed: {}", e),
        }
        cx.notify();
    }

    /// File > Save Project As…
    pub(super) fn save_project_as(&mut self, win: &mut Window, cx: &mut Context<Self>) {
        self.save_project_as_then(win, cx, |ide, _win, cx| {
            cx.notify();
            let _ = ide;
        });
    }

    /// File > Add Form: a new form (unsaved until the project is saved).
    /// Project > Set as Start Form: the active form becomes the one the
    /// program starts with. Modules can't be the start.
    pub(super) fn set_start_form(&mut self, cx: &mut Context<Self>) {
        let doc = self.workspace.active_doc();
        let name = doc.name.clone();
        if doc.kind != DocKind::Form {
            self.status = format!("{} is a module; choose a form", name);
        } else if let Err(e) = self.workspace.set_start_form(&name) {
            self.status = e;
        } else {
            self.status = format!("{} starts the program", name);
        }
        cx.notify();
    }

    pub(super) fn add_form(&mut self, cx: &mut Context<Self>) {
        self.store_active();
        self.workspace.add_form();
        self.load_active();
        self.status = format!("added {}", self.workspace.active_doc().name);
        cx.notify();
    }

    /// File > Add Module: a new standard module.
    pub(super) fn add_module(&mut self, cx: &mut Context<Self>) {
        self.store_active();
        self.workspace.add_module();
        self.load_active();
        self.status = format!("added {}", self.workspace.active_doc().name);
        cx.notify();
    }

    /// File > Remove Form or Module: removes the active document after the
    /// user confirms. Its files are kept on disk.
    pub(super) fn remove_active(&mut self, win: &mut Window, cx: &mut Context<Self>) {
        self.store_active();
        let i = self.workspace.active;
        let name = self.workspace.docs[i].name.clone();
        let rx = win.prompt(
            PromptLevel::Warning,
            &format!("Remove {} from the project?", name),
            Some("Its files are kept on disk, but it is no longer part of the project."),
            &["Remove", "Cancel"],
            cx,
        );
        cx.spawn_in(win, async move |this, cx| {
            if !matches!(rx.await, Ok(0)) {
                return;
            }
            let _ = this.update_in(cx, move |ide, _win, cx| {
                if ide.workspace.remove(i) {
                    ide.load_active();
                    ide.status = format!("removed {}", name);
                } else {
                    ide.status = "a project keeps at least one form".into();
                }
                cx.notify();
            });
        })
        .detach();
    }
}

/// Renaming a form.
impl Ide {
    /// Rename the identifier `old` to `new` in every *other* document's
    /// code — `Form2.Show`, `Unload Form2` — and in the working copy. The
    /// renamed form's own code refers to itself as `Me` or by bare control
    /// names, so it needs no change. Returns how many identifiers changed.
    pub(super) fn rename_form_references(&mut self, old: &str, new: &str) -> usize {
        let active = self.workspace.active;
        let mut changed = 0;
        for (i, doc) in self.workspace.docs.iter_mut().enumerate() {
            if i == active {
                continue;
            }
            changed += code_model::rename_identifier(&mut doc.lines, old, new);
        }
        let renamed = code_model::rename_identifier(&mut self.lines, old, new);
        if renamed > 0 {
            // as for a control rename: the form changed too, so the code's
            // older undo steps no longer fit it
            self.anchor = None;
            self.history.clear();
        }
        changed += renamed;
        changed
    }
}
