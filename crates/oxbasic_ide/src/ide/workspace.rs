//! The workspace: the open project's documents (forms and modules), which
//! one is being edited, and what has changed since it was last saved.
//!
//! This is the IDE's project model without any GUI, so opening, saving,
//! adding and removing documents are tested headless. [`super::Ide`] keeps
//! the active document's `form` and `lines` as its working copy and calls
//! [`Workspace::store`] / [`Workspace::load_active`] to swap them.

use oxbasic_ui::project::{FormEntry, ModuleEntry, Project, PROJECT_EXT};
use oxbasic_ui::FormSpec;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Whether a document is a form (layout and code) or a standard module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    Form,
    Module,
}

/// One form or module of the project.
#[derive(Debug, Clone)]
pub struct Doc {
    pub kind: DocKind,
    /// The BASIC name. A form's is its `form.name`.
    pub name: String,
    /// The `.oxform` layout (forms), `None` until first saved.
    pub layout: Option<PathBuf>,
    /// The `.bas` code, `None` until first saved.
    pub code: Option<PathBuf>,
    pub form: FormSpec,
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
    /// Breakpoints in this document's own line numbers (1-based).
    pub breakpoints: HashSet<usize>,
    saved_form: String,
    saved_code: String,
}

/// `text` as editor lines (an empty file is one empty line).
pub fn to_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

impl Doc {
    fn form_json(&self) -> String {
        match self.kind {
            DocKind::Form => self.form.to_oxform(),
            DocKind::Module => String::new(),
        }
    }

    /// A new, unsaved form with an empty code module.
    pub fn new_form(name: &str) -> Doc {
        let mut d = Doc {
            kind: DocKind::Form,
            name: name.to_string(),
            layout: None,
            code: None,
            form: FormSpec::new(name, name, 340.0, 300.0),
            lines: vec![String::new()],
            cursor: (0, 0),
            breakpoints: HashSet::new(),
            saved_form: String::new(),
            saved_code: String::new(),
        };
        // a new document counts as unsaved until it is written
        d.saved_form = "\u{0}new".into();
        d
    }

    /// A new, unsaved standard module.
    pub fn new_module(name: &str) -> Doc {
        Doc {
            kind: DocKind::Module,
            name: name.to_string(),
            layout: None,
            code: None,
            form: FormSpec::new(name, name, 0.0, 0.0),
            lines: vec![String::new()],
            cursor: (0, 0),
            breakpoints: HashSet::new(),
            saved_form: String::new(),
            saved_code: "\u{0}new".into(),
        }
    }

    /// A document read from disk, or built from the given parts, already
    /// saved.
    pub fn saved(
        kind: DocKind,
        name: &str,
        layout: Option<PathBuf>,
        code: Option<PathBuf>,
        form: FormSpec,
        lines: Vec<String>,
    ) -> Doc {
        let mut d = Doc {
            kind,
            name: name.to_string(),
            layout,
            code,
            form,
            lines,
            cursor: (0, 0),
            breakpoints: HashSet::new(),
            saved_form: String::new(),
            saved_code: String::new(),
        };
        d.mark_saved();
        d
    }

    /// The code as one string, one `\n` per line.
    pub fn source(&self) -> String {
        self.lines.join("\n")
    }

    /// Whether the layout or the code differs from what was last saved.
    pub fn is_dirty(&self) -> bool {
        self.form_json() != self.saved_form || self.source() != self.saved_code
    }

    pub fn mark_saved(&mut self) {
        self.saved_form = self.form_json();
        self.saved_code = self.source();
    }

    /// Forget that the document was saved (its file doesn't exist yet).
    pub fn mark_unsaved(&mut self) {
        self.saved_form = "\u{0}unsaved".into();
        self.saved_code = "\u{0}unsaved".into();
    }

    /// The code was written, the layout wasn't.
    pub fn mark_code_saved(&mut self) {
        self.saved_code = self.source();
    }

    /// The layout was written, the code wasn't.
    pub fn mark_form_saved(&mut self) {
        self.saved_form = self.form_json();
    }

    /// Label for the Project panel: `Form1 (Form1.oxform)`, with `*` when
    /// unsaved.
    pub fn file_label(&self) -> String {
        let file = match self.kind {
            DocKind::Form => self.layout.as_ref(),
            DocKind::Module => self.code.as_ref(),
        }
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| match self.kind {
            DocKind::Form => format!("{}.oxform", self.name),
            DocKind::Module => format!("{}.bas", self.name),
        });
        format!("{}{}", file, if self.is_dirty() { " *" } else { "" })
    }
}

