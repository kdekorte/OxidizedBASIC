//! The Menu Editor window, opened from the designer's Properties for a form. It
//! edits a copy of the form's menu; Done (or closing the window) writes the
//! copy back into the designer's working form, which saves with the project.

use super::text_field;
use super::*;
use oxbasic_ui::menu::{display_caption, MenuItem};
use oxbasic_ui::menu_editor::MenuEditor;

/// The Menu Editor's view: the items being edited and the Caption box.
pub struct MenuEditorView {
    pub editor: MenuEditor,
    /// The Caption box's text, and its caret (in characters).
    pub caption: String,
    pub caret: usize,
    /// The keyboard focus of the window's content (the Caption box types here).
    pub focus: gpui::FocusHandle,
    /// The shortcut's modifier toggles and its key (empty: no shortcut).
    pub mods: oxbasic_ui::menu::Modifiers,
    pub key: String,
    /// True while the key box has the keyboard (set by clicking it), so typing
    /// goes to the key rather than the caption.
    pub key_focus: bool,
}

impl MenuEditorView {
    pub fn new(items: Vec<MenuItem>, cx: &mut Context<Self>) -> Self {
        MenuEditorView {
            editor: MenuEditor::new(items),
            caption: "New item".into(),
            caret: "New item".chars().count(),
            focus: cx.focus_handle(),
            mods: Default::default(),
            key: String::new(),
            key_focus: false,
        }
    }

    /// The shortcut the toggles and key make now (`""` when the key is empty).
    pub fn shortcut(&self) -> String {
        oxbasic_ui::menu::shortcut_from(self.mods, &self.key)
    }

    /// Flip one modifier toggle: 0 Ctrl, 1 Cmd, 2 Alt, 3 Shift.
    pub fn toggle(&mut self, which: usize) {
        match which {
            0 => self.mods.ctrl = !self.mods.ctrl,
            1 => self.mods.cmd = !self.mods.cmd,
            2 => self.mods.alt = !self.mods.alt,
            3 => self.mods.shift = !self.mods.shift,
            _ => {}
        }
    }

    /// Type a letter or digit into the key box (one key only).
    /// Type a letter, digit, or (with the key box focused) a function key name
    /// into the key box. A whole word such as `F5` is taken from the typed text
    /// when it is an F1–F12 key; otherwise one letter or digit is taken.
    pub fn type_key(&mut self, s: &str) {
        let s = s.trim();
        if let Some(key) = oxbasic_ui::menu::key_from_text(s) {
            self.key = key;
        }
    }

    /// The row's text: its caption at its depth, with `&` mnemonics hidden and
    /// a separator drawn as a rule.
    pub fn row_label(item: &MenuItem, depth: usize) -> String {
        let label = if item.is_separator() {
            "────".to_string()
        } else {
            display_caption(&item.caption)
        };
        format!("{}{}", "    ".repeat(depth), label)
    }

    /// The Caption box typed into: a character goes in at the caret.
    pub fn type_caption(&mut self, s: &str) {
        self.caret = text_field::insert(&mut self.caption, self.caret, s);
    }

    /// Backspace in the Caption box.
    pub fn backspace_caption(&mut self) {
        self.caret = text_field::backspace(&mut self.caption, self.caret);
    }

    /// "New": add a menu item with the Caption box's text, after the selection
    /// under its parent (or on the bar), and select it.
    pub fn new_item(&mut self) -> Option<usize> {
        let text = self.caption.trim();
        if text.is_empty() {
            return None;
        }
        Some(self.editor.add(text))
    }

    /// What "Apply" writes to the selected item: its caption and its shortcut
    /// (from the toggles and key; an empty key clears the shortcut).
    pub fn apply_caption(&mut self) -> bool {
        let text = self.caption.trim().to_string();
        match self.editor.selected {
            Some(i) if !text.is_empty() => {
                self.editor.items[i].caption = text;
                self.editor.items[i].shortcut = self.shortcut();
                true
            }
            _ => false,
        }
    }

