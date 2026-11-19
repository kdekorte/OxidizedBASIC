//! The live form as a BASIC program sees it: `Label1.Caption = "x"`,
//! `Text1.Text`, `List1.AddItem "a"`, `Combo1.ListIndex = 2`.
//!
//! [`FormHost`] implements the interpreter's [`ObjectHost`] over a form
//! model. The program's changes are published as [`FormSnapshot`]s for
//! the window that shows the form, and the user's edits come back as
//! [`UserEdit`]s so the program reads what the user typed or picked.
//!
//! No GUI is needed here, so this module is usable and tested headless.

use crate::{ControlKind, ControlSpec, FormSpec};
use oxbasic_runtime::{ExecResult, ObjectHost, Variant, VbError};

/// A control's run-time state: the text of a TextBox or ComboBox, and the
/// selected row of a list.
#[derive(Clone, Debug, Default)]
pub struct ControlState {
    pub text: String,
    pub list_index: Option<usize>,
    pub dropped: bool,
}

impl ControlState {
    pub fn new(c: &ControlSpec) -> ControlState {
        ControlState {
            text: match c.kind {
                ControlKind::TextBox | ControlKind::ComboBox => c.text().to_string(),
                _ => String::new(),
            },
            list_index: None,
            dropped: false,
        }
    }

    /// The user picked item `i` of the control's list.
    pub fn pick(&mut self, c: &ControlSpec, i: usize) {
        self.list_index = Some(i);
        self.dropped = false;
        if c.kind == ControlKind::ComboBox {
            if let Some(item) = c.items.get(i) {
                self.text = item.clone();
            }
        }
    }
}

/// The whole live form at one moment: what the window should show.
#[derive(Clone, Debug)]
pub struct FormSnapshot {
    pub form: FormSpec,
    /// Per control, in `form.controls` order.
    pub state: Vec<ControlState>,
    /// Per control: the program changed its `state` (text, selection).
    /// The window keeps its own state for the others, so keystrokes typed
    /// while this snapshot was in flight are not overwritten.
    pub touched: Vec<bool>,
}

impl FormSnapshot {
    /// Bring a window's copy of the form up to date: take the form as the
    /// program has it, and the text and selection of the controls the
    /// program touched. The other controls keep the window's own state, so
    /// keystrokes typed while this snapshot was in flight survive, and so
    /// does the window's own open-list flag.
    pub fn apply_to(&self, form: &mut FormSpec, state: &mut Vec<ControlState>) {
        *form = self.form.clone();
        state.resize_with(form.controls.len(), ControlState::default);
        for (i, mine) in self.state.iter().enumerate() {
            if self.touched.get(i).copied().unwrap_or(false) {
                state[i].text = mine.text.clone();
                state[i].list_index = mine.list_index;
            }
        }
    }
}

/// Something the user did in the live window that the program must see.
#[derive(Clone, Debug)]
pub enum UserEdit {
    /// The field's whole text after a keystroke.
    Text { control: usize, text: String },
    /// The user picked row `index` of a list or combo box.
    Pick { control: usize, index: usize },
}

/// Where a `Name.Member` points.
enum Target {
    Form,
    Control(usize),
}

/// The live form, owned by the program's side. The interpreter reads and
/// writes its objects through [`ObjectHost`].
pub struct FormHost {
    form: FormSpec,
    state: Vec<ControlState>,
    /// Something changed since the last [`FormHost::take_changes`].
    changed: bool,
    /// Controls whose state the program changed since then.
    touched: Vec<bool>,
}

impl FormHost {
    pub fn new(form: FormSpec) -> FormHost {
        let state = form.controls.iter().map(ControlState::new).collect();
        let touched = vec![false; form.controls.len()];
        FormHost {
            form,
            state,
            changed: false,
            touched,
        }
    }

    /// The form as it stands now, every control's state included.
    pub fn snapshot(&self) -> FormSnapshot {
        FormSnapshot {
            form: self.form.clone(),
            state: self.state.clone(),
            touched: vec![true; self.form.controls.len()],
        }
    }

    /// What the program changed since the last call, else `None`.
    pub fn take_changes(&mut self) -> Option<FormSnapshot> {
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        let n = self.form.controls.len();
        let touched = std::mem::replace(&mut self.touched, vec![false; n]);
        Some(FormSnapshot {
            form: self.form.clone(),
            state: self.state.clone(),
            touched,
        })
    }

