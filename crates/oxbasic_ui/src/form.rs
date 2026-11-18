//! The render-agnostic form model: classic VB forms and controls with
//! their layout coordinates, properties, and `.oxform` JSON layout
//! serialization (spec §6), plus the event dispatch that connects UI
//! events to BASIC subroutines (spec §5.1).

use crate::{handler_name, EventDispatcher, EventRegistry};
use serde::{Deserialize, Serialize};

/// Classic BASIC control kinds (spec §5 mapping table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlKind {
    CommandButton,
    Label,
    TextBox,
    Frame,
    ListBox,
    ComboBox,
    PictureBox,
    Timer,
}

impl ControlKind {
    /// The BASIC type name as it appears in code / `.oxform` files.
    pub fn basic_name(&self) -> &'static str {
        match self {
            ControlKind::CommandButton => "CommandButton",
            ControlKind::Label => "Label",
            ControlKind::TextBox => "TextBox",
            ControlKind::Frame => "Frame",
            ControlKind::ListBox => "ListBox",
            ControlKind::ComboBox => "ComboBox",
            ControlKind::PictureBox => "PictureBox",
            ControlKind::Timer => "Timer",
        }
    }

    /// Parse a control kind from its BASIC name (case-insensitive).
    pub fn from_basic_name(name: &str) -> Option<ControlKind> {
        Some(match name.to_ascii_lowercase().as_str() {
            "commandbutton" | "button" => ControlKind::CommandButton,
            "label" => ControlKind::Label,
            "textbox" | "text" => ControlKind::TextBox,
            "frame" => ControlKind::Frame,
            "listbox" | "list" => ControlKind::ListBox,
            "combobox" | "combo" => ControlKind::ComboBox,
            "picturebox" | "picture" | "image" => ControlKind::PictureBox,
            "timer" => ControlKind::Timer,
            _ => return None,
        })
    }
}

/// A control's position and size in VB form coordinates (pixels here;
/// twips conversion arrives with the visual designer in Phase 3).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(left: f32, top: f32, width: f32, height: f32) -> Rect {
        Rect {
            left,
            top,
            width,
            height,
        }
    }
}

/// One classic BASIC control on a form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlSpec {
    /// BASIC name, e.g. `Command1` — event subs derive from it
    /// (`Command1_Click`).
    pub name: String,
    pub kind: ControlKind,
    pub rect: Rect,
    /// `Caption` for buttons/labels/frames; window text for TextBox.
    #[serde(default)]
    pub caption: String,
    /// `Text` property (TextBox). Defaults to `caption` when absent.
    #[serde(default)]
    pub text: Option<String>,
    /// Items for ListBox / ComboBox.
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Timer interval in milliseconds (Timer only).
    #[serde(default)]
    pub interval_ms: u64,
    /// Fill color for PictureBox, as 0xRRGGBB.
    #[serde(default)]
    pub color: Option<u32>,
    /// `Style` (ComboBox): 0 = Dropdown Combo (editable text + drop-down
    /// list), 1 = Simple Combo (editable text + always-visible list),
    /// 2 = Dropdown List (pick only). Other kinds ignore it.
    #[serde(default)]
    pub style: u8,
}

fn default_true() -> bool {
    true
}

impl ControlKind {
    /// Size in pixels a control gets when dropped from the toolbox, close
    /// to VB6's defaults (a Timer has no size: it is invisible at run time).
    pub fn default_size(&self) -> (f32, f32) {
        match self {
            ControlKind::CommandButton => (96.0, 28.0),
            ControlKind::Label => (100.0, 20.0),
            ControlKind::TextBox => (120.0, 24.0),
            ControlKind::Frame => (160.0, 100.0),
            ControlKind::ListBox => (120.0, 84.0),
            ControlKind::ComboBox => (120.0, 24.0),
            ControlKind::PictureBox => (100.0, 100.0),
            ControlKind::Timer => (0.0, 0.0),
        }
    }