    /// Selecting a row fills the Caption box, the toggles and the key with that
    /// item's caption and shortcut.
    pub fn select(&mut self, i: Option<usize>) {
        self.editor.select(i);
        self.key_focus = false;
        if let Some(k) = self.editor.selected {
            self.caption = display_caption(&self.editor.items[k].caption);
            self.caret = self.caption.chars().count();
            let (mods, key) = oxbasic_ui::menu::shortcut_parts(&self.editor.items[k].shortcut);
            self.mods = mods;
            self.key = key;
        }
    }
}

impl gpui::Render for MenuEditorView {
    fn render(&mut self, _win: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<gpui::Stateful<gpui::Div>> = self
            .editor
            .rows()
            .into_iter()
            .map(|(i, depth)| {
                let selected = self.editor.selected == Some(i);
                div()
                    .id(("menu-row", i))
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(rgb(0xCFE3FF)))
                    .child(SharedString::from(Self::row_label(
                        &self.editor.items[i],
                        depth,
                    )))
                    .on_click(cx.listener(move |this, _ev: &gpui::ClickEvent, _win, cx| {
                        this.select(Some(i));
                        cx.notify();
                    }))
            })
            .collect();
        div()
            .size_full()
            .bg(rgb(0xF2F2F7))
            .text_color(rgb(0x1D1D1F))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .key_context("MenuEditor")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _win, cx| {
                match ev.keystroke.key.as_str() {
                    "backspace" => {
                        if this.key_focus {
                            this.key.clear();
                        } else {
                            this.backspace_caption();
                        }
                    }
                    "enter" => {
                        this.apply_caption();
                    }
                    _ => {
                        if let Some(text) = ev.keystroke.key_char.as_deref() {
                            if !ev.keystroke.modifiers.platform && !ev.keystroke.modifiers.control {
                                if this.key_focus {
                                    this.type_key(text);
                                } else {
                                    this.type_caption(text);
                                }
                            }
                        }
                    }
                }
                cx.notify();
            }))
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Menu Editor"),
            )
            .child(
                div()
                    .flex_1()
                    .border_1()
                    .border_color(rgb(0xBBBBBB))
                    .overflow_hidden()
                    .children(rows),
            )
            .child(div().child(format!("Caption: {}|", self.caption)))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .items_center()
                    .child(div().child("Shortcut:"))
                    .child(modifier_toggle("mod-ctrl", "⌃ Ctrl", self.mods.ctrl).on_click(
                        cx.listener(|this, _ev: &gpui::ClickEvent, _win, cx| {
                            this.toggle(0);
                            cx.notify();
                        }),
                    ))
                    .child(modifier_toggle("mod-cmd", "⌘ Cmd", self.mods.cmd).on_click(
                        cx.listener(|this, _ev: &gpui::ClickEvent, _win, cx| {
                            this.toggle(1);
                            cx.notify();
                        }),
                    ))
                    .child(modifier_toggle("mod-alt", "⌥ Alt", self.mods.alt).on_click(
                        cx.listener(|this, _ev: &gpui::ClickEvent, _win, cx| {
                            this.toggle(2);
                            cx.notify();
                        }),
                    ))
                    .child(modifier_toggle("mod-shift", "⇧ Shift", self.mods.shift).on_click(
                        cx.listener(|this, _ev: &gpui::ClickEvent, _win, cx| {
                            this.toggle(3);
                            cx.notify();
                        }),
                    ))
                    .child(
                        div()
                            .id("menu-key")
                            .px_2()
                            .w(px(60.))
                            .border_1()
                            .border_color(if self.key_focus { rgb(0x3478F6) } else { rgb(0xBBBBBB) })
                            .bg(rgb(0xFFFFFF))
                            .cursor_text()
                            .on_click(cx.listener(|this, _ev: &gpui::ClickEvent, _win, cx| {
                                this.key_focus = true;
                                cx.notify();
                            }))
                            .child(format!(
                                "{}",
                                if self.key.is_empty() { "none" } else { &self.key }
                            )),
                    ),
            )
            .child(div().child(format!(
                "Shortcut shown: {}",
                if self.shortcut().is_empty() { "(none)".to_string() } else { self.shortcut() }
            )))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(menu_button("menu-new", "New").on_click(cx.listener(
                        |this, _ev: &gpui::ClickEvent, _win, cx| {
                            this.new_item();
                            cx.notify();
                        },
                    )))
                    .child(menu_button("menu-delete", "Delete").on_click(cx.listener(
                        |this, _ev: &gpui::ClickEvent, _win, cx| {
                            this.delete_selected();
                            cx.notify();
                        },
                    )))
                    .child(menu_button("menu-indent", "Indent →").on_click(cx.listener(
                        |this, _ev: &gpui::ClickEvent, _win, cx| {
                            this.editor.indent();
                            cx.notify();
                        },
                    )))
                    .child(
                        menu_button("menu-outdent", "Outdent ←").on_click(cx.listener(
                            |this, _ev: &gpui::ClickEvent, _win, cx| {
                                this.editor.outdent();
                                cx.notify();
                            },
                        )),
                    )
                    .child(menu_button("menu-apply", "Apply").on_click(cx.listener(
                        |this, _ev: &gpui::ClickEvent, _win, cx| {
                            this.apply_caption();
                            cx.notify();
                        },
                    ))),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x555555))
                    .child("Closing the window keeps the menu with the form."),
            )
    }
}

