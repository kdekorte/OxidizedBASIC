//! GPUI & GPUI Kit widget mapping runtime engine.
//!
//! Phase 2 of the roadmap: this crate maps classic Visual BASIC controls
//! to high-performance GPU-rendered GPUI elements and routes GPUI input
//! events to BASIC event handler subroutines, per §5 of the technical
//! specification:
//!
//! ```ignore
//! button.on_click(cx.listener(|this, _event, window_cx| {
//!     this.runtime.evaluate_sub("Command1_Click", vec![]);
//! }));
//! ```
//!
//! - [`form`] holds the render-agnostic form model (`.oxform` JSON,
//!   layout coordinates, properties) and the [`FormDispatcher`] event
//!   engine — usable and tested without a GUI.
//! - `gpui_backend` (behind the `gpui` feature) renders a [`FormSpec`]
//!   as a real GPU-accelerated GPUI window and drives the dispatcher
//!   from GPUI input/timer events.

mod form;
pub mod menu;

#[cfg(test)]
mod dir_tests;
#[cfg(test)]
mod file_tests;
#[cfg(test)]
mod range_tests;
#[cfg(test)]
mod tab_tests;
#[cfg(test)]
mod menu_tests;
#[cfg(feature = "gpui")]
pub mod gpui_backend;
#[cfg(feature = "gpui")]
pub mod gpui_controls;
#[cfg(feature = "gpui")]
pub mod gpui_text_hit;

pub mod canvas;
pub mod events;
pub mod host;
mod host_controls;
mod host_draw;
#[cfg(target_os = "macos")]
extern crate objc;
pub mod mac_cursor;
pub use host_controls::{container_key, split_element};
pub mod project;
pub mod project_host;
pub use form::*;

use std::collections::HashMap;

/// One row of the classic-control → GPUI Kit mapping table (spec §5).
#[derive(Debug, Clone)]
pub struct ControlMapping {
    /// Classic BASIC control name (e.g. `CommandButton`).
    pub basic_control: &'static str,
    /// GPUI / GPUI Kit component it maps to.
    pub gpui_component: &'static str,
    /// Description of the mapped behaviour.
    pub description: &'static str,
}

/// The full classic-control → GPUI Kit mapping table.
pub fn mappings() -> Vec<ControlMapping> {
    vec![
        ControlMapping {
            basic_control: "Form",
            gpui_component: "gpui::Window / Root",
            description: "Top-level GPU window with Flex layout engine",
        },
        ControlMapping {
            basic_control: "CommandButton",
            gpui_component: "gpui_kit::component::button::Button",
            description: "Interactive button with hover/active/focus states",
        },
        ControlMapping {
            basic_control: "TextBox",
            gpui_component: "gpui_kit::component::input::Input / Rope Editor",
            description: "High-performance text edit field",
        },
        ControlMapping {
            basic_control: "Label",
            gpui_component: "gpui::elements::Label / Text",
            description: "GPU text-shapen static text display",
        },
        ControlMapping {
            basic_control: "Frame",
            gpui_component: "div().border()",
            description: "Flexbox container with customizable border/padding",
        },
        ControlMapping {
            basic_control: "ListBox",
            gpui_component: "gpui_kit::component::table / Virtualized List",
            description: "Virtual-scrolled list item view",
        },
        ControlMapping {
            basic_control: "ComboBox",
            gpui_component: "gpui_kit::component::select",
            description: "Dropdown select component",
        },
        ControlMapping {
            basic_control: "PictureBox",
            gpui_component: "gpui::img()",
            description: "Accelerated image loader & canvas frame",
        },
        ControlMapping {
            basic_control: "Timer",
            gpui_component: "gpui::Context::spawn / Async Timer",
            description: "Non-blocking async loop timer thread",
        },
        ControlMapping {
            basic_control: "CheckBox",
            gpui_component: "div() + check mark",
            description: "Tri-state toggle: unchecked, checked, grayed",
        },
        ControlMapping {
            basic_control: "OptionButton",
            gpui_component: "div() + radio mark",
            description: "Exclusive choice within the form's option group",
        },
    ]
}

/// Look up the GPUI Kit component for a classic control name
/// (case-insensitive).
pub fn map_control(name: &str) -> Option<&'static str> {
    mappings()
        .into_iter()
        .find(|m| m.basic_control.eq_ignore_ascii_case(name))
        .map(|m| m.gpui_component)
}

const KEY_UPDOWN: &str = "KeyCode As Integer, Shift As Integer";
const KEY_PRESS: &str = "KeyAscii As Integer";
const MOUSE: &str = "Button As Integer, Shift As Integer, X As Single, Y As Single";