    /// Whether the control has a `Caption` property in VB6.
    pub fn has_caption(&self) -> bool {
        matches!(
            self,
            ControlKind::CommandButton | ControlKind::Label | ControlKind::Frame
        )
    }

    /// Whether the control has an editable `Text` property in VB6.
    pub fn has_text(&self) -> bool {
        matches!(self, ControlKind::TextBox | ControlKind::ComboBox)
    }
}

impl ControlSpec {
    /// Effective text content (`Text` or `Caption`).
    pub fn text(&self) -> &str {
        self.text.as_deref().unwrap_or(&self.caption)
    }

    /// The conventional BASIC handler sub name for one of this control's
    /// events, e.g. `Command1` + `Click` → `Command1_Click`.
    pub fn handler(&self, event: &str) -> String {
        handler_name(&self.name, event)
    }
}

/// A classic VB form: top-level window plus its controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormSpec {
    pub name: String,
    pub caption: String,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub controls: Vec<ControlSpec>,
}

impl FormSpec {
    pub fn new(name: &str, caption: &str, width: f32, height: f32) -> FormSpec {
        FormSpec {
            name: name.to_string(),
            caption: caption.to_string(),
            width,
            height,
            controls: Vec::new(),
        }
    }

    /// Builder: add a control, returning `&mut Self` for chaining.
    pub fn control(&mut self, spec: ControlSpec) -> &mut Self {
        self.controls.push(spec);
        self
    }

    /// Builder shortcut for a command button.
    pub fn button(&mut self, name: &str, caption: &str, rect: Rect) -> &mut Self {
        self.control(ControlSpec {
            name: name.to_string(),
            kind: ControlKind::CommandButton,
            rect,
            caption: caption.to_string(),
            text: None,
            items: vec![],
            enabled: true,
            visible: true,
            interval_ms: 0,
            color: None,
            style: 0,
        })
    }

    /// Builder shortcut for a label.
    pub fn label(&mut self, name: &str, caption: &str, rect: Rect) -> &mut Self {
        self.control(ControlSpec {
            name: name.to_string(),
            kind: ControlKind::Label,
            rect,
            caption: caption.to_string(),
            text: None,
            items: vec![],
            enabled: true,
            visible: true,
            interval_ms: 0,
            color: None,
            style: 0,
        })
    }

    /// Builder shortcut for a text box.
    pub fn text_box(&mut self, name: &str, text: &str, rect: Rect) -> &mut Self {
        self.control(ControlSpec {
            name: name.to_string(),
            kind: ControlKind::TextBox,
            rect,
            caption: String::new(),
            text: Some(text.to_string()),
            items: vec![],
            enabled: true,
            visible: true,
            interval_ms: 0,
            color: None,
            style: 0,
        })
    }

    /// Builder shortcut for a timer.
    pub fn timer(&mut self, name: &str, interval_ms: u64) -> &mut Self {
        self.control(ControlSpec {
            name: name.to_string(),
            kind: ControlKind::Timer,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            caption: String::new(),
            text: None,
            items: vec![],
            enabled: true,
            visible: false,
            interval_ms,
            color: None,
            style: 0,
        })
    }

    /// Look up a control by BASIC name (case-insensitive).
    pub fn control_by_name(&self, name: &str) -> Option<&ControlSpec> {
        self.controls
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// Load a `.oxform` JSON layout specification (spec §6).
    pub fn from_oxform(json: &str) -> Result<FormSpec, String> {
        serde_json::from_str(json).map_err(|e| format!("invalid .oxform: {}", e))
    }

    /// Serialize to `.oxform` JSON (pretty).
    pub fn to_oxform(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialize form")
    }
}

/// The event dispatch engine connecting UI events to BASIC subroutines.
///
/// `on_sub` receives the fully-derived handler name (e.g.
/// `Command1_Click`) and performs the call into the BASIC runtime
/// (spec §5.1: `runtime.evaluate_sub(...)`). The form's own events
/// (`Form_Load`, `Form_Unload`) use the form's name.
pub struct FormDispatcher {
    form: FormSpec,
    registry: EventRegistry,
}

impl FormDispatcher {
    pub fn new(form: FormSpec) -> FormDispatcher {
        FormDispatcher {
            form,
            registry: EventRegistry::default(),
        }
    }