/// A modifier toggle: a bordered label, filled when the modifier is on.
fn modifier_toggle(id: &'static str, label: &'static str, on: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_2()
        .py_1()
        .border_1()
        .border_color(rgb(0xBBBBBB))
        .rounded_md()
        .cursor_pointer()
        .when(on, |d| d.bg(rgb(0xCFE3FF)))
        .child(label)
}

/// One of the Menu Editor's buttons: a bordered, clickable label.
fn menu_button(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_3()
        .py_1()
        .border_1()
        .border_color(rgb(0xBBBBBB))
        .rounded_md()
        .cursor_pointer()
        .child(label)
}

impl MenuEditorView {
    /// "Delete": remove the selected item and its submenu.
    pub fn delete_selected(&mut self) -> bool {
        let done = self.editor.delete();
        if done {
            self.caption.clear();
            self.caret = 0;
        }
        done
    }
}

impl Ide {
    /// Open the Menu Editor for the designer's form: a window on a copy of the
    /// form's menu. Done (or closing it) writes the copy back to the form.
    pub(super) fn open_menu_editor(&mut self, cx: &mut Context<Self>) {
        self.store_active();
        let items = self.form.menus.clone();
        let view = cx.new(|cx| MenuEditorView::new(items, cx));
        let ide = cx.entity().downgrade();
        let bounds = Bounds::centered(None, size(px(520.), px(480.)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(SharedString::from("Menu Editor")),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |win, cx| {
                // closing the window keeps its menu: write the copy back first
                let view_for_close = view.clone();
                let ide_for_close = ide.clone();
                win.on_window_should_close(cx, move |_win, cx| {
                    let items = view_for_close.read(cx).editor.items.clone();
                    if let Some(ide) = ide_for_close.upgrade() {
                        ide.update(cx, |ide, cx| ide.apply_menu_items(items, cx));
                    }
                    true
                });
                view
            },
        );
        if let Err(e) = opened {
            self.status = format!("cannot open the Menu Editor: {}", e);
            cx.notify();
        }
    }

    /// Write the Menu Editor's items into the designer's form: the menu saves
    /// with the project, which is then unsaved.
    pub(super) fn apply_menu_items(&mut self, items: Vec<MenuItem>, cx: &mut Context<Self>) {
        self.form.menus = items;
        self.workspace.active_doc_mut().mark_unsaved();
        self.status = "menu updated".into();
        cx.notify();
    }
}