/// The open project: its documents, the active one, and where the project
/// file is.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// The project file, `None` for a project not yet saved.
    pub file: Option<PathBuf>,
    pub name: String,
    /// The startup form's name.
    pub startup: String,
    pub docs: Vec<Doc>,
    /// Index into `docs` of the document being edited.
    pub active: usize,
    /// The app's version, and its icon and bundle id, as the project file
    /// gives them (see `oxbasic_ui::project::Project`).
    pub version: String,
    pub icon: String,
    pub bundle_id: String,
}

impl Workspace {
    /// A new project with one form, `Form1`, and no file yet.
    pub fn new_project() -> Workspace {
        Workspace {
            file: None,
            name: "Project1".into(),
            startup: "Form1".into(),
            docs: vec![Doc::new_form("Form1")],
            active: 0,
            version: oxbasic_ui::project::default_version(),
            icon: String::new(),
            bundle_id: String::new(),
        }
    }

    pub fn active_doc(&self) -> &Doc {
        &self.docs[self.active]
    }

    pub fn active_doc_mut(&mut self) -> &mut Doc {
        &mut self.docs[self.active]
    }

    /// Whether any document has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.docs.iter().any(|d| d.is_dirty())
    }

    /// The index of the document named `name` (case-insensitive).
    pub fn find(&self, name: &str) -> Option<usize> {
        self.docs
            .iter()
            .position(|d| d.name.eq_ignore_ascii_case(name))
    }

    /// Make `index` the active document.
    pub fn activate(&mut self, index: usize) {
        if index < self.docs.len() {
            self.active = index;
        }
    }

    /// Add a form, named `Form<n>` with the next free number, and make it
    /// active. Returns its index.
    pub fn add_form(&mut self) -> usize {
        let name = self.free_name("Form");
        self.docs.push(Doc::new_form(&name));
        self.active = self.docs.len() - 1;
        self.active
    }

    /// Add a standard module, named `Module<n>`, and make it active.
    pub fn add_module(&mut self) -> usize {
        let name = self.free_name("Module");
        self.docs.push(Doc::new_module(&name));
        self.active = self.docs.len() - 1;
        self.active
    }

    /// The first `<prefix><n>` no document uses.
    pub fn free_name(&self, prefix: &str) -> String {
        (1..)
            .map(|n| format!("{}{}", prefix, n))
            .find(|n| self.find(n).is_none())
            .unwrap()
    }

    /// Remove document `index`. The startup form can't be removed while it
    /// is the only form. Returns false when refused.
    pub fn remove(&mut self, index: usize) -> bool {
        let Some(doc) = self.docs.get(index) else {
            return false;
        };
        let forms = self.docs.iter().filter(|d| d.kind == DocKind::Form).count();
        if doc.kind == DocKind::Form && forms == 1 {
            return false;
        }
        if doc.kind == DocKind::Form && doc.name.eq_ignore_ascii_case(&self.startup) {
            // the startup object moves to the next form
            let next = self
                .docs
                .iter()
                .enumerate()
                .find(|(i, d)| *i != index && d.kind == DocKind::Form)
                .map(|(_, d)| d.name.clone());
            self.startup = next.unwrap_or_default();
        }
        self.docs.remove(index);
        if self.active >= self.docs.len() || self.active > index {
            self.active = self.active.saturating_sub(1);
        }
        true
    }

    /// Rename document `index`. The name must be a free identifier; a
    /// form's rename also moves the startup object along with it.
    pub fn rename(&mut self, index: usize, new: &str) -> Result<(), String> {
        if !oxbasic_ui::project::is_identifier(new) {
            return Err(format!("\"{}\" is not a valid BASIC name", new));
        }
        if self
            .docs
            .iter()
            .enumerate()
            .any(|(i, d)| i != index && d.name.eq_ignore_ascii_case(new))
        {
            return Err(format!("\"{}\" is already used in the project", new));
        }
        let old = self.docs[index].name.clone();
        if self.docs[index].kind == DocKind::Form {
            if self.startup.eq_ignore_ascii_case(&old) {
                self.startup = new.to_string();
            }
            self.docs[index].form.name = new.to_string();
        }
        self.docs[index].name = new.to_string();
        Ok(())
    }
}

