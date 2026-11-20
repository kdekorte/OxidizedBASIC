//! The run-time menu bar of a form: the form's `menus` as the application's
//! menu bar while its window is active. Each item is an action carrying its
//! index in the form's `menus`; running it raises the item's `Click`.
//!
//! Built only with the `gpui` feature, since the bar is GPUI's.

use crate::menu::{bar, children, MenuItem};
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
                GpuiMenuItem::action(m.caption.clone(), FormMenuClick { index: i })
            } else {
                GpuiMenuItem::submenu(gpui::Menu {
                    name: m.caption.clone().into(),
                    items: entries(items, Some(i)),
                })
            }
        })
        .collect()
}

/// The form's menu bar as GPUI menus: one menu per top-level item, its
/// children as its entries. A form with no menu gives none.
pub fn menu_bar(items: &[MenuItem]) -> Vec<Menu> {
    bar(items)
        .into_iter()
        .map(|i| Menu {
            name: items[i].caption.clone().into(),
            items: entries(items, Some(i)),
        })
        .collect()
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
    cx.set_menus(menu_bar(items));
}