    pub fn form(&self) -> &FormSpec {
        &self.form
    }

    /// Wire every control event that has a conventional handler. The
    /// caller supplies the list of sub names the BASIC program actually
    /// defines, so no dead registrations are created.
    pub fn wire_defined_subs<'a, I>(&mut self, defined_subs: I)
    where
        I: IntoIterator,
        I::Item: AsRef<str> + 'a,
    {
        let defined: Vec<String> = defined_subs
            .into_iter()
            .map(|s| s.as_ref().to_lowercase())
            .collect();
        let mut wire = |control: &str, event: &str| {
            let h = handler_name(control, event);
            if defined.contains(&h.to_lowercase()) {
                self.registry.register(control, event, &h);
            }
        };
        for c in &self.form.controls {
            for ev in crate::events_for_control(c.kind.basic_name()) {
                wire(&c.name, ev);
            }
        }
        // Form-level events: classic VB uses `Form_Load` regardless of
        // the form's own name, so wire both spellings.
        for ev in crate::events_for_control("Form") {
            wire("Form", ev);
            wire(&self.form.name, ev);
        }
    }

    /// Dispatch a UI event to BASIC: `dispatch("Command1", "Click", &mut rt)`.
    pub fn dispatch<F>(
        &self,
        control: &str,
        event: &str,
        evaluate_sub: &mut F,
    ) -> Result<(), String>
    where
        F: FnMut(&str) -> Result<(), String>,
    {
        for h in &self.registry.handlers(control, event) {
            evaluate_sub(h).map_err(|e| format!("{} failed: {}", h, e))?;
        }
        Ok(())
    }

