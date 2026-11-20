//! The Menu Editor: a window that edits a form's menu items (File > Edit >
//! Help...). Its rows show each item at its depth; New, Delete, Indent and
//! Outdent act on the selected row; the items are written back to the form
//! when the window closes, so they save with the project. The rules are in
//! `oxbasic_ui::menu_editor` and are tested there.

use oxbasic_ui::menu_editor::MenuEditor;

/// The menu editor window for form `form` of the designer.
pub(super) struct MenuEditorWindow {
    pub form: usize,
    pub editor: MenuEditor,
    /// The caption typed for New (or for the selected item's rename).
    pub caption: String,
}

impl MenuEditorWindow {
    pub(super) fn new(form: usize, items: Vec<oxbasic_ui::menu::MenuItem>) -> Self {
        MenuEditorWindow {
            form,
            editor: MenuEditor::new(items),
            caption: "New item".into(),
        }
    }

    /// The text of each row: its caption, indented by its depth, with a
    /// separator shown as a line.
    pub(super) fn row_labels(&self) -> Vec<String> {
        self.editor
            .rows()
            .into_iter()
            .map(|(i, depth)| {
                let m = &self.editor.items[i];
                let label = if m.is_separator() {
                    "────".to_string()
                } else {
                    m.caption.replace('&', "")
                };
                format!("{}{}", "    ".repeat(depth), label)
            })
            .collect()
    }
}
