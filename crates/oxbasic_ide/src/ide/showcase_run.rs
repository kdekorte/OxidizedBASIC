//! A running showcase sample, driven the way its windows drive it: the
//! helper starts the checked-in project's program and sends the same
//! commands a click, a keystroke or a timer tick would.

use super::debug_session::*;
use super::program::combine;
use super::showcase::showcase;
use super::workspace::DocKind;
use oxbasic_ui::host::{FormSnapshot, UserEdit};
use oxbasic_ui::FormSpec;
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The sample's code as shipped (`examples/showcase/<name>.bas`).
fn shipped(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/showcase")
        .join(format!("{}.bas", name));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

pub struct Run {
    pub session: DebugSession,
    pub rx: Receiver<DebugEvent>,
    forms: Vec<FormSpec>,
    /// Form1 as it first appeared, after `Form_Load`.
    pub first: FormSnapshot,
}

impl Drop for Run {
    fn drop(&mut self) {
        self.session.send(DebugCommand::Stop);
    }
}

impl Run {
    /// Run the showcase and wait for its first window.
    pub fn start() -> Run {
        let w = showcase(shipped);
        let program = combine(&w.docs);
        let forms: Vec<FormSpec> = w
            .docs
            .iter()
            .filter(|d| d.kind == DocKind::Form)
            .map(|d| d.form.clone())
            .collect();
        let (tx, rx) = channel();
        let session = DebugSession::start_program(
            ProgramSpec {
                source: program.source,
                forms: forms.clone(),
                startup: 0,
                bare: false,
                base_dir: None,
            },
            Arc::new(Mutex::new(HashSet::new())),
            tx,
        );
        session.send(DebugCommand::Run);
        let mut run = Run {
            session,
            rx,
            forms,
            first: dummy(),
        };
        let seen = run.until(|e| matches!(e, DebugEvent::FormShown(s) if s.form_index == 0));
        let Some(DebugEvent::FormShown(snap)) = seen.into_iter().last() else {
            unreachable!()
        };
        run.first = snap;
        run
    }

    /// Events until `pred` accepts one (that one included).
    pub fn until(&self, pred: impl Fn(&DebugEvent) -> bool) -> Vec<DebugEvent> {
        let mut seen = Vec::new();
        loop {
            let ev = self
                .rx
                .recv_timeout(Duration::from_secs(5))
                .expect("the program went quiet");
            if let DebugEvent::Failed(e) = &ev {
                panic!("the program failed: {}", e);
            }
            let done = pred(&ev);
            seen.push(ev);
            if done {
                return seen;
            }
        }
    }

    /// The index of control `name` (element `index` of an array) on form
    /// `form`.
    pub fn control(&self, form: usize, name: &str, index: Option<i32>) -> usize {
        self.forms[form]
            .controls
            .iter()
            .position(|c| c.name == name && c.index == index)
            .unwrap_or_else(|| panic!("no control {} on form {}", name, form))
    }

    /// Fire `handler` on `form` and return the form's next changes.
    pub fn call(&self, form: usize, handler: &str, args: &[i32]) -> FormSnapshot {
        self.session
            .send(DebugCommand::Call(form, handler.into(), args.to_vec()));
        let seen = self.until(|e| matches!(e, DebugEvent::Form(s) if s.form_index == form));
        let Some(DebugEvent::Form(snap)) = seen.into_iter().last() else {
            unreachable!()
        };
        snap
    }

    /// The user typed `text` into a text box or combo.
    pub fn type_into(&self, form: usize, name: &str, text: &str) {
        let control = self.control(form, name, None);
        self.session.send(DebugCommand::Edit(
            form,
            UserEdit::Text {
                control,
                text: text.into(),
            },
        ));
    }

    /// The user clicked a CheckBox / OptionButton, which now has `value`.
    pub fn set_value(&self, form: usize, name: &str, value: i32) {
        let control = self.control(form, name, None);
        self.session
            .send(DebugCommand::Edit(form, UserEdit::Value { control, value }));
    }

    /// The user picked row `index` of a list or combo.
    pub fn pick(&self, form: usize, name: &str, index: usize) {
        let control = self.control(form, name, None);
        self.session
            .send(DebugCommand::Edit(form, UserEdit::Pick { control, index }));
    }
}

fn dummy() -> FormSnapshot {
    FormSnapshot {
        form: FormSpec::new("", "", 0.0, 0.0),
        state: vec![],
        touched: vec![],
        focus: None,
        form_index: 0,
    }
}

/// The caption of the control called `name` in `snap`.
pub fn caption(snap: &FormSnapshot, name: &str) -> String {
    snap.form
        .controls
        .iter()
        .find(|c| c.name == name && c.index.is_none())
        .unwrap_or_else(|| panic!("no control {}", name))
        .caption
        .clone()
}