/// The classic VB6 events each control kind raises, as
/// `(event name, handler parameter list)`, sorted by name the way VB's
/// procedure list shows them. The parameter list is what a generated
/// `Private Sub <Control>_<Event>(…)` placeholder declares.
pub fn event_signatures(control: &str) -> &'static [(&'static str, &'static str)] {
    match control.to_ascii_lowercase().as_str() {
        "commandbutton" => &[
            ("Click", ""),
            ("GotFocus", ""),
            ("KeyDown", KEY_UPDOWN),
            ("KeyPress", KEY_PRESS),
            ("KeyUp", KEY_UPDOWN),
            ("LostFocus", ""),
            ("MouseDown", MOUSE),
            ("MouseMove", MOUSE),
            ("MouseUp", MOUSE),
        ],
        "label" => &[
            ("Change", ""),
            ("Click", ""),
            ("DblClick", ""),
            ("MouseDown", MOUSE),
            ("MouseMove", MOUSE),
            ("MouseUp", MOUSE),
        ],
        "textbox" => &[
            ("Change", ""),
            ("Click", ""),
            ("DblClick", ""),
            ("GotFocus", ""),
            ("KeyDown", KEY_UPDOWN),
            ("KeyPress", KEY_PRESS),
            ("KeyUp", KEY_UPDOWN),
            ("LostFocus", ""),
            ("MouseDown", MOUSE),
            ("MouseMove", MOUSE),
            ("MouseUp", MOUSE),
        ],
        "frame" | "image" => &[
            ("Click", ""),
            ("DblClick", ""),
            ("MouseDown", MOUSE),
            ("MouseMove", MOUSE),
            ("MouseUp", MOUSE),
        ],
        // VB6's ListBox has no Change; ItemCheck needs Style 1
        // (checkboxes), which these lists don't have
        "listbox" => &[
            ("Click", ""),
            ("DblClick", ""),
            ("GotFocus", ""),
            ("KeyDown", KEY_UPDOWN),
            ("KeyPress", KEY_PRESS),
            ("KeyUp", KEY_UPDOWN),
            ("LostFocus", ""),
            ("MouseDown", MOUSE),
            ("MouseMove", MOUSE),
            ("MouseUp", MOUSE),
            ("Scroll", ""),
        ],
        "combobox" => &[
            ("Change", ""),
            ("Click", ""),
            ("DblClick", ""),
            ("DropDown", ""),
            ("GotFocus", ""),
            ("KeyDown", KEY_UPDOWN),
            ("KeyPress", KEY_PRESS),
            ("KeyUp", KEY_UPDOWN),
            ("LostFocus", ""),
            ("Scroll", ""),
        ],
        // Change: the Picture changed
        "picturebox" => &[
            ("Change", ""),
            ("Click", ""),
            ("DblClick", ""),
            ("GotFocus", ""),
            ("KeyDown", KEY_UPDOWN),
            ("KeyPress", KEY_PRESS),
            ("KeyUp", KEY_UPDOWN),
            ("LostFocus", ""),
            ("MouseDown", MOUSE),
            ("MouseMove", MOUSE),
            ("MouseUp", MOUSE),
            ("Paint", ""),
            ("Resize", ""),
        ],
        "timer" => &[("Timer", "")],
        "checkbox" | "optionbutton" => &[
            ("Click", ""),
            ("GotFocus", ""),
            ("KeyDown", KEY_UPDOWN),
            ("KeyPress", KEY_PRESS),
            ("KeyUp", KEY_UPDOWN),
            ("LostFocus", ""),
            ("MouseDown", MOUSE),
            ("MouseMove", MOUSE),
            ("MouseUp", MOUSE),
        ],
        "form" => &[
            ("Activate", ""),
            ("Click", ""),
            ("DblClick", ""),
            ("Deactivate", ""),
            ("KeyDown", KEY_UPDOWN),
            ("KeyPress", KEY_PRESS),
            ("KeyUp", KEY_UPDOWN),
            ("Load", ""),
            ("MouseDown", MOUSE),
            ("MouseMove", MOUSE),
            ("MouseUp", MOUSE),
            ("Paint", ""),
            ("QueryUnload", "Cancel As Integer, UnloadMode As Integer"),
            ("Resize", ""),
            ("Unload", "Cancel As Integer"),
        ],
        _ => &[],
    }
}

/// The classic BASIC event names each control can raise.
pub fn events_for_control(control: &str) -> Vec<&'static str> {
    event_signatures(control).iter().map(|(e, _)| *e).collect()
}

