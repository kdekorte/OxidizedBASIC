//! Where the recent projects are kept between runs: a small JSON file in the
//! user's application-support folder. A file that is missing or unreadable is
//! an empty list, and a write that fails is ignored, so the menu never stops
//! the IDE from starting or saving.

use super::recent::{RecentList, RecentProject};
use std::path::{Path, PathBuf};

/// The file the list is kept in: `~/Library/Application Support/OxidizedBASIC/recent.json`
/// on macOS, and the temporary folder when there is no home folder.
pub fn default_file() -> PathBuf {
    // a test run never touches the user's real list: its IDE windows record
    // into a folder of their own
    if cfg!(test) {
        return std::env::temp_dir()
            .join("oxide-test-recent")
            .join(format!("recent-{}.json", std::process::id()));
    }
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|h| h.join("Library").join("Application Support"))
        .unwrap_or_else(std::env::temp_dir);
    base.join("OxidizedBASIC").join("recent.json")
}

/// The list stored in `file`; empty when the file is missing or can't be read.
pub fn load(file: &Path) -> RecentList {
    let Ok(text) = std::fs::read_to_string(file) else {
        return RecentList::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return RecentList::default();
    };
    let entries = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| {
            Some(RecentProject {
                name: v.get("name")?.as_str()?.to_string(),
                path: PathBuf::from(v.get("path")?.as_str()?),
            })
        })
        .collect();
    RecentList::from_entries(entries)
}

/// Store `list` in `file`, creating its folder if needed. Returns whether it
/// was written; a failure is not an error the IDE needs to show.
pub fn save(file: &Path, list: &RecentList) -> bool {
    if let Some(dir) = file.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
    }
    let entries: Vec<serde_json::Value> = list
        .items()
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "path": p.path.to_string_lossy(),
            })
        })
        .collect();
    let Ok(text) = serde_json::to_string_pretty(&entries) else {
        return false;
    };
    std::fs::write(file, text).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("oxide-recent-{}-{}", tag, std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    #[test]
    fn a_saved_list_loads_back_in_the_same_order() {
        let d = dir("roundtrip");
        let file = d.join("recent.json");
        let mut l = RecentList::default();
        l.record("First", Path::new("/x/First.oxproj"));
        l.record("Second", Path::new("/x/Second.oxproj"));
        assert!(save(&file, &l));
        let back = load(&file);
        assert_eq!(back, l);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_missing_or_broken_file_is_an_empty_list() {
        let d = dir("broken");
        std::fs::create_dir_all(&d).unwrap();
        assert!(load(&d.join("none.json")).items().is_empty());
        std::fs::write(d.join("bad.json"), "{ not json").unwrap();
        assert!(load(&d.join("bad.json")).items().is_empty());
        std::fs::write(d.join("shape.json"), r#"[{"name": 3}, {"path": "/x"}]"#).unwrap();
        assert!(
            load(&d.join("shape.json")).items().is_empty(),
            "entries without both fields are skipped"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn saving_creates_the_folder() {
        let d = dir("folder");
        let file = d.join("nested").join("recent.json");
        assert!(save(&file, &RecentList::default()));
        assert!(file.exists());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_default_file_is_in_application_support() {
        // in a test run the default is a folder of its own, never the real list
        let f = default_file();
        assert!(
            !f.starts_with(
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_default()
                    .join("Library")
            ),
            "a test never uses the user's Library folder: {:?}",
            f
        );
        assert!(f.ends_with(format!("recent-{}.json", std::process::id())));
    }
}
