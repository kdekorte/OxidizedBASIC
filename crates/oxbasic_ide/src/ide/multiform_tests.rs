//! Multi-form programs: a project's forms run as one program, with each
//! form's handlers and variables kept apart, `Show`/`Hide`/`Unload`
//! driving which windows exist, and `End` finishing the lot.

use super::debug_session::*;
use super::program::combine;
use super::workspace::{to_lines, Doc};
use oxbasic_ui::{ControlKind, ControlSpec, FormSpec, Rect};
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A form with a label and a button.
fn form(name: &str, label: &str) -> FormSpec {
    let mut f = FormSpec::new(name, name, 300.0, 200.0);
    f.label("Label1", label, Rect::new(0.0, 0.0, 100.0, 20.0));
    f.control(ControlSpec::new(
        "Command1",
        ControlKind::CommandButton,
        Rect::new(0.0, 30.0, 100.0, 28.0),
    ));
    f
}

fn doc(name: &str, layout: FormSpec, code: &str) -> Doc {
    let mut d = Doc::new_form(name);
    d.form = layout;
    d.lines = to_lines(code);
    d
}

/// Start a program for `docs` and return its session and events.
fn start(docs: &[Doc]) -> (DebugSession, Receiver<DebugEvent>) {
    let program = combine(docs);
    let forms = docs
        .iter()
        .filter(|d| d.kind == super::workspace::DocKind::Form)
        .map(|d| d.form.clone())
        .collect();
    let (tx, rx) = channel();
    let session = DebugSession::start_program(
        ProgramSpec {
            source: program.source,
            forms,
            startup: 0,
            bare: false,
        },
        Arc::new(Mutex::new(HashSet::new())),
        tx,
    );
    session.send(DebugCommand::Run);
    (session, rx)
}

fn recv(rx: &Receiver<DebugEvent>) -> DebugEvent {
    rx.recv_timeout(Duration::from_secs(5)).expect("an event")
}

/// Events until `pred` accepts one; the ones before it are returned too.
fn until(rx: &Receiver<DebugEvent>, pred: impl Fn(&DebugEvent) -> bool) -> Vec<DebugEvent> {
    let mut seen = Vec::new();
    loop {
        let ev = recv(rx);
        let done = pred(&ev);
        seen.push(ev);
        if done {
            return seen;
        }
    }
}

fn shown(ev: &DebugEvent) -> Option<usize> {
    match ev {
        DebugEvent::FormShown(s) => Some(s.form_index),
        _ => None,
    }
}

