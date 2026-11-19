//! Raising events from a host: handlers get their arguments by reference
//! (`KeyAscii = 0`, `Cancel = True`), may declare fewer parameters than
//! the event has, and property changes the program makes raise their
//! object's events inside the statement.

use oxbasic_runtime::{
    ExecResult, Interpreter, ObjectHost, ObjectRef, Output, RaisedEvent, Variant,
};
use std::cell::RefCell;
use std::rc::Rc;

struct Buf(Rc<RefCell<String>>);

impl Output for Buf {
    fn write(&mut self, s: &str) {
        self.0.borrow_mut().push_str(s);
    }
}

/// One object, `Text1`, whose `Text` raises `Text1_Change` when the
/// program sets it to a different value; `Form1` can be unloaded.
#[derive(Default)]
struct Host {
    text: String,
    raised: Vec<RaisedEvent>,
    unloaded: bool,
}

impl ObjectHost for Host {
    fn has_object(&self, name: &str) -> bool {
        name.eq_ignore_ascii_case("text1") || name.eq_ignore_ascii_case("form1")
    }
    fn get(&mut self, _o: &ObjectRef, _p: &str, _a: &[Variant]) -> ExecResult<Variant> {
        Ok(Variant::String(self.text.clone()))
    }
    fn set(&mut self, _o: &ObjectRef, _p: &str, _a: &[Variant], v: Variant) -> ExecResult<()> {
        let v = v.as_string()?;
        if v != self.text {
            self.text = v;
            self.raised.push(RaisedEvent {
                form: "Form1".into(),
                handler: "Text1_Change".into(),
                args: vec![],
            });
        }
        Ok(())
    }
    fn call(&mut self, _o: &ObjectRef, m: &str, _a: &[Variant]) -> ExecResult<Variant> {
        if m.eq_ignore_ascii_case("unload") {
            self.unloaded = true;
        }
        Ok(Variant::Empty)
    }
    fn take_raised(&mut self) -> Vec<RaisedEvent> {
        std::mem::take(&mut self.raised)
    }
    fn unloading_form(&self, o: &ObjectRef) -> Option<String> {
        o.name
            .eq_ignore_ascii_case("form1")
            .then(|| "Form1".to_string())
    }
}

/// Run `src`'s module code with the host installed, then `f` on the
/// interpreter; returns what was printed.
fn with_program(
    src: &str,
    f: impl FnOnce(&mut Interpreter, &Rc<RefCell<Host>>),
) -> (String, Rc<RefCell<Host>>) {
    let prog = oxbasic_parser::parse(src).expect("parse");
    let out = Rc::new(RefCell::new(String::new()));
    let mut buf = Buf(out.clone());
    let host = Rc::new(RefCell::new(Host::default()));
    {
        let mut rt = Interpreter::new(&prog, &mut buf);
        rt.set_object_host(host.clone());
        rt.run().expect("module code");
        f(&mut rt, &host);
    }
    let printed = out.borrow().clone();
    (printed, host)
}

#[test]
fn a_handler_changes_its_by_reference_arguments() {
    let src = "Sub Text1_KeyPress(KeyAscii As Integer)\n\
               \x20 If KeyAscii = 97 Then KeyAscii = 0 Else KeyAscii = KeyAscii - 32\n\
               End Sub\n";
    with_program(src, |rt, _| {
        let out = rt
            .call_event("Text1_KeyPress", &[Variant::Integer(97)])
            .unwrap();
        assert_eq!(out[0].as_i64().unwrap(), 0, "the key is swallowed");
        let out = rt
            .call_event("Text1_KeyPress", &[Variant::Integer(98)])
            .unwrap();
        assert_eq!(out[0].as_i64().unwrap(), 66, "and changed: b to B");
    });
}

#[test]
fn a_handler_may_declare_fewer_parameters_than_the_event_has() {
    // a placeholder written before the control joined an array
    let src = "Sub Command1_Click()\n  Print \"clicked\"\nEnd Sub\n";
    let (out, _) = with_program(src, |rt, _| {
        let args = rt
            .call_event("Command1_Click", &[Variant::Integer(2)])
            .unwrap();
        assert_eq!(args.len(), 1, "every event argument comes back");
    });
    assert_eq!(out, "clicked\n");
}

#[test]
fn setting_a_property_raises_its_event_inside_the_statement() {
    let src = "Sub Text1_Change()\n  Print \"change to \" & Text1.Text\nEnd Sub\n\
               Text1.Text = \"a\"\nPrint \"after\"\nText1.Text = \"a\"\nText1.Text = \"b\"\n";
    let (out, _) = with_program(src, |_, _| {});
    // the handler runs before the next statement; an unchanged value
    // raises nothing
    assert_eq!(out, "change to a\nafter\nchange to b\n");
}

#[test]
fn a_handler_that_changes_its_own_property_settles() {
    // VB recurses until the value stops changing: here, once
    let src = "Sub Text1_Change()\n  Text1.Text = UCase(Text1.Text)\n  Print Text1.Text\nEnd Sub\n\
               Text1.Text = \"hi\"\n";
    let (out, host) = with_program(src, |_, _| {});
    assert_eq!(host.borrow().text, "HI");
    assert_eq!(out, "HI\nHI\n", "the inner change, then the outer one");
}

#[test]
fn unload_asks_query_unload_and_unload_which_may_cancel() {
    let src = "Dim Allow As Boolean\n\
               Sub Form_QueryUnload(Cancel As Integer, UnloadMode As Integer)\n\
               \x20 Print \"query \" & UnloadMode\n\
               \x20 If Not Allow Then Cancel = True\n\
               End Sub\n\
               Sub Form_Unload(Cancel As Integer)\n  Print \"unload\"\nEnd Sub\n\
               Unload Form1\nPrint \"kept\"\nAllow = True\nUnload Form1\n";
    let (out, host) = with_program(src, |_, _| {});
    assert_eq!(out, "query 1\nkept\nquery 1\nunload\n");
    assert!(host.borrow().unloaded, "the second Unload went ahead");
}

#[test]
fn closing_the_window_asks_with_unload_mode_zero() {
    let src = "Sub Form_QueryUnload(Cancel As Integer, UnloadMode As Integer)\n\
               \x20 Print \"mode \" & UnloadMode\nEnd Sub\n";
    let (out, _) = with_program(src, |rt, _| {
        assert!(rt.confirm_unload("Form1", 0).unwrap());
    });
    assert_eq!(out, "mode 0\n");
}
