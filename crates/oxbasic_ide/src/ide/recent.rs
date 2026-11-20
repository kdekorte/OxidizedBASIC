//! The File menu's recent projects: the last few projects opened or saved,
//! most recent first, each with its name and full path. This module is the
//! list and its rules (add, move to the top, keep the last few); reading and
//! writing the file that keeps it is in `recent_store.rs`. No GUI here, so
//! it is tested headless.

use std::path::{Path, PathBuf};

/// How many projects the list keeps.
pub const MAX_RECENT: usize = 5;

/// One project in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentProject {
    /// The project's name, as the Project panel shows it.
    pub name: String,
    /// The full path of its `.oxproj` file.
    pub path: PathBuf,
}

/// The recent projects, most recent first, at most [`MAX_RECENT`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecentList {
    items: Vec<RecentProject>,
}

impl RecentList {
    /// The list, most recent first.
    pub fn items(&self) -> &[RecentProject] {
        &self.items
    }

    /// Record that the project `name` at `path` was opened or saved: it goes
    /// to the top, replacing an earlier entry for the same file, and the
    /// list is cut back to [`MAX_RECENT`].
    pub fn record(&mut self, name: &str, path: &Path) {
        let path = normalise(path);
        self.items.retain(|p| p.path != path);
        self.items.insert(
            0,
            RecentProject {
                name: name.to_string(),
                path,
            },
        );
        self.items.truncate(MAX_RECENT);
    }

    /// Remove the entry for `path` (a project that no longer exists, say).
    pub fn forget(&mut self, path: &Path) {
        let path = normalise(path);
        self.items.retain(|p| p.path != path);
    }

    /// Empty the list (File > Open Recent > Clear Menu).
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Build a list from stored entries, keeping their order and dropping
    /// duplicates and anything beyond [`MAX_RECENT`].
    pub fn from_entries(entries: Vec<RecentProject>) -> RecentList {
        let mut list = RecentList::default();
        for e in entries.into_iter().rev() {
            list.record(&e.name, &e.path);
        }
        list
    }
}

/// The path as the list compares it: the same file, however it was named,
/// is the same entry.
fn normalise(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn the_last_opened_project_is_first() {
        let mut l = RecentList::default();
        l.record("A", &p("/x/A.oxproj"));
        l.record("B", &p("/x/B.oxproj"));
        let names: Vec<&str> = l.items().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["B", "A"]);
    }

    #[test]
    fn reopening_moves_a_project_to_the_top_without_a_duplicate() {
        let mut l = RecentList::default();
        l.record("A", &p("/x/A.oxproj"));
        l.record("B", &p("/x/B.oxproj"));
        l.record("A", &p("/x/A.oxproj"));
        let names: Vec<&str> = l.items().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["A", "B"]);
    }

    #[test]
    fn only_the_last_five_are_kept() {
        let mut l = RecentList::default();
        for i in 0..7 {
            l.record(&format!("P{}", i), &p(&format!("/x/P{}.oxproj", i)));
        }
        assert_eq!(l.items().len(), MAX_RECENT);
        assert_eq!(l.items()[0].name, "P6", "the newest is first");
        assert_eq!(l.items()[4].name, "P2", "the oldest kept is fifth");
    }

    #[test]
    fn forget_and_clear_remove_entries() {
        let mut l = RecentList::default();
        l.record("A", &p("/x/A.oxproj"));
        l.record("B", &p("/x/B.oxproj"));
        l.forget(&p("/x/A.oxproj"));
        assert_eq!(l.items().len(), 1);
        l.clear();
        assert!(l.items().is_empty());
    }

    #[test]
    fn stored_entries_load_in_their_order_and_are_trimmed() {
        let entries: Vec<RecentProject> = (0..8)
            .map(|i| RecentProject {
                name: format!("P{}", i),
                path: p(&format!("/x/P{}.oxproj", i)),
            })
            .collect();
        let l = RecentList::from_entries(entries);
        assert_eq!(l.items().len(), MAX_RECENT);
        assert_eq!(l.items()[0].name, "P0", "the stored order is kept");
    }
}