fn output(events: &[DebugEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            DebugEvent::Output(s) => Some(s.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_startup_form_opens_and_a_click_reaches_its_own_handler() {
    let docs = [
        doc(
            "Form1",
            form("Form1", "one"),
            "Sub Command1_Click()\n  Label1.Caption = \"clicked one\"\nEnd Sub",
        ),
        doc(
            "Form2",
            form("Form2", "two"),
            "Sub Command1_Click()\n  Label1.Caption = \"clicked two\"\nEnd Sub",
        ),
    ];
    let (session, rx) = start(&docs);
    // only the startup form is shown
    let seen = until(&rx, |e| shown(e).is_some());
    assert_eq!(seen.iter().filter_map(shown).collect::<Vec<_>>(), [0]);

    // Form1's click changes Form1's label, not Form2's
    session.send(DebugCommand::Call(0, "Command1_Click".into(), vec![]));
    let seen = until(&rx, |e| matches!(e, DebugEvent::Form(_)));
    let DebugEvent::Form(snap) = seen.last().unwrap() else {
        unreachable!()
    };
    assert_eq!(snap.form_index, 0);
    assert_eq!(snap.form.controls[0].caption, "clicked one");

    // Form2 isn't loaded, so a click addressed to it is ignored
    session.send(DebugCommand::Call(1, "Command1_Click".into(), vec![]));
    session.send(DebugCommand::Stop);
    let rest = until(&rx, |e| matches!(e, DebugEvent::Finished(_)));
    assert!(!rest.iter().any(|e| matches!(e, DebugEvent::Form(_))));
}

#[test]
fn show_opens_a_second_window_and_load_runs_its_form_load_first() {
    let docs = [
        doc(
            "Form1",
            form("Form1", "one"),
            "Sub Command1_Click()\n  Form2.Show\nEnd Sub",
        ),
        doc(
            "Form2",
            form("Form2", "two"),
            "Sub Form_Load()\n  Label1.Caption = \"loaded\"\nEnd Sub",
        ),
    ];
    let (session, rx) = start(&docs);
    until(&rx, |e| shown(e) == Some(0));

    session.send(DebugCommand::Call(0, "Command1_Click".into(), vec![]));
    let seen = until(&rx, |e| shown(e) == Some(1));
    let DebugEvent::FormShown(snap) = seen.last().unwrap() else {
        unreachable!()
    };
    assert_eq!(
        snap.form.controls[0].caption, "loaded",
        "Form_Load ran before the window opened"
    );
    session.send(DebugCommand::Stop);
}

#[test]
fn hide_closes_the_window_but_show_brings_back_the_same_state() {
    let docs = [
        doc(
            "Form1",
            form("Form1", "one"),
            "Sub Command1_Click()\n  Label1.Caption = \"kept\"\n  Me.Hide\n  Me.Show\nEnd Sub",
        ),
        doc("Form2", form("Form2", "two"), ""),
    ];
    let (session, rx) = start(&docs);
    until(&rx, |e| shown(e) == Some(0));
    session.send(DebugCommand::Call(0, "Command1_Click".into(), vec![]));
    let seen = until(&rx, |e| shown(e) == Some(0));
    assert!(seen.iter().any(|e| matches!(e, DebugEvent::FormHidden(0))));
    let DebugEvent::FormShown(snap) = seen.last().unwrap() else {
        unreachable!()
    };
    assert_eq!(snap.form.controls[0].caption, "kept");
    session.send(DebugCommand::Stop);
}

#[test]
fn unload_me_closes_the_window_and_the_last_unload_ends_the_program() {
    let docs = [doc(
        "Form1",
        form("Form1", "one"),
        "Sub Command1_Click()\n  Unload Me\nEnd Sub",
    )];
    let (session, rx) = start(&docs);
    until(&rx, |e| shown(e) == Some(0));
    session.send(DebugCommand::Call(0, "Command1_Click".into(), vec![]));
    let seen = until(&rx, |e| matches!(e, DebugEvent::Finished(_)));
    assert!(seen.iter().any(|e| matches!(e, DebugEvent::FormClosed(0))));
}

#[test]
fn closing_a_window_unloads_that_form_only() {
    let docs = [
        doc(
            "Form1",
            form("Form1", "one"),
            "Sub Command1_Click()\n  Form2.Show\nEnd Sub",
        ),
        doc("Form2", form("Form2", "two"), ""),
    ];
    let (session, rx) = start(&docs);
    until(&rx, |e| shown(e) == Some(0));
    session.send(DebugCommand::Call(0, "Command1_Click".into(), vec![]));
    until(&rx, |e| shown(e) == Some(1));

    // the user closes Form2: Form1 carries on, and can show it again
    session.send(DebugCommand::Closed(1));
    session.send(DebugCommand::Call(0, "Command1_Click".into(), vec![]));
    until(&rx, |e| shown(e) == Some(1));

    // closing the last loaded forms ends the program
    session.send(DebugCommand::Closed(1));
    session.send(DebugCommand::Closed(0));
    let seen = until(&rx, |e| matches!(e, DebugEvent::Finished(_)));
    assert!(matches!(seen.last(), Some(DebugEvent::Finished(_))));
}

#[test]
fn end_finishes_the_program_from_any_handler() {
    let docs = [doc(
        "Form1",
        form("Form1", "one"),
        "Sub Helper()\n  Print \"bye\"\n  End\nEnd Sub\n\
         Sub Command1_Click()\n  Helper\n  Print \"never\"\nEnd Sub",
    )];
    let (session, rx) = start(&docs);
    until(&rx, |e| shown(e) == Some(0));
    session.send(DebugCommand::Call(0, "Command1_Click".into(), vec![]));
    let seen = until(&rx, |e| matches!(e, DebugEvent::Finished(_)));
    let out = output(&seen);
    assert!(out.contains("bye"), "{}", out);
    assert!(!out.contains("never"), "{}", out);
    let Some(DebugEvent::Finished(msg)) = seen.last() else {
        unreachable!()
    };
    assert_eq!(msg, "Program ended");
}

#[test]
fn a_module_function_is_shared_and_each_forms_variables_stay_its_own() {
    let mut module = Doc::new_module("Module1");
    module.lines = to_lines("Function Twice(n As Long) As Long\n  Twice = n * 2\nEnd Function");
    let docs = [
        doc(
            "Form1",
            form("Form1", "one"),
            "Dim Count As Long\nSub Command1_Click()\n  Count = Count + 1\n  \
             Label1.Caption = Twice(Count)\nEnd Sub",
        ),
        doc(
            "Form2",
            form("Form2", "two"),
            "Dim Count As Long\nSub Command1_Click()\n  Count = Count + 10\n  \
             Label1.Caption = Twice(Count)\nEnd Sub",
        ),
        module,
    ];
    let (session, rx) = start(&docs);
    until(&rx, |e| shown(e) == Some(0));
    let caption = |form: usize| -> String {
        session.send(DebugCommand::Call(form, "Command1_Click".into(), vec![]));
        let seen = until(&rx, |e| matches!(e, DebugEvent::Form(_)));
        let DebugEvent::Form(s) = seen.last().unwrap() else {
            unreachable!()
        };
        s.form.controls[0].caption.clone()
    };
    assert_eq!(caption(0), "2", "Form1's Count is 1");
    assert_eq!(caption(0), "4", "and 2");
    session.send(DebugCommand::Stop);
}
