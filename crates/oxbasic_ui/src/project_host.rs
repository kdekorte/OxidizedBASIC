//! A project's forms as one object host: every form's controls, with the
//! form whose code is running (the *context*) deciding what a bare
//! `Label1` means, and the form commands `Form2.Show`, `Form2.Hide`,
//! `Unload Me` and `Load Form2`.
//!
//! Each form is a [`FormHost`]. A form's code is written in its own
//! `Sub Command1_Click()`s; to keep two forms' handlers apart the IDE
//! renames them to `Form1__Command1_Click` before the program is parsed,
//! and [`handler_for`] / [`split_handler`] map between the two spellings.

use crate::host::{FormHost, FormSnapshot, UserEdit};
use crate::{ControlSpec, FormSpec};
use oxbasic_runtime::{ExecResult, ObjectHost, ObjectRef, RaisedEvent, Variant, VbError};

/// The separator between a form's name and its handler in the combined
/// program: `Form2__Command1_Click`.
pub const FORM_SEP: &str = "__";

/// The combined-program name of `handler` (`Command1_Click`) in `form`.
pub fn handler_for(form: &str, handler: &str) -> String {
    format!("{}{}{}", form, FORM_SEP, handler)
}

/// `(form, handler)` of a combined-program name, if it is one.
pub fn split_handler(name: &str) -> Option<(&str, &str)> {
    name.split_once(FORM_SEP)
}

/// What the project's forms asked the IDE to do since it last looked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormCommand {
    /// The form was loaded (first use, `Load`, or the startup form): run
    /// its `Form_Load` handler.
    Load(usize),
    /// Open the form's window (`Form2.Show`).
    Show(usize),
    /// Hide the form's window but keep its state (`Form2.Hide`).
    Hide(usize),
    /// Close the form and drop its state (`Unload Form2`).
    Unload(usize),
}

/// Every form of the running project.
pub struct ProjectHost {
    /// The layouts as designed, to start a form afresh after `Unload`.
    specs: Vec<FormSpec>,
    forms: Vec<FormHost>,
    /// The form whose code is running; a bare control name means its own.
    context: usize,
    /// Forms that are loaded (`Form_Load` ran, state is live).
    loaded: Vec<bool>,
    /// Forms whose window is open.
    visible: Vec<bool>,
    /// Commands for the IDE, in the order the program gave them.
    commands: Vec<FormCommand>,
}

fn err<T>(number: i32, description: &str) -> ExecResult<T> {
    Err(VbError {
        number,
        description: description.to_string(),
    })
}

impl ProjectHost {
    /// A host for `forms`, with form 0 as the context.
    pub fn new(forms: Vec<FormSpec>) -> ProjectHost {
        let n = forms.len();
        ProjectHost {
            specs: forms.clone(),
            forms: forms.into_iter().map(FormHost::new).collect(),
            context: 0,
            loaded: vec![false; n],
            visible: vec![false; n],
            commands: Vec::new(),
        }
    }

    pub fn form_count(&self) -> usize {
        self.forms.len()
    }

    /// The index of the form called `name`.
    pub fn form_index(&self, name: &str) -> Option<usize> {
        self.forms
            .iter()
            .position(|f| f.name().eq_ignore_ascii_case(name))
    }

    /// The form's name.
    pub fn form_name(&self, index: usize) -> Option<&str> {
        self.forms.get(index).map(|f| f.name())
    }

    /// Load form `index` if it isn't: its `Form_Load` is due.
    pub fn load(&mut self, index: usize) {
        if index < self.forms.len() && !self.loaded[index] {
            self.loaded[index] = true;
            self.commands.push(FormCommand::Load(index));
        }
    }

    /// `Form.Show`: load the form if needed and open its window.
    pub fn show(&mut self, index: usize) {
        self.load(index);
        if index < self.forms.len() && !self.visible[index] {
            self.visible[index] = true;
            self.commands.push(FormCommand::Show(index));
        }
    }

    /// `Form.Hide`: close the window, keep the state.
    pub fn hide(&mut self, index: usize) {
        if index < self.forms.len() && self.visible[index] {
            self.visible[index] = false;
            self.commands.push(FormCommand::Hide(index));
        }
    }

    /// `Unload Form`: close the window and forget everything the program
    /// did to the form; showing it again starts from its design.
    pub fn unload(&mut self, index: usize) {
        if index < self.forms.len() && self.loaded[index] {
            self.forms[index] = FormHost::new(self.specs[index].clone());
            self.loaded[index] = false;
            self.visible[index] = false;
            self.commands.push(FormCommand::Unload(index));
        }
    }