    /// Which handlers are wired for `(control, event)` — for tests/tools.
    pub fn handlers_for(&self, control: &str, event: &str) -> Vec<String> {
        self.registry.handlers(control, event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oxform_round_trip() {
        let mut form = FormSpec::new("Form1", "Click me", 320.0, 200.0);
        form.button("Command1", "OK", Rect::new(16.0, 150.0, 88.0, 28.0));
        form.label("Label1", "Hello", Rect::new(16.0, 16.0, 200.0, 20.0));
        form.text_box("Text1", "type here", Rect::new(16.0, 48.0, 200.0, 24.0));
        form.timer("Timer1", 250);

        let json = form.to_oxform();
        let back = FormSpec::from_oxform(&json).unwrap();
        assert_eq!(back.name, "Form1");
        assert_eq!(back.controls.len(), 4);
        assert_eq!(
            back.control_by_name("command1").unwrap().kind,
            ControlKind::CommandButton
        );
        assert_eq!(back.control_by_name("text1").unwrap().text(), "type here");
        assert_eq!(back.control_by_name("timer1").unwrap().interval_ms, 250);
        assert_eq!(back.controls[0].rect.left, 16.0);
    }

    #[test]
    fn control_kind_parsing() {
        assert_eq!(
            ControlKind::from_basic_name("CommandButton"),
            Some(ControlKind::CommandButton)
        );
        assert_eq!(
            ControlKind::from_basic_name("textbox"),
            Some(ControlKind::TextBox)
        );
        assert_eq!(ControlKind::from_basic_name("nope"), None);
        assert_eq!(ControlKind::Timer.basic_name(), "Timer");
    }

    #[test]
    fn dispatch_routes_to_basic_subs() {
        let mut form = FormSpec::new("Form1", "Demo", 300.0, 200.0);
        form.button("Command1", "OK", Rect::new(10.0, 10.0, 80.0, 24.0));
        let mut d = FormDispatcher::new(form);
        d.wire_defined_subs(["Command1_Click", "Form_Load", "Command1_MouseDown"]);

        let called = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let mut eval = {
            let called = called.clone();
            move |sub: &str| -> Result<(), String> {
                called.borrow_mut().push(sub.to_string());
                Ok(())
            }
        };
        d.dispatch("Command1", "Click", &mut eval).unwrap();
        assert_eq!(*called.borrow(), vec!["Command1_Click"]);

        called.borrow_mut().clear();
        d.dispatch("Command1", "MouseDown", &mut eval).unwrap();
        assert_eq!(*called.borrow(), vec!["Command1_MouseDown"]);
        called.borrow_mut().clear();
        d.dispatch("Command1", "Change", &mut eval).unwrap();
        assert!(called.borrow().is_empty());

        // form-level events: classic `Form_Load` spelling
        d.dispatch("Form", "Load", &mut eval).unwrap();
        assert_eq!(*called.borrow(), vec!["Form_Load"]);

        let mut d2 = FormDispatcher::new(FormSpec::new("Form1", "x", 10.0, 10.0));
        d2.wire_defined_subs(["Command9_Click"]);
        assert!(d2.handlers_for("Form1", "Load").is_empty());
    }

    #[test]
    fn missing_event_handlers_are_noops_and_defined_handler_errors_propagate() {
        let mut form = FormSpec::new("Form1", "Demo", 300.0, 200.0);
        form.text_box("Text1", "", Rect::new(10.0, 10.0, 100.0, 24.0));
        form.button("Command1", "OK", Rect::new(10.0, 40.0, 80.0, 24.0));
        let mut dispatcher = FormDispatcher::new(form);
        dispatcher.wire_defined_subs(["Command1_Click"]);
        let mut calls = Vec::new();
        let mut evaluate = |name: &str| -> Result<(), String> {
            calls.push(name.to_string());
            Err("handler failed".to_string())
        };

        dispatcher
            .dispatch("Text1", "Click", &mut evaluate)
            .unwrap();
        dispatcher.dispatch("Form", "Load", &mut evaluate).unwrap();
        assert_eq!(
            dispatcher.dispatch("command1", "click", &mut evaluate),
            Err("Command1_Click failed: handler failed".to_string())
        );
        assert_eq!(calls, vec!["Command1_Click"]);
    }

    /// End-to-end with the real interpreter: a click runs BASIC code.
    #[test]
    fn dispatch_runs_basic() {
        use oxbasic_runtime::{Interpreter, Output};

        struct Sink(String);
        impl Output for Sink {
            fn write(&mut self, s: &str) {
                self.0.push_str(s);
            }
        }

        let src = "Sub Command1_Click()\n  MsgBox \"clicked!\"\nEnd Sub\n\
                   Function Add(a As Long, b As Long) As Long\n  Add = a + b\nEnd Function\n";
        let prog = oxbasic_parser::parse(src).unwrap();
        let sink = std::rc::Rc::new(std::cell::RefCell::new(String::new()));

        struct SharedSink(std::rc::Rc<std::cell::RefCell<String>>);
        impl Output for SharedSink {
            fn write(&mut self, s: &str) {
                self.0.borrow_mut().push_str(s);
            }
        }

        let mut shared = SharedSink(sink.clone());
        let mut rt = Interpreter::new(&prog, &mut shared);

        rt.evaluate_sub("Command1_Click", &[]).unwrap();
        assert_eq!(*sink.borrow(), "clicked!\n");
        let v = rt
            .evaluate_sub(
                "Add",
                &[
                    oxbasic_runtime::Variant::Long(3),
                    oxbasic_runtime::Variant::Long(4),
                ],
            )
            .unwrap();
        assert!(matches!(v, oxbasic_runtime::Variant::Long(7)));

        assert!(rt.evaluate_sub("Nope", &[]).is_err());
    }
}