impl Workspace {
    /// The project file's folder, or the current folder for an unsaved one.
    fn dir(&self) -> PathBuf {
        self.file
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Write every document whose files are known (`Save Project`). A new
    /// project is written next to `file`, with its documents' files named
    /// after them.
    pub fn save_all(&mut self, file: &Path) -> Result<(), String> {
        self.file = Some(file.to_path_buf());
        let dir = self.dir();
        for i in 0..self.docs.len() {
            self.assign_files(i, &dir);
            self.save_doc(i)?;
        }
        self.to_project().save(file)?;
        Ok(())
    }

    /// Give document `i` its file names in `dir` if it has none yet.
    fn assign_files(&mut self, i: usize, dir: &Path) {
        self.ensure_files(i, dir);
    }

    /// Give document `i` its file names in `dir` if it has none yet.
    pub fn ensure_files(&mut self, i: usize, dir: &Path) {
        let d = &mut self.docs[i];
        if d.kind == DocKind::Form && d.layout.is_none() {
            d.layout = Some(dir.join(format!("{}.oxform", d.name)));
        }
        if d.code.is_none() {
            d.code = Some(dir.join(format!("{}.bas", d.name)));
        }
    }

    /// Write only document `i`'s code, leaving its layout as it is.
    pub fn save_doc_code(&mut self, i: usize) -> Result<(), String> {
        let d = &mut self.docs[i];
        let code = d.code.clone().ok_or("the module has no file")?;
        std::fs::write(&code, d.source() + "\n")
            .map_err(|e| format!("cannot save {}: {}", code.display(), e))?;
        d.mark_code_saved();
        Ok(())
    }

    /// Write document `i` to its files and mark it saved.
    pub fn save_doc(&mut self, i: usize) -> Result<(), String> {
        let d = &mut self.docs[i];
        if d.kind == DocKind::Form {
            let layout = d.layout.clone().ok_or("the form has no file")?;
            std::fs::write(&layout, d.form.to_oxform())
                .map_err(|e| format!("cannot save {}: {}", layout.display(), e))?;
        }
        let code = d.code.clone().ok_or("the module has no file")?;
        std::fs::write(&code, d.source() + "\n")
            .map_err(|e| format!("cannot save {}: {}", code.display(), e))?;
        d.mark_saved();
        Ok(())
    }

    /// The project as its `.oxproj` file describes it, with paths relative
    /// to the project file.
    pub fn to_project(&self) -> Project {
        let rel = |p: &Option<PathBuf>, fallback: String| -> String {
            p.as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or(fallback)
        };
        let mut project = Project::new(&self.name);
        project.startup = self.startup.clone();
        project.version = self.version.clone();
        project.icon = self.icon.clone();
        project.bundle_id = self.bundle_id.clone();
        for d in &self.docs {
            match d.kind {
                DocKind::Form => project.forms.push(FormEntry {
                    name: d.name.clone(),
                    layout: rel(&d.layout, format!("{}.oxform", d.name)),
                    code: rel(&d.code, format!("{}.bas", d.name)),
                }),
                DocKind::Module => project.modules.push(ModuleEntry {
                    name: d.name.clone(),
                    code: rel(&d.code, format!("{}.bas", d.name)),
                }),
            }
        }
        project
    }

    /// Open a project file: check it against the disk, then read every
    /// form and module. Nothing is changed if any file is missing.
    pub fn open(file: &Path) -> Result<Workspace, String> {
        let project = Project::load(file)?;
        let dir = file.parent().unwrap_or(Path::new(".")).to_path_buf();
        project.validate(&dir).map_err(|e| e.to_string())?;
        let mut docs = Vec::new();
        for f in &project.forms {
            let layout = Project::resolve(file, &f.layout);
            let json = std::fs::read_to_string(&layout)
                .map_err(|e| format!("cannot read {}: {}", layout.display(), e))?;
            let form = FormSpec::from_oxform(&json)?;
            let code = Project::resolve(file, &f.code);
            let text = std::fs::read_to_string(&code)
                .map_err(|e| format!("cannot read {}: {}", code.display(), e))?;
            docs.push(Doc::saved(
                DocKind::Form,
                &f.name,
                Some(layout),
                Some(code),
                form,
                to_lines(&text),
            ));
        }
        for m in &project.modules {
            let code = Project::resolve(file, &m.code);
            let text = std::fs::read_to_string(&code)
                .map_err(|e| format!("cannot read {}: {}", code.display(), e))?;
            docs.push(Doc::saved(
                DocKind::Module,
                &m.name,
                None,
                Some(code),
                FormSpec::new(&m.name, &m.name, 0.0, 0.0),
                to_lines(&text),
            ));
        }
        if docs.is_empty() {
            return Err("the project has no forms".into());
        }
        let startup = project
            .startup_form()
            .map(|f| f.name.clone())
            .unwrap_or_default();
        Ok(Workspace {
            file: Some(file.to_path_buf()),
            name: project.name,
            startup,
            docs,
            active: 0,
            version: project.version,
            icon: project.icon,
            bundle_id: project.bundle_id,
        })
    }

    /// Whether the project file has a `.oxproj` extension (`Open` accepts
    /// it; the IDE's `.bas`/`.oxform` arguments go another way).
    pub fn is_project_file(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some(PROJECT_EXT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder for one test, removed when `Drop`ped.
    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Dir {
            let p = std::env::temp_dir().join(format!("oxws-{}-{}", tag, std::process::id()));
            std::fs::remove_dir_all(&p).ok();
            std::fs::create_dir_all(&p).unwrap();
            Dir(p)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    #[test]
    fn a_new_project_has_one_unsaved_form() {
        let w = Workspace::new_project();
        assert_eq!(w.docs.len(), 1);
        assert_eq!(w.startup, "Form1");
        assert!(w.is_dirty(), "a new form is unsaved");
        assert_eq!(w.active_doc().file_label(), "Form1.oxform *");
    }

    #[test]
    fn add_names_documents_with_free_numbers() {
        let mut w = Workspace::new_project();
        assert_eq!(w.add_form(), 1);
        assert_eq!(w.docs[1].name, "Form2");
        assert_eq!(w.add_module(), 2);
        assert_eq!(w.docs[2].name, "Module1");
        // removing Form2 frees its number for the next form (appended last)
        assert!(w.remove(1));
        let i = w.add_form();
        assert_eq!(w.docs[i].name, "Form2");
    }

    #[test]
    fn the_last_form_and_a_missing_index_cannot_be_removed() {
        let mut w = Workspace::new_project();
        assert!(!w.remove(0), "a project keeps at least one form");
        assert!(!w.remove(9));
        w.add_module();
        assert!(!w.remove(0), "modules don't count as forms");
    }

    #[test]
    fn removing_the_startup_form_moves_startup_to_another_form() {
        let mut w = Workspace::new_project();
        w.add_form();
        w.startup = "Form2".into();
        assert!(w.remove(1));
        assert_eq!(w.startup, "Form1");
    }

    #[test]
    fn rename_follows_the_startup_form_and_refuses_clashes() {
        let mut w = Workspace::new_project();
        w.add_form();
        w.add_module();
        w.rename(0, "Main").unwrap();
        assert_eq!(w.startup, "Main");
        assert_eq!(w.docs[0].form.name, "Main", "the layout's name follows");
        assert!(w.rename(1, "main").is_err());
        assert!(w.rename(1, "1bad").is_err());
        assert!(
            w.rename(2, "Form1").is_ok(),
            "a module may take a freed name"
        );
    }

    #[test]
    fn save_then_open_round_trips_forms_modules_and_code() {
        let dir = Dir::new("roundtrip");
        let file = dir.0.join("app.oxproj");
        let mut w = Workspace::new_project();
        w.add_form();
        w.add_module();
        w.docs[2].lines = to_lines("Sub Helper()\n  Print \"hi\"\nEnd Sub");
        w.docs[0].form.caption = "Main window".into();
        w.save_all(&file).unwrap();
        assert!(!w.is_dirty(), "everything is saved");
        assert!(file.is_file());
        assert!(dir.0.join("Form1.oxform").is_file());
        assert!(dir.0.join("Module1.bas").is_file());

        let back = Workspace::open(&file).unwrap();
        assert_eq!(back.docs.len(), 3);
        assert_eq!(back.docs[0].form.caption, "Main window");
        assert_eq!(back.docs[2].kind, DocKind::Module);
        assert_eq!(back.docs[2].lines[0], "Sub Helper()");
        assert!(!back.is_dirty());
    }

    #[test]
    fn edits_mark_the_document_dirty_until_saved() {
        let dir = Dir::new("dirty");
        let file = dir.0.join("app.oxproj");
        let mut w = Workspace::new_project();
        w.save_all(&file).unwrap();
        assert!(!w.is_dirty());

        // a code edit dirties the document; saving only the code cleans it
        w.docs[0].lines[0] = "Print 1".into();
        assert!(w.docs[0].is_dirty());
        assert!(w.docs[0].file_label().ends_with('*'));
        w.save_doc_code(0).unwrap();
        assert!(!w.is_dirty(), "the layout was never touched");

        // a layout edit dirties it too, and saving the code alone doesn't help
        w.docs[0].form.caption = "Changed".into();
        assert!(w.is_dirty());
        w.save_doc_code(0).unwrap();
        assert!(w.is_dirty(), "the layout is still unsaved");
        w.save_doc(0).unwrap();
        assert!(!w.is_dirty());
    }

    #[test]
    fn opening_refuses_a_project_with_a_missing_file() {
        let dir = Dir::new("missing");
        let file = dir.0.join("app.oxproj");
        let mut w = Workspace::new_project();
        w.save_all(&file).unwrap();
        std::fs::remove_file(dir.0.join("Form1.bas")).unwrap();
        let err = Workspace::open(&file).unwrap_err();
        assert!(err.contains("Form1.bas"), "{}", err);
    }
}