    /// Whether any form is still loaded. A VB program ends when the last
    /// form is unloaded.
    pub fn any_loaded(&self) -> bool {
        self.loaded.iter().any(|l| *l)
    }

    pub fn is_loaded(&self, index: usize) -> bool {
        self.loaded.get(index).copied().unwrap_or(false)
    }

    /// Count form `index` as loaded without running its `Form_Load` (a
    /// single-form program whose caller runs the handler itself).
    pub fn mark_loaded(&mut self, index: usize) {
        if let Some(l) = self.loaded.get_mut(index) {
            *l = true;
        }
    }
}

impl ProjectHost {
    /// Make form `index` the context (its code is about to run).
    pub fn set_context_index(&mut self, index: usize) {
        if index < self.forms.len() {
            self.context = index;
        }
    }

    /// The program's copy of control `control` of form `form`.
    pub fn control_spec(&self, form: usize, control: usize) -> Option<&ControlSpec> {
        self.forms.get(form)?.spec().controls.get(control)
    }

    /// Whether form `form` sees keys before its controls (`KeyPreview`).
    pub fn key_preview(&self, form: usize) -> bool {
        self.forms.get(form).is_some_and(|f| f.spec().key_preview)
    }

    /// The commands given since the last call.
    pub fn take_commands(&mut self) -> Vec<FormCommand> {
        std::mem::take(&mut self.commands)
    }

    /// Each loaded form's changes since the last call, with its index.
    pub fn take_changes(&mut self) -> Vec<FormSnapshot> {
        let mut out = Vec::new();
        for (i, f) in self.forms.iter_mut().enumerate() {
            if let Some(mut snap) = f.take_changes() {
                snap.form_index = i;
                out.push(snap);
            }
        }
        out
    }

    /// Form `index` as it stands now (to open its window).
    pub fn snapshot(&self, index: usize) -> Option<FormSnapshot> {
        let mut snap = self.forms.get(index)?.snapshot();
        snap.form_index = index;
        Some(snap)
    }

    /// Apply what the user did in form `index`'s window.
    pub fn apply_edit(&mut self, index: usize, edit: UserEdit) {
        if let Some(f) = self.forms.get_mut(index) {
            f.apply_edit(edit);
        }
    }

    /// The timers of form `index`: `(control, interval_ms, enabled)`.
    pub fn timers(&self, index: usize) -> Vec<(usize, u64, bool)> {
        self.forms
            .get(index)
            .map(|f| f.timers())
            .unwrap_or_default()
    }

    /// The form a name in `object` refers to, for a form command: `Me` is
    /// the context, otherwise a form of the project.
    fn form_of(&self, name: &str) -> Option<usize> {
        if name.eq_ignore_ascii_case("me") {
            (self.context < self.forms.len()).then_some(self.context)
        } else {
            self.form_index(name)
        }
    }

    /// Where `Name.Member` points: a form (the context's own `Me`, or any
    /// form by name) or a control, which is the context form's unless the
    /// program wrote it as `Form2.Label1`.
    fn host_for(&mut self, object: &ObjectRef) -> ExecResult<usize> {
        // `Form2.Label1.Caption`: the interpreter passes the dotted tail
        // as the property, so the object here is just the form
        if let Some(i) = self.form_of(&object.name) {
            return Ok(i);
        }
        if self
            .forms
            .get(self.context)
            .is_some_and(|f| f.has_control(&object.name))
        {
            return Ok(self.context);
        }
        // a control of another form, named without its form: VB wouldn't
        // find it either
        err(424, "object required")
    }
}

impl ProjectHost {
    /// Work out which form, control and property a program's `object` and
    /// `property` mean. `Form2.Label1.Caption` arrives as the object
    /// `Form2` with the dotted property `Label1.Caption`.
    fn resolve(
        &mut self,
        object: &ObjectRef,
        property: &str,
    ) -> ExecResult<(usize, ObjectRef, String)> {
        if object.index.is_none() {
            if let Some(form) = self.form_of(&object.name) {
                if form != self.context {
                    // using another form loads it, as in VB
                    self.load(form);
                }
                if let Some((first, rest)) = property.split_once('.') {
                    if self.forms[form].has_control(first) {
                        let control = ObjectRef {
                            name: first.to_string(),
                            index: None,
                        };
                        return Ok((form, control, rest.to_string()));
                    }
                }
                let me = ObjectRef {
                    name: self.forms[form].name().to_string(),
                    index: None,
                };
                return Ok((form, me, property.to_string()));
            }
        }
        let form = self.host_for(object)?;
        Ok((form, object.clone(), property.to_string()))
    }
}