    /// Apply something the user did in the window. The window already shows
    /// it, so it is not published back.
    pub fn apply_edit(&mut self, edit: UserEdit) {
        match edit {
            UserEdit::Text { control, text } => {
                if let Some(s) = self.state.get_mut(control) {
                    s.text = text;
                    s.list_index = None;
                }
            }
            UserEdit::Pick { control, index } => {
                if let Some(spec) = self.form.controls.get(control).cloned() {
                    self.state[control].pick(&spec, index);
                }
            }
        }
    }

    fn control_index(&self, name: &str) -> Option<usize> {
        self.form
            .controls
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name))
    }

    fn is_form(&self, object: &str) -> bool {
        object.eq_ignore_ascii_case("me") || object.eq_ignore_ascii_case(&self.form.name)
    }

    fn target(&self, object: &str) -> ExecResult<Target> {
        if self.is_form(object) {
            return Ok(Target::Form);
        }
        match self.control_index(object) {
            Some(i) => Ok(Target::Control(i)),
            None => err(424, "object required"),
        }
    }
}
impl FormHost {
    fn get_control(&self, i: usize, p: &str, args: &[Variant]) -> ExecResult<Variant> {
        let c = &self.form.controls[i];
        let s = &self.state[i];
        let is_list = matches!(c.kind, ControlKind::ListBox | ControlKind::ComboBox);
        Ok(match p {
            "caption" if c.kind.has_caption() => Variant::String(c.caption.clone()),
            "text" if c.kind.has_text() => Variant::String(s.text.clone()),
            // a ListBox's Text is its selected row
            "text" if c.kind == ControlKind::ListBox => Variant::String(
                s.list_index
                    .and_then(|li| c.items.get(li))
                    .cloned()
                    .unwrap_or_default(),
            ),
            "left" => Variant::Single(c.rect.left),
            "top" => Variant::Single(c.rect.top),
            "width" => Variant::Single(c.rect.width),
            "height" => Variant::Single(c.rect.height),
            "visible" => Variant::Boolean(c.visible),
            "enabled" => Variant::Boolean(c.enabled),
            "interval" if c.kind == ControlKind::Timer => Variant::Long(c.interval_ms as i32),
            "style" if c.kind == ControlKind::ComboBox => Variant::Integer(c.style as i16),
            "listcount" if is_list => Variant::Long(c.items.len() as i32),
            "listindex" if is_list => Variant::Long(s.list_index.map_or(-1, |li| li as i32)),
            "list" if is_list => {
                let n = index_arg(args)?;
                match c.items.get(n) {
                    Some(item) => Variant::String(item.clone()),
                    None => return err(381, "Invalid property array index"),
                }
            }
            _ => return unsupported(),
        })
    }

    fn set_form(&mut self, p: &str, value: &Variant) -> ExecResult<()> {
        match p {
            "caption" => self.form.caption = value.as_string()?,
            "width" | "height" => {
                let v = coord(value)?;
                if v <= 0.0 {
                    return err(380, "Invalid property value");
                }
                if p == "width" {
                    self.form.width = v;
                } else {
                    self.form.height = v;
                }
            }
            _ => return unsupported(),
        }
        Ok(())
    }