/// The handler parameter list for `control`'s `event` (case-insensitive),
/// or `None` when that control kind doesn't raise the event.
pub fn event_params(control: &str, event: &str) -> Option<&'static str> {
    event_signatures(control)
        .iter()
        .find(|(e, _)| e.eq_ignore_ascii_case(event))
        .map(|(_, p)| *p)
}

/// The event VB's designer opens when a control is double-clicked
/// (`Command1` → `Click`, `Text1` → `Change`, `Form` → `Load`, …).
pub fn default_event(control: &str) -> Option<&'static str> {
    Some(match control.to_ascii_lowercase().as_str() {
        "commandbutton" | "label" | "frame" | "listbox" | "picturebox" | "checkbox"
        | "optionbutton" | "image" => "Click",
        "textbox" | "combobox" => "Change",
        "timer" => "Timer",
        "form" => "Load",
        _ => return None,
    })
}

/// Registry connecting GPUI input events to BASIC event-handler
/// subroutines (`<ControlName>_<Event>`), per the spec's event
/// dispatcher engine. Render-backend agnostic so the interpreter tests
/// can drive it without GPUI.
pub trait EventDispatcher {
    /// Register `handler` (a BASIC sub name) for `control`'s `event`.
    fn register(&mut self, control: &str, event: &str, handler: &str);
    /// All registered handlers for `control`'s `event`.
    fn handlers(&self, control: &str, event: &str) -> Vec<String>;
    /// Remove every registration (e.g. `Form_Unload`).
    fn clear(&mut self);
}

/// Default in-memory [`EventDispatcher`] implementation.
#[derive(Debug, Default)]
pub struct EventRegistry {
    map: HashMap<String, Vec<String>>,
}

impl EventRegistry {
    fn key(control: &str, event: &str) -> String {
        format!(
            "{}_{}",
            control.to_ascii_lowercase(),
            event.to_ascii_lowercase()
        )
    }
}

impl EventDispatcher for EventRegistry {
    fn register(&mut self, control: &str, event: &str, handler: &str) {
        self.map
            .entry(Self::key(control, event))
            .or_default()
            .push(handler.to_string());
    }

    fn handlers(&self, control: &str, event: &str) -> Vec<String> {
        self.map
            .get(&Self::key(control, event))
            .cloned()
            .unwrap_or_default()
    }

    fn clear(&mut self) {
        self.map.clear();
    }
}

/// Derive the conventional BASIC handler sub name for a control event,
/// e.g. `Command1` + `Click` → `Command1_Click`.
pub fn handler_name(control: &str, event: &str) -> String {
    format!("{}_{}", control, event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_table_matches_spec() {
        assert_eq!(mappings().len(), 11);
        assert_eq!(
            map_control("CommandButton"),
            Some("gpui_kit::component::button::Button")
        );
        assert_eq!(
            map_control("commandbutton"),
            Some("gpui_kit::component::button::Button")
        );
        assert!(map_control("Unknown").is_none());
    }

    #[test]
    fn event_registry_routes_events() {
        let mut reg = EventRegistry::default();
        reg.register("Command1", "Click", "Command1_Click");
        assert_eq!(reg.handlers("Command1", "Click"), vec!["Command1_Click"]);
        assert!(reg.handlers("Command1", "MouseDown").is_empty());
        assert_eq!(handler_name("Command1", "Click"), "Command1_Click");
        reg.clear();
        assert!(reg.handlers("Command1", "Click").is_empty());
    }

    #[test]
    fn control_events() {
        assert!(events_for_control("Timer").contains(&"Timer"));
        assert!(events_for_control("form").contains(&"Load"));
        // a button doesn't raise Change
        assert!(!events_for_control("CommandButton").contains(&"Change"));
    }

    #[test]
    fn event_signatures_defaults_and_params() {
        for kind in [
            "CommandButton",
            "Label",
            "TextBox",
            "Frame",
            "ListBox",
            "ComboBox",
            "PictureBox",
            "Image",
            "Timer",
            "CheckBox",
            "OptionButton",
            "Form",
        ] {
            let events = events_for_control(kind);
            assert!(!events.is_empty(), "{} has no events", kind);
            let mut sorted = events.clone();
            sorted.sort();
            assert_eq!(events, sorted, "{} events must be sorted", kind);
            let default = default_event(kind).expect("default event");
            assert!(events.contains(&default), "{} default {}", kind, default);
        }
        assert_eq!(event_params("CommandButton", "click"), Some(""));
        assert_eq!(
            event_params("TextBox", "KeyPress"),
            Some("KeyAscii As Integer")
        );
        assert_eq!(event_params("Form", "Unload"), Some("Cancel As Integer"));
        assert_eq!(event_params("Timer", "Click"), None);
        assert_eq!(default_event("Unknown"), None);
    }
}
