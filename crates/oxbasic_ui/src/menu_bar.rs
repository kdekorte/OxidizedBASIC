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

/// The form's menu bar as GPUI menus. The first menu is the application's
/// own: it holds the top-level items marked `app_menu`, in order, and a Quit
/// of its own if the form has no quit item there. Every other top-level item
/// is a menu of its own, its children as its entries.
pub fn menu_bar(items: &[MenuItem]) -> Vec<Menu> {
    let top = bar(items);
    // macOS shows the first menu under the application's name. A separator
    // belongs to the application menu when it sits among its items.
    let mut app: Vec<GpuiMenuItem> = Vec::new();
    let mut has_quit = false;
    for &i in top.iter().filter(|&&i| items[i].app_menu || items[i].is_separator()) {
        if items[i].is_separator() {
            app.push(GpuiMenuItem::separator());
        } else if children(items, Some(i)).is_empty() {
            has_quit |= items[i].name == "mnuQuit";
            app.push(GpuiMenuItem::action(
                display_caption(&items[i].caption),
                FormMenuClick { index: i },
            ));
        } else {
            app.push(GpuiMenuItem::submenu(Menu {
                name: display_caption(&items[i].caption).into(),
                items: entries(items, Some(i)),
            }));
        }
    }
    let has_items = app.iter().any(|m| !matches!(m, GpuiMenuItem::Separator));
    if !has_quit {
        if has_items {
            app.push(GpuiMenuItem::separator());
        }
        app.push(GpuiMenuItem::action("Quit", AppQuit));
    }
    let mut out = vec![Menu {
        name: "App".into(),
        items: app,
    }];
    out.extend(
        top.into_iter()
            .filter(|&i| !items[i].app_menu && !items[i].is_separator())
            .map(|i| Menu {
                name: display_caption(&items[i].caption).into(),
                items: entries(items, Some(i)),
            }),
    );
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

    /// The showcase's menu: About and Quit under the application menu, and
    /// File with Drawing, Files and Tabs under it.
    fn showcase_menu() -> Vec<MenuItem> {
        let mut about = MenuItem::new("mnuAbout", "&About…", None);
        about.app_menu = true;
        let mut quit = MenuItem::new("mnuQuit", "&Quit", None);
        quit.app_menu = true;
        vec![
            about,
            MenuItem::new("mnuSep", "-", None),
            quit,
            MenuItem::new("mnuFile", "&File", None),
            MenuItem::new("mnuDrawing", "&Drawing…", Some(3)),
            MenuItem::new("mnuFiles", "&Files…", Some(3)),
        ]
    }

    #[test]
    fn the_bar_is_the_application_menu_then_file() {
        let names: Vec<String> = menu_bar(&showcase_menu())
            .iter()
            .map(|m| m.name.to_string())
            .collect();
        assert_eq!(names, vec!["App", "File"], "no Help menu");
    }

    #[test]
    fn about_quit_and_separator_sit_under_the_application_menu() {
        let bar = menu_bar(&showcase_menu());
        let app = &bar[0];
        assert_eq!(app.items.len(), 3, "About, a separator, Quit");
    }

    #[test]
    fn file_keeps_its_items_as_a_real_menu() {
        let bar = menu_bar(&showcase_menu());
        assert_eq!(bar[1].items.len(), 2, "Drawing and Files under File");
    }

    #[test]
    fn an_app_menu_without_quit_gets_a_quit_of_its_own() {
        let mut about = MenuItem::new("mnuAbout", "&About…", None);
        about.app_menu = true;
        let bar = menu_bar(&[about]);
        assert_eq!(bar[0].items.len(), 3, "About, a separator, then the built-in Quit");
    }

    #[test]
    fn a_form_quit_under_the_app_menu_is_not_duplicated() {
        let mut quit = MenuItem::new("mnuQuit", "&Quit", None);
        quit.app_menu = true;
        let bar = menu_bar(&[quit]);
        assert_eq!(bar[0].items.len(), 1, "only the form's own Quit");
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