impl ObjectHost for ProjectHost {
    fn has_object(&self, name: &str) -> bool {
        self.form_of(name).is_some()
            || self
                .forms
                .get(self.context)
                .is_some_and(|f| f.has_control(name))
    }

    fn get(&mut self, object: &ObjectRef, property: &str, args: &[Variant]) -> ExecResult<Variant> {
        let (form, target, prop) = self.resolve(object, property)?;
        self.forms[form].get(&target, &prop, args)
    }

    fn set(
        &mut self,
        object: &ObjectRef,
        property: &str,
        args: &[Variant],
        value: Variant,
    ) -> ExecResult<()> {
        let (form, target, prop) = self.resolve(object, property)?;
        self.forms[form].set(&target, &prop, args, value)
    }

    fn call(&mut self, object: &ObjectRef, method: &str, args: &[Variant]) -> ExecResult<Variant> {
        // form commands: `Form2.Show`, `Me.Hide`, `Unload Me`, `Load Form2`
        if object.index.is_none() && !method.contains('.') {
            if let Some(form) = self.form_of(&object.name) {
                match method.to_ascii_lowercase().as_str() {
                    "show" => {
                        self.show(form);
                        return Ok(Variant::Empty);
                    }
                    "hide" => {
                        self.hide(form);
                        return Ok(Variant::Empty);
                    }
                    "unload" => {
                        self.unload(form);
                        return Ok(Variant::Empty);
                    }
                    "load" => {
                        self.load(form);
                        return Ok(Variant::Empty);
                    }
                    _ => {}
                }
            }
        }
        let (form, target, m) = self.resolve(object, method)?;
        self.forms[form].call(&target, &m, args)
    }

    fn context(&self) -> String {
        self.forms
            .get(self.context)
            .map(|f| f.name().to_string())
            .unwrap_or_default()
    }

    fn set_context(&mut self, ctx: &str) {
        if let Some(i) = self.form_index(ctx) {
            self.context = i;
        }
    }

    fn take_raised(&mut self) -> Vec<RaisedEvent> {
        self.forms
            .iter_mut()
            .flat_map(|f| f.take_raised())
            .collect()
    }

    fn unloading_form(&self, object: &ObjectRef) -> Option<String> {
        if object.index.is_some() {
            return None;
        }
        let i = self.form_of(&object.name)?;
        self.is_loaded(i).then(|| self.forms[i].name().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ControlSpec, Rect};
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Buf(String);

    impl oxbasic_runtime::Output for Buf {
        fn write(&mut self, s: &str) {
            self.0.push_str(s);
        }
    }

    /// Form1 (Label1 "one", Text1) and Form2 (Label1 "two", Label2).
    fn forms() -> Vec<FormSpec> {
        let mut a = FormSpec::new("Form1", "First", 300.0, 200.0);
        a.label("Label1", "one", Rect::new(0.0, 0.0, 100.0, 20.0));
        a.text_box("Text1", "", Rect::new(0.0, 30.0, 100.0, 24.0));
        let mut b = FormSpec::new("Form2", "Second", 300.0, 200.0);
        b.label("Label1", "two", Rect::new(0.0, 0.0, 100.0, 20.0));
        b.control(ControlSpec::new(
            "Label2",
            crate::ControlKind::Label,
            Rect::new(0.0, 30.0, 100.0, 20.0),
        ));
        vec![a, b]
    }

    fn host() -> Rc<RefCell<ProjectHost>> {
        Rc::new(RefCell::new(ProjectHost::new(forms())))
    }

    fn run(src: &str, host: &Rc<RefCell<ProjectHost>>) -> Result<String, String> {
        let prog = oxbasic_parser::parse(src)?;
        let mut out = Buf(String::new());
        {
            let mut rt = oxbasic_runtime::Interpreter::new(&prog, &mut out);
            rt.set_object_host(host.clone());
            rt.run().map_err(|e| e.to_string())?;
        }
        Ok(out.0)
    }

    #[test]
    fn handler_names_round_trip() {
        assert_eq!(
            handler_for("Form2", "Command1_Click"),
            "Form2__Command1_Click"
        );
        assert_eq!(
            split_handler("Form2__Command1_Click"),
            Some(("Form2", "Command1_Click"))
        );
        assert_eq!(split_handler("Command1_Click"), None);
    }

    #[test]
    fn a_bare_control_name_means_the_running_forms_control() {
        let h = host();
        let out = run("Print Label1.Caption\n", &h).unwrap();
        assert_eq!(out, "one\n", "context starts as Form1");
        h.borrow_mut().set_context_index(1);
        assert_eq!(run("Print Label1.Caption\n", &h).unwrap(), "two\n");
        // Form1's Text1 isn't visible from Form2's code
        let e = run("Print Text1.Text\n", &h).unwrap_err();
        assert!(e.contains("35") || e.contains("424"), "{}", e);
    }

    #[test]
    fn another_forms_controls_are_reached_through_its_name() {
        let h = host();
        let out = run(
            "Form2.Label1.Caption = \"changed\"\nPrint Form2.Label1.Caption\nPrint Label1.Caption\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "changed\none\n");
        let snaps = h.borrow_mut().take_changes();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].form_index, 1, "the change is Form2's");
        assert_eq!(snaps[0].form.controls[0].caption, "changed");
    }

