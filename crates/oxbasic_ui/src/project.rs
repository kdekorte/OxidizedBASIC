//! The project: what an `.oxproj` file names — its forms (each a
//! `.oxform` layout with a `.bas` code module) and its standard modules —
//! and the rules that keep it consistent.
//!
//! Paths are stored relative to the project file, so a project folder can
//! be moved or copied as a whole. The model is pure data with no GUI, so it
//! is tested headless; the IDE holds a [`Project`] and does the file I/O
//! through [`Project::load`] and [`Project::save`].

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The file extension of a project file.
pub const PROJECT_EXT: &str = "oxproj";

/// A form and its files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormEntry {
    /// The form's BASIC name (`Form1`).
    pub name: String,
    /// The `.oxform` layout, relative to the project file.
    pub layout: String,
    /// The form's code module, relative to the project file.
    pub code: String,
}

/// A standard module (code that isn't a form's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleEntry {
    /// The module's BASIC name (`Module1`).
    pub name: String,
    /// The `.bas` file, relative to the project file.
    pub code: String,
}

/// An `.oxproj` project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    /// The form the program starts with (Start Up Object). Empty means the
    /// first form.
    #[serde(default)]
    pub startup: String,
    #[serde(default)]
    pub forms: Vec<FormEntry>,
    #[serde(default)]
    pub modules: Vec<ModuleEntry>,
}

/// Why a project can't be used as it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    /// Two forms or modules share a name (case-insensitively).
    DuplicateName(String),
    /// A name isn't a BASIC identifier.
    BadName(String),
    /// A form the operation names isn't in the project.
    NoSuchForm(String),
    /// A file a form or module needs isn't there.
    MissingFile(String),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectError::DuplicateName(n) => {
                write!(f, "\"{}\" is already used in the project", n)
            }
            ProjectError::BadName(n) => write!(f, "\"{}\" is not a valid BASIC name", n),
            ProjectError::NoSuchForm(n) => write!(f, "\"{}\" is not a form of this project", n),
            ProjectError::MissingFile(p) => write!(f, "file not found: {}", p),
        }
    }
}

/// Whether `name` is a BASIC identifier (letters, digits, `_`; starts with
/// a letter; at most 40 characters).
pub fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= 40
}

impl Project {
    /// An empty project named `name`.
    pub fn new(name: &str) -> Project {
        Project {
            name: name.to_string(),
            startup: String::new(),
            forms: Vec::new(),
            modules: Vec::new(),
        }
    }

    /// The form the program starts with: `startup` if set and valid, else
    /// the first form.
    pub fn startup_form(&self) -> Option<&FormEntry> {
        self.forms
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(&self.startup))
            .or_else(|| self.forms.first())
    }

    /// Whether `name` is already a form or module, other than the entry
    /// `except` (`(is_form, index)`).
    fn name_taken(&self, name: &str, except: Option<(bool, usize)>) -> bool {
        let form = self
            .forms
            .iter()
            .enumerate()
            .any(|(i, f)| f.name.eq_ignore_ascii_case(name) && except != Some((true, i)));
        let module = self
            .modules
            .iter()
            .enumerate()
            .any(|(i, m)| m.name.eq_ignore_ascii_case(name) && except != Some((false, i)));
        form || module
    }

    fn check_name(&self, name: &str, except: Option<(bool, usize)>) -> Result<(), ProjectError> {
        if !is_identifier(name) {
            return Err(ProjectError::BadName(name.to_string()));
        }
        if self.name_taken(name, except) {
            return Err(ProjectError::DuplicateName(name.to_string()));
        }
        Ok(())
    }

    /// Add a form. Its name must be unique and a valid identifier.
    pub fn add_form(&mut self, entry: FormEntry) -> Result<(), ProjectError> {
        self.check_name(&entry.name, None)?;
        self.forms.push(entry);
        Ok(())
    }

    /// Add a standard module. Its name must be unique and a valid identifier.
    pub fn add_module(&mut self, entry: ModuleEntry) -> Result<(), ProjectError> {
        self.check_name(&entry.name, None)?;
        self.modules.push(entry);
        Ok(())
    }

    /// Remove the form `name`. If it was the startup object, the startup
    /// falls back to the first form.
    pub fn remove_form(&mut self, name: &str) -> bool {
        let before = self.forms.len();
        self.forms.retain(|f| !f.name.eq_ignore_ascii_case(name));
        if self.startup.eq_ignore_ascii_case(name) {
            self.startup.clear();
        }
        self.forms.len() != before
    }

    /// Remove the module `name`.
    pub fn remove_module(&mut self, name: &str) -> bool {
        let before = self.modules.len();
        self.modules.retain(|m| !m.name.eq_ignore_ascii_case(name));
        self.modules.len() != before
    }

    /// Rename a form, checking the new name like [`Project::add_form`].
    pub fn rename_form(&mut self, old: &str, new: &str) -> Result<(), ProjectError> {
        let Some(i) = self
            .forms
            .iter()
            .position(|f| f.name.eq_ignore_ascii_case(old))
        else {
            return Err(ProjectError::NoSuchForm(old.to_string()));
        };
        self.check_name(new, Some((true, i)))?;
        if self.startup.eq_ignore_ascii_case(old) {
            self.startup = new.to_string();
        }
        self.forms[i].name = new.to_string();
        Ok(())
    }

    /// Choose the startup object. It must be a form of the project.
    pub fn set_startup(&mut self, name: &str) -> Result<(), ProjectError> {
        if !self.forms.iter().any(|f| f.name.eq_ignore_ascii_case(name)) {
            return Err(ProjectError::NoSuchForm(name.to_string()));
        }
        self.startup = name.to_string();
        Ok(())
    }
}