    fn set_control(
        &mut self,
        i: usize,
        p: &str,
        args: &[Variant],
        value: &Variant,
    ) -> ExecResult<()> {
        let kind = self.form.controls[i].kind;
        let is_list = matches!(kind, ControlKind::ListBox | ControlKind::ComboBox);
        match p {
            "caption" if kind.has_caption() => {
                self.form.controls[i].caption = value.as_string()?;
            }
            "text" if kind.has_text() => self.state[i].text = value.as_string()?,
            "text" if kind == ControlKind::ListBox => return err(382, "Property is read-only"),
            "left" => self.form.controls[i].rect.left = coord(value)?,
            "top" => self.form.controls[i].rect.top = coord(value)?,
            "width" | "height" => {
                let v = coord(value)?;
                if v <= 0.0 {
                    return err(380, "Invalid property value");
                }
                if p == "width" {
                    self.form.controls[i].rect.width = v;
                } else {
                    self.form.controls[i].rect.height = v;
                }
            }
            "visible" => self.form.controls[i].visible = value.as_bool()?,
            "enabled" => self.form.controls[i].enabled = value.as_bool()?,
            "interval" if kind == ControlKind::Timer => {
                let n = value.as_i64()?;
                if n < 0 {
                    return err(380, "Invalid property value");
                }
                self.form.controls[i].interval_ms = n as u64;
            }
            "style" if kind == ControlKind::ComboBox => {
                let n = value.as_i64()?;
                if !(0..=2).contains(&n) {
                    return err(380, "Invalid property value");
                }
                self.form.controls[i].style = n as u8;
            }
            "listindex" if is_list => {
                let n = value.as_i64()?;
                if n == -1 {
                    self.state[i].list_index = None;
                } else if n >= 0 && (n as usize) < self.form.controls[i].items.len() {
                    let spec = self.form.controls[i].clone();
                    self.state[i].pick(&spec, n as usize);
                } else {
                    return err(381, "Invalid property array index");
                }
            }
            "list" if is_list => {
                let n = index_arg(args)?;
                if n >= self.form.controls[i].items.len() {
                    return err(381, "Invalid property array index");
                }
                self.form.controls[i].items[n] = value.as_string()?;
            }
            _ => return unsupported(),
        }
        Ok(())
    }
}

fn err<T>(number: i32, description: &str) -> ExecResult<T> {
    Err(VbError {
        number,
        description: description.to_string(),
    })
}

fn unsupported<T>() -> ExecResult<T> {
    err(438, "object doesn't support this property or method")
}

/// The first argument as a list index (`List1.List(0)`, `RemoveItem 2`).
fn index_arg(args: &[Variant]) -> ExecResult<usize> {
    let Some(v) = args.first() else {
        return err(450, "wrong number of arguments");
    };
    let n = v.as_i64()?;
    if n < 0 {
        return err(381, "Invalid property array index");
    }
    Ok(n as usize)
}

/// A size or position as a VB `Single`.
fn coord(value: &Variant) -> ExecResult<f32> {
    Ok(value.as_f64()? as f32)
}

impl ObjectHost for FormHost {
    fn has_object(&self, name: &str) -> bool {
        self.is_form(name) || self.control_index(name).is_some()
    }

    fn get(&mut self, object: &str, property: &str, args: &[Variant]) -> ExecResult<Variant> {
        let p = property.to_ascii_lowercase();
        match self.target(object)? {
            Target::Form => match p.as_str() {
                "caption" => Ok(Variant::String(self.form.caption.clone())),
                "width" => Ok(Variant::Single(self.form.width)),
                "height" => Ok(Variant::Single(self.form.height)),
                _ => unsupported(),
            },
            Target::Control(i) => self.get_control(i, &p, args),
        }
    }

    fn set(
        &mut self,
        object: &str,
        property: &str,
        args: &[Variant],
        value: Variant,
    ) -> ExecResult<()> {
        let p = property.to_ascii_lowercase();
        let target = self.target(object)?;
        let result = match target {
            Target::Form => self.set_form(&p, &value),
            Target::Control(i) => self.set_control(i, &p, args, &value),
        };
        if result.is_ok() {
            self.changed = true;
            if let Target::Control(i) = target {
                self.touched[i] = true;
            }
        }
        result
    }