    #[test]
    fn form_properties_through_me_and_a_name() {
        let h = host();
        run("Me.Caption = \"A\"\nForm2.Caption = \"B\"\n", &h).unwrap();
        let a = h.borrow().snapshot(0).unwrap();
        let b = h.borrow().snapshot(1).unwrap();
        assert_eq!(
            (a.form.caption.as_str(), b.form.caption.as_str()),
            ("A", "B")
        );
    }

    #[test]
    fn show_hide_and_unload_issue_commands_in_order() {
        let h = host();
        run("Form2.Show\nForm2.Hide\nForm2.Show\nUnload Form2\n", &h).unwrap();
        let cmds = h.borrow_mut().take_commands();
        assert_eq!(
            cmds,
            [
                FormCommand::Load(1),
                FormCommand::Show(1),
                FormCommand::Hide(1),
                FormCommand::Show(1),
                FormCommand::Unload(1),
            ]
        );
        assert!(!h.borrow().is_loaded(1));
    }

    #[test]
    fn unload_me_and_load_use_the_context() {
        let h = host();
        h.borrow_mut().set_context_index(1);
        // Form1 is loaded; Form2 (the context) was never loaded, so `Unload
        // Me` has nothing to unload
        run("Load Form1\nUnload Me\n", &h).unwrap();
        assert_eq!(h.borrow_mut().take_commands(), vec![FormCommand::Load(0)]);

        // once Form2 is loaded, `Unload Me` unloads it
        run("Load Form2\nUnload Me\n", &h).unwrap();
        assert_eq!(
            h.borrow_mut().take_commands(),
            vec![FormCommand::Load(1), FormCommand::Unload(1)]
        );
    }

    #[test]
    fn unloading_forgets_what_the_program_did() {
        let h = host();
        run(
            "Form2.Show\nForm2.Label1.Caption = \"edited\"\nUnload Form2\n",
            &h,
        )
        .unwrap();
        run("Form2.Show\nPrint Form2.Label1.Caption\n", &h).unwrap();
        let out = run("Print Form2.Label1.Caption\n", &h).unwrap();
        assert_eq!(out, "two\n", "a reloaded form starts from its design");
    }

    #[test]
    fn the_program_ends_when_the_last_form_is_unloaded() {
        let h = host();
        run("Form1.Show\nForm2.Show\n", &h).unwrap();
        assert!(h.borrow().any_loaded());
        run("Unload Form1\n", &h).unwrap();
        assert!(h.borrow().any_loaded(), "Form2 is still loaded");
        run("Unload Form2\n", &h).unwrap();
        assert!(!h.borrow().any_loaded());
    }

    #[test]
    fn end_stops_the_program_wherever_it_runs() {
        let h = host();
        let out = run(
            "Sub Quit()\n  Print \"in\"\n  End\nEnd Sub\nQuit\nPrint \"after\"\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "in\n");
    }

    #[test]
    fn a_handler_runs_in_its_own_forms_context() {
        let h = host();
        // Form2's handler, renamed as the IDE does, sets *its* Label1
        let out = run(
            "Sub Form2__Command1_Click()\n  Label1.Caption = \"from form2\"\nEnd Sub\n\
             Form2__Command1_Click\n\
             Print Label1.Caption\nPrint Form2.Label1.Caption\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "one\nfrom form2\n", "Form1's Label1 is untouched");
        assert_eq!(h.borrow().context(), "Form1", "the context is restored");
    }
}