impl Project {
    /// Every file the project refers to, relative to the project file.
    pub fn files(&self) -> Vec<String> {
        let mut files = Vec::new();
        for f in &self.forms {
            files.push(f.layout.clone());
            files.push(f.code.clone());
        }
        for m in &self.modules {
            files.push(m.code.clone());
        }
        files
    }

    /// Check the project against the disk: names are valid and unique, the
    /// startup object exists, and every file is there. `dir` is the folder
    /// the project file is in.
    pub fn validate(&self, dir: &Path) -> Result<(), ProjectError> {
        let names: Vec<String> = self
            .forms
            .iter()
            .map(|f| f.name.clone())
            .chain(self.modules.iter().map(|m| m.name.clone()))
            .collect();
        for (i, n) in names.iter().enumerate() {
            if !is_identifier(n) {
                return Err(ProjectError::BadName(n.clone()));
            }
            if names[..i].iter().any(|s| s.eq_ignore_ascii_case(n)) {
                return Err(ProjectError::DuplicateName(n.clone()));
            }
        }
        if !self.startup.is_empty()
            && !self
                .forms
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(&self.startup))
        {
            return Err(ProjectError::NoSuchForm(self.startup.clone()));
        }
        for file in self.files() {
            if !dir.join(&file).is_file() {
                return Err(ProjectError::MissingFile(file));
            }
        }
        Ok(())
    }

    /// Read a project file.
    pub fn load(path: &Path) -> Result<Project, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
        serde_json::from_str(&text)
            .map_err(|e| format!("invalid project {}: {}", path.display(), e))
    }

    /// Write the project file (pretty JSON).
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json + "\n")
            .map_err(|e| format!("cannot write {}: {}", path.display(), e))
    }

    /// The path of `file` (relative to the project) given the project file.
    pub fn resolve(project_file: &Path, file: &str) -> PathBuf {
        project_file.parent().unwrap_or(Path::new(".")).join(file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn form(name: &str) -> FormEntry {
        FormEntry {
            name: name.into(),
            layout: format!("{}.oxform", name),
            code: format!("{}.bas", name),
        }
    }

    #[test]
    fn names_are_unique_across_forms_and_modules() {
        let mut p = Project::new("App");
        p.add_form(form("Form1")).unwrap();
        assert_eq!(
            p.add_form(form("form1")),
            Err(ProjectError::DuplicateName("form1".into()))
        );
        let clash = ModuleEntry {
            name: "Form1".into(),
            code: "m.bas".into(),
        };
        assert!(
            p.add_module(clash).is_err(),
            "a module can't share a form's name"
        );
        p.add_module(ModuleEntry {
            name: "Helpers".into(),
            code: "Helpers.bas".into(),
        })
        .unwrap();
    }

    #[test]
    fn names_must_be_identifiers() {
        let mut p = Project::new("App");
        for bad in ["", "1Form", "My Form", "x-y"] {
            assert!(p.add_form(form(bad)).is_err(), "{:?}", bad);
        }
        assert!(is_identifier("Form_2"));
        assert!(!is_identifier(&"a".repeat(41)));
    }

    #[test]
    fn rename_keeps_startup_and_refuses_clashes() {
        let mut p = Project::new("App");
        p.add_form(form("Form1")).unwrap();
        p.add_form(form("Form2")).unwrap();
        p.set_startup("Form2").unwrap();
        p.rename_form("Form2", "Main").unwrap();
        assert_eq!(p.startup, "Main");
        assert!(p.rename_form("Main", "Form1").is_err());
        assert_eq!(p.forms[1].name, "Main");
    }

    #[test]
    fn removing_the_startup_form_falls_back_to_the_first() {
        let mut p = Project::new("App");
        p.add_form(form("Form1")).unwrap();
        p.add_form(form("Form2")).unwrap();
        p.set_startup("Form2").unwrap();
        assert!(p.remove_form("form2"));
        assert_eq!(p.startup_form().unwrap().name, "Form1");
        assert!(!p.remove_form("Nope"));
    }

    #[test]
    fn startup_must_be_a_form() {
        let mut p = Project::new("App");
        p.add_form(form("Form1")).unwrap();
        assert_eq!(
            p.set_startup("Missing"),
            Err(ProjectError::NoSuchForm("Missing".into()))
        );
        assert_eq!(p.startup_form().unwrap().name, "Form1");
    }

    #[test]
    fn json_round_trip_and_validation_against_the_disk() {
        let dir = std::env::temp_dir().join(format!("oxproj-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = Project::new("Demo");
        p.add_form(form("Form1")).unwrap();
        p.add_module(ModuleEntry {
            name: "Helpers".into(),
            code: "Helpers.bas".into(),
        })
        .unwrap();
        let file = dir.join("demo.oxproj");
        p.save(&file).unwrap();
        assert_eq!(Project::load(&file).unwrap(), p);

        assert_eq!(
            p.validate(&dir),
            Err(ProjectError::MissingFile("Form1.oxform".into()))
        );
        for f in p.files() {
            std::fs::write(dir.join(&f), "").unwrap();
        }
        p.validate(&dir).unwrap();
        assert_eq!(Project::resolve(&file, "Form1.bas"), dir.join("Form1.bas"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