    fn call(&mut self, object: &str, method: &str, args: &[Variant]) -> ExecResult<Variant> {
        let m = method.to_ascii_lowercase();
        // a parameterised property read, `List1.List(0)`, arrives as a call
        if let Target::Control(i) = self.target(object)? {
            if m == "list" || m == "listcount" || m == "listindex" || m == "text" {
                return self.get_control(i, &m, args);
            }
        }
        // only the list controls have methods; the form has none
        let i = match self.target(object)? {
            Target::Control(i)
                if matches!(
                    self.form.controls[i].kind,
                    ControlKind::ListBox | ControlKind::ComboBox
                ) =>
            {
                i
            }
            _ => return unsupported(),
        };
        let result = match m.as_str() {
            "additem" => {
                let text = match args.first() {
                    Some(v) => v.as_string()?,
                    None => return err(450, "wrong number of arguments"),
                };
                let len = self.form.controls[i].items.len();
                let at = match args.get(1) {
                    None => len,
                    Some(v) => match v.as_i64()? {
                        -1 => len,
                        n if n >= 0 && (n as usize) <= len => n as usize,
                        _ => return err(381, "Invalid property array index"),
                    },
                };
                self.form.controls[i].items.insert(at, text);
                if let Some(li) = self.state[i].list_index {
                    if li >= at {
                        self.state[i].list_index = Some(li + 1);
                    }
                }
                Ok(Variant::Empty)
            }
            "removeitem" => {
                let n = index_arg(args)?;
                if n >= self.form.controls[i].items.len() {
                    return err(381, "Invalid property array index");
                }
                self.form.controls[i].items.remove(n);
                self.state[i].list_index = match self.state[i].list_index {
                    Some(li) if li == n => None,
                    Some(li) if li > n => Some(li - 1),
                    other => other,
                };
                Ok(Variant::Empty)
            }
            "clear" => {
                self.form.controls[i].items.clear();
                self.state[i].list_index = None;
                Ok(Variant::Empty)
            }
            _ => unsupported(),
        };
        if result.is_ok() {
            self.changed = true;
            self.touched[i] = true;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rect;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Output sink for the test programs.
    struct Buf(String);

    impl oxbasic_runtime::Output for Buf {
        fn write(&mut self, s: &str) {
            self.0.push_str(s);
        }
    }

    /// Form1 with Label1, Text1, List1 (ListBox), Combo1 (ComboBox), Timer1.
    fn form() -> FormSpec {
        let mut f = FormSpec::new("Form1", "Sample", 400.0, 300.0);
        f.label("Label1", "Hello", Rect::new(0.0, 0.0, 100.0, 20.0));
        f.text_box("Text1", "start", Rect::new(0.0, 30.0, 100.0, 24.0));
        f.control(ControlSpec {
            name: "List1".into(),
            kind: ControlKind::ListBox,
            rect: Rect::new(0.0, 60.0, 100.0, 80.0),
            caption: String::new(),
            text: None,
            items: vec!["Apple".into(), "Cherry".into()],
            enabled: true,
            visible: true,
            interval_ms: 0,
            color: None,
            style: 0,
        });
        f.control(ControlSpec {
            name: "Combo1".into(),
            kind: ControlKind::ComboBox,
            rect: Rect::new(0.0, 150.0, 100.0, 24.0),
            caption: String::new(),
            text: Some(String::new()),
            items: vec!["Red".into(), "Green".into(), "Blue".into()],
            enabled: true,
            visible: true,
            interval_ms: 0,
            color: None,
            style: 0,
        });
        f.timer("Timer1", 250);
        f
    }

    fn host() -> Rc<RefCell<FormHost>> {
        Rc::new(RefCell::new(FormHost::new(form())))
    }

    /// Run `src` with `host` as its object host; returns the printed
    /// output, or the error text.
    fn run(src: &str, host: &Rc<RefCell<FormHost>>) -> Result<String, String> {
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
    fn program_sets_and_reads_captions_and_text() {
        let h = host();
        let out = run(
            "Label1.Caption = \"Clicks: 3\"\nPrint Label1.Caption\n\
             Text1.Text = \"typed\"\nPrint Text1.Text\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "Clicks: 3\ntyped\n");
        let snap = h.borrow().snapshot();
        assert_eq!(snap.form.controls[0].caption, "Clicks: 3");
        assert_eq!(snap.state[1].text, "typed");
    }

    #[test]
    fn list_methods_and_properties() {
        let h = host();
        let out = run(
            "List1.AddItem \"Banana\"\n\
             List1.AddItem \"Avocado\", 0\n\
             Print List1.ListCount\n\
             Print List1.List(0)\n\
             List1.RemoveItem 1\n\
             List1.ListIndex = 1\n\
             Print List1.Text\n\
             List1.Clear\n\
             Print List1.ListCount; List1.ListIndex\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, " 4 \nAvocado\nCherry\n 0  -1 \n");
        assert!(h.borrow().snapshot().form.controls[2].items.is_empty());
    }

    #[test]
    fn combo_list_index_sets_its_text() {
        let h = host();
        let out = run("Combo1.ListIndex = 1\nPrint Combo1.Text\n", &h).unwrap();
        assert_eq!(out, "Green\n");
        let snap = h.borrow().snapshot();
        assert_eq!(snap.state[3].text, "Green");
        assert_eq!(snap.state[3].list_index, Some(1));
    }

    #[test]
    fn form_caption_and_geometry_through_me_and_its_name() {
        let h = host();
        let out = run(
            "Me.Caption = \"New title\"\nForm1.Width = 500\nPrint Form1.Caption\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "New title\n");
        assert_eq!(h.borrow().snapshot().form.width, 500.0);
    }

    #[test]
    fn unknown_members_keep_their_vb_errors() {
        let h = host();
        let e = run("Print Label1.Text\n", &h).unwrap_err();
        assert!(e.contains("438"), "{}", e);
        let e = run("Label1.AddItem \"x\"\n", &h).unwrap_err();
        assert!(e.contains("438"), "{}", e);
        let e = run("Print Nope.Caption\n", &h).unwrap_err();
        assert!(e.contains("424"), "{}", e);
        let e = run("List1.List(9) = \"x\"\n", &h).unwrap_err();
        assert!(e.contains("381"), "{}", e);
        let e = run("List1.Text = \"x\"\n", &h).unwrap_err();
        assert!(e.contains("382"), "{}", e);
        let e = run("Combo1.Style = 7\n", &h).unwrap_err();
        assert!(e.contains("380"), "{}", e);
    }

    #[test]
    fn a_variable_of_the_same_name_wins_over_the_control() {
        let h = host();
        let out = run("Dim Label1 As Long\nLabel1 = 4\nPrint Label1\n", &h).unwrap();
        assert_eq!(out, " 4 \n");
        assert_eq!(h.borrow().snapshot().form.controls[0].caption, "Hello");
    }

    #[test]
    fn changes_are_published_once() {
        let h = host();
        assert!(h.borrow_mut().take_changes().is_none());
        run("Label1.Caption = \"x\"\n", &h).unwrap();
        assert!(h.borrow_mut().take_changes().is_some());
        assert!(h.borrow_mut().take_changes().is_none());
        // a read changes nothing
        run("Print Label1.Caption\n", &h).unwrap();
        assert!(h.borrow_mut().take_changes().is_none());
    }

    #[test]
    fn only_controls_the_program_touched_are_flagged() {
        let h = host();
        run("Label1.Caption = \"x\"\nList1.AddItem \"Plum\"\n", &h).unwrap();
        let snap = h.borrow_mut().take_changes().unwrap();
        // Label1 and List1 were written; Text1 and Combo1 were not, so the
        // window keeps whatever the user typed there
        assert_eq!(snap.touched, vec![true, false, true, false, false]);
        // flags reset after being taken
        run("Text1.Text = \"y\"\n", &h).unwrap();
        let snap = h.borrow_mut().take_changes().unwrap();
        assert_eq!(snap.touched, vec![false, true, false, false, false]);
    }

    #[test]
    fn a_snapshot_updates_only_the_touched_controls_state() {
        let h = host();
        run("Label1.Caption = \"x\"\nCombo1.ListIndex = 0\n", &h).unwrap();
        let snap = h.borrow_mut().take_changes().unwrap();

        // the window's copy: the user has typed in Text1 meanwhile
        let mut form = form();
        let mut state: Vec<ControlState> = form.controls.iter().map(ControlState::new).collect();
        state[1].text = "typed meanwhile".into();
        state[3].dropped = true;
        snap.apply_to(&mut form, &mut state);

        assert_eq!(form.controls[0].caption, "x", "the form is the program's");
        assert_eq!(state[1].text, "typed meanwhile", "untouched: keeps typing");
        assert_eq!(state[3].text, "Red", "touched: the program's text");
        assert_eq!(state[3].list_index, Some(0));
        assert!(state[3].dropped, "the window's own list flag survives");
    }

    #[test]
    fn user_edits_are_visible_to_the_program() {
        let h = host();
        h.borrow_mut().apply_edit(UserEdit::Text {
            control: 1,
            text: "hi there".into(),
        });
        h.borrow_mut().apply_edit(UserEdit::Pick {
            control: 3,
            index: 2,
        });
        let out = run(
            "Print Text1.Text\nPrint Combo1.Text\nPrint List1.ListIndex\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "hi there\nBlue\n -1 \n");
        // the window already shows them: nothing is published back
        assert!(h.borrow_mut().take_changes().is_none());
    }
}
