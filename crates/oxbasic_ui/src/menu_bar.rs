//! The run-time menu bar of a form: the form's `menus` as the application's
//! menu bar while its window is active. Each item is an action carrying its
//! index in the form's `menus`; running it raises the item's `Click`.
//!
//! Built only with the `gpui` feature, since the bar is GPUI's.

use crate::menu::{bar, children, display_caption, MenuItem};
use gpui::{Action, App, KeyBinding, Menu, MenuItem as GpuiMenuItem};

/// A form menu item was chosen: `index` is its position in the form's `menus`.
#[derive(Clone, PartialEq, Debug)]
pub struct FormMenuClick {
    pub index: usize,
}

impl Action for FormMenuClick {
    fn boxed_clone(&self) -> Box<dyn Action> {
        Box::new(self.clone())
    }

    fn partial_eq(&self, action: &dyn Action) -> bool {
        action
            .as_any()
            .downcast_ref::<FormMenuClick>()
            .is_some_and(|other| other == self)
    }

    fn name(&self) -> &'static str {
        "oxbasic::FormMenuClick"
    }

    fn name_for_type() -> &'static str
    where
        Self: Sized,
    {
        "oxbasic::FormMenuClick"
    }

    fn build(_value: serde_json::Value) -> gpui::Result<Box<dyn Action>>
    where
        Self: Sized,
    {
        Err(std::io::Error::other("FormMenuClick is built in code, not from a keymap").into())
    }
}

/// The menu entries of one submenu (or the bar's top level, as `Menu`s).
fn entries(items: &[MenuItem], parent: Option<usize>) -> Vec<GpuiMenuItem> {
    children(items, parent)
        .into_iter()
        .map(|i| {
            let m = &items[i];
            if m.is_separator() {
                return GpuiMenuItem::separator();
            }
            if children(items, Some(i)).is_empty() {
                GpuiMenuItem::action(display_caption(&m.caption), FormMenuClick { index: i })
            } else {
                GpuiMenuItem::submenu(gpui::Menu {
                    name: display_caption(&m.caption).into(),
                    items: entries(items, Some(i)),
                })
            }
        })
        .collect()
}

/// The form's menu bar as GPUI menus: one menu per top-level item, its
/// children as its entries. A form with no menu gives none.
pub fn menu_bar(items: &[MenuItem]) -> Vec<Menu> {
    // macOS shows the first menu as the application's own, so the bar starts
    // with an app menu (as the IDE's does); the form's menus follow it as
    // real menus, not as its app menu
    let mut out = vec![Menu {
        name: "App".into(),
        items: vec![GpuiMenuItem::action("Quit", AppQuit)],
    }];
    out.extend(bar(items).into_iter().map(|i| Menu {
        name: display_caption(&items[i].caption).into(),
        items: entries(items, Some(i)),
    }));
    out
}

/// Quit from a form's app menu: the whole application ends, as the IDE's
/// own Quit does.
#[derive(Clone, PartialEq, Debug)]
pub struct AppQuit;

impl Action for AppQuit {
    fn boxed_clone(&self) -> Box<dyn Action> {
        Box::new(self.clone())
    }

    fn partial_eq(&self, action: &dyn Action) -> bool {
        action.as_any().downcast_ref::<AppQuit>().is_some()
    }

    fn name(&self) -> &'static str {
        "oxbasic::AppQuit"
    }

    fn name_for_type() -> &'static str
    where
        Self: Sized,
    {
        "oxbasic::AppQuit"
    }

    fn build(_value: serde_json::Value) -> gpui::Result<Box<dyn Action>>
    where
        Self: Sized,
    {
        Err(std::io::Error::other("AppQuit is built in code, not from a keymap").into())
    }
}

/// The key bindings the form's shortcuts need, one per valid item.
pub fn key_bindings(items: &[MenuItem]) -> Vec<KeyBinding> {
    items
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let keys = crate::menu::keystroke_for(&m.shortcut)?;
            Some(KeyBinding::new(&keys, FormMenuClick { index: i }, None))
        })
        .collect()
}

/// Make the form's bar the application's while its window is active.
pub fn install(items: &[MenuItem], cx: &mut App) {
    cx.bind_keys(key_bindings(items));
    cx.on_action(|_: &AppQuit, cx| cx.quit());
    cx.set_menus(menu_bar(items));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::MenuItem;

    /// Form1's menu: File (Drawing, Quit) and Help (About), as the showcase has it.
    fn showcase_menu() -> Vec<MenuItem> {
        let mut file = MenuItem::new("mnuFile", "&File", None);
        file.shortcut = String::new();
        vec![
            file,
            MenuItem::new("mnuDrawing", "&Drawing…", Some(0)),
            MenuItem::new("mnuQuit", "&Quit", Some(0)),
            MenuItem::new("mnuHelp", "&Help", None),
            MenuItem::new("mnuAbout", "&About…", Some(3)),
        ]
    }

    #[test]
    fn the_bar_starts_with_an_app_menu_then_the_forms_own_menus() {
        let names: Vec<String> = menu_bar(&showcase_menu())
            .iter()
            .map(|m| m.name.to_string())
            .collect();
        assert_eq!(names, vec!["App", "File", "Help"]);
    }

    #[test]
    fn file_and_help_keep_their_entries_as_real_menus() {
        let bar = menu_bar(&showcase_menu());
        let file = &bar[1];
        assert_eq!(file.items.len(), 2, "Drawing and Quit under File");
        let help = &bar[2];
        assert_eq!(help.items.len(), 1, "About under Help");
    }

    #[test]
    fn a_form_with_no_menu_gets_only_the_app_menu() {
        let names: Vec<String> = menu_bar(&[])
            .iter()
            .map(|m| m.name.to_string())
            .collect();
        assert_eq!(names, vec!["App"]);
    }
}
