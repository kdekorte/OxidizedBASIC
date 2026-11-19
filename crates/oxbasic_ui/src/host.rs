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
use oxbasic_runtime::{ExecResult, ObjectHost, ObjectRef, Variant, VbError};

/// A control's run-time state: the text of a TextBox or ComboBox, the
/// selected row of a list, and a CheckBox's or OptionButton's `Value`.
#[derive(Clone, Debug, Default)]
pub struct ControlState {
    pub text: String,
    pub list_index: Option<usize>,
    pub dropped: bool,
    pub value: i32,
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
            value: c.value,
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
    /// Per control: the program changed its `state` (text, selection,
    /// value). The window keeps its own state for the others, so keystrokes
    /// typed while this snapshot was in flight are not overwritten.
    pub touched: Vec<bool>,
    /// The control the program gave the focus to (`Text1.SetFocus`), if it
    /// did since the last snapshot.
    pub focus: Option<usize>,
}

impl FormSnapshot {
    /// Bring a window's copy of the form up to date: take the form as the
    /// program has it, and the state of the controls the program touched.
    /// The other controls keep the window's own state, so keystrokes typed
    /// while this snapshot was in flight survive, and so does the window's
    /// own open-list flag.
    pub fn apply_to(&self, form: &mut FormSpec, state: &mut Vec<ControlState>) {
        *form = self.form.clone();
        state.resize_with(form.controls.len(), ControlState::default);
        for (i, mine) in self.state.iter().enumerate() {
            if self.touched.get(i).copied().unwrap_or(false) {
                state[i].text = mine.text.clone();
                state[i].list_index = mine.list_index;
                state[i].value = mine.value;
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
    /// The user clicked a CheckBox or OptionButton, which now has `value`
    /// (an OptionButton also clears the others of its group).
    Value { control: usize, value: i32 },
}

/// The OptionButtons that act as one group with `i`: VB groups them by
/// container, and a form has no containers here, so every OptionButton on
/// the form is one group.
pub fn option_group(form: &FormSpec, i: usize) -> Vec<usize> {
    if form.controls.get(i).map(|c| c.kind) != Some(ControlKind::OptionButton) {
        return vec![];
    }
    form.controls
        .iter()
        .enumerate()
        .filter(|(j, c)| *j != i && c.kind == ControlKind::OptionButton)
        .map(|(j, _)| j)
        .collect()
}

/// The new state of `form` after the user set control `i` to `value`
/// (a CheckBox or OptionButton): every control whose value changed.
pub fn set_value(form: &FormSpec, state: &mut [ControlState], i: usize, value: i32) -> Vec<usize> {
    let mut changed = vec![];
    if state[i].value != value {
        state[i].value = value;
        changed.push(i);
    }
    if value != 0 {
        for j in option_group(form, i) {
            if state[j].value != 0 {
                state[j].value = 0;
                changed.push(j);
            }
        }
    }
    changed
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
    /// `SetFocus` was called on this control since then.
    focus: Option<usize>,
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
            focus: None,
        }
    }

    /// The form as it stands now, every control's state included.
    pub fn snapshot(&self) -> FormSnapshot {
        FormSnapshot {
            form: self.form.clone(),
            state: self.state.clone(),
            touched: vec![true; self.form.controls.len()],
            focus: self.focus,
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
            focus: self.focus.take(),
        })
    }

    /// The interval of every Timer, in control order, as the program has
    /// set them: `(control, interval_ms, enabled)`.
    pub fn timers(&self) -> Vec<(usize, u64, bool)> {
        self.form
            .controls
            .iter()
            .enumerate()
            .filter(|(_, c)| c.kind == ControlKind::Timer)
            .map(|(i, c)| (i, c.interval_ms, c.enabled))
            .collect()
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
            UserEdit::Value { control, value } => {
                if control < self.state.len() {
                    set_value(&self.form, &mut self.state, control, value);
                }
            }
        }
    }

    /// The control a program name refers to: `Label1`, or element `index`
    /// of a control array (`Command1(2)`).
    fn control_index(&self, name: &str, index: Option<i32>) -> Option<usize> {
        self.form
            .controls
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name) && c.index == index)
    }

    /// Whether `name` is a control array (its controls have an `Index`).
    fn is_array(&self, name: &str) -> bool {
        self.form
            .controls
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(name) && c.index.is_some())
    }

    fn is_form(&self, object: &str) -> bool {
        object.eq_ignore_ascii_case("me") || object.eq_ignore_ascii_case(&self.form.name)
    }

    fn target(&self, object: &ObjectRef) -> ExecResult<Target> {
        if object.index.is_none() && self.is_form(&object.name) {
            return Ok(Target::Form);
        }
        match self.control_index(&object.name, object.index) {
            Some(i) => Ok(Target::Control(i)),
            // `Command1.Caption` on an array, or `Command1(9)` past its end
            None if self.is_array(&object.name) => match object.index {
                None => err(343, "Object not an array"),
                Some(_) => err(340, "Control array element does not exist"),
            },
            None if object.index.is_some() => err(343, "Object not an array"),
            None => err(424, "object required"),
        }
    }
}
impl FormHost {
    fn get_control(&self, i: usize, p: &str, args: &[Variant]) -> ExecResult<Variant> {
        let c = &self.form.controls[i];
        let s = &self.state[i];
        let is_list = matches!(c.kind, ControlKind::ListBox | ControlKind::ComboBox);
        if let Some(fp) = p.strip_prefix("font.") {
            return self.get_font(i, fp);
        }
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
            "name" => Variant::String(c.name.clone()),
            "index" => match c.index {
                Some(n) => Variant::Integer(n as i16),
                None => return err(343, "Object not an array"),
            },
            "left" => Variant::Single(c.rect.left),
            "top" => Variant::Single(c.rect.top),
            "width" => Variant::Single(c.rect.width),
            "height" => Variant::Single(c.rect.height),
            "visible" => Variant::Boolean(c.visible),
            "enabled" => Variant::Boolean(c.enabled),
            "interval" if c.kind == ControlKind::Timer => Variant::Long(c.interval_ms as i32),
            "style" if c.kind == ControlKind::ComboBox => Variant::Integer(c.style as i16),
            // a CheckBox's Value is 0/1/2; an OptionButton's is a Boolean
            "value" if c.kind == ControlKind::CheckBox => Variant::Integer(s.value as i16),
            "value" if c.kind == ControlKind::OptionButton => Variant::Boolean(s.value != 0),
            "backcolor" if c.kind != ControlKind::Timer => {
                Variant::Long(to_vb_color(c.color.unwrap_or(default_back(c.kind))))
            }
            "forecolor" if c.kind.has_font() => {
                Variant::Long(to_vb_color(c.fore_color.unwrap_or(DEFAULT_FORE)))
            }
            // `Font` / `FontName` etc., VB's shorthands for `Font.Name`
            "fontname" | "fontsize" | "fontbold" | "fontitalic" | "fontunderline"
            | "fontstrikethru" => return self.get_font(i, &p[4..]),
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

    /// `Font.Name`, `Font.Size`, `Font.Bold`... of control `i`.
    fn get_font(&self, i: usize, p: &str) -> ExecResult<Variant> {
        let c = &self.form.controls[i];
        if !c.kind.has_font() {
            return unsupported();
        }
        let f = &c.font;
        Ok(match p {
            "name" => Variant::String(if f.name.is_empty() {
                DEFAULT_FONT_NAME.to_string()
            } else {
                f.name.clone()
            }),
            "size" => Variant::Single(f.size),
            "bold" => Variant::Boolean(f.bold),
            "italic" => Variant::Boolean(f.italic),
            "underline" => Variant::Boolean(f.underline),
            "strikethrough" | "strikethru" => Variant::Boolean(f.strikethrough),
            _ => return unsupported(),
        })
    }

    /// Set `Font.Name`, `Font.Size`, `Font.Bold`... of control `i`.
    fn set_font(&mut self, i: usize, p: &str, value: &Variant) -> ExecResult<()> {
        if !self.form.controls[i].kind.has_font() {
            return unsupported();
        }
        let f = &mut self.form.controls[i].font;
        match p {
            "name" => {
                let name = value.as_string()?;
                f.name = if name.eq_ignore_ascii_case(DEFAULT_FONT_NAME) {
                    String::new()
                } else {
                    name
                };
            }
            "size" => {
                let size = value.as_f64()? as f32;
                if !(1.0..=2048.0).contains(&size) {
                    return err(380, "Invalid property value");
                }
                f.size = size;
            }
            "bold" => f.bold = value.as_bool()?,
            "italic" => f.italic = value.as_bool()?,
            "underline" => f.underline = value.as_bool()?,
            "strikethrough" | "strikethru" => f.strikethrough = value.as_bool()?,
            _ => return unsupported(),
        }
        Ok(())
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
            // CheckBox: 0, 1 or 2 (grayed). OptionButton: True/False; a
            // True clears the form's other OptionButtons, as a click does.
            "value" if kind == ControlKind::CheckBox => {
                let n = value.as_i64()?;
                if !(0..=2).contains(&n) {
                    return err(380, "Invalid property value");
                }
                self.state[i].value = n as i32;
            }
            "value" if kind == ControlKind::OptionButton => {
                let on = value.as_bool()?;
                let touched = set_value(&self.form, &mut self.state, i, on as i32);
                for j in touched {
                    self.touched[j] = true;
                }
            }
            "backcolor" if kind != ControlKind::Timer => {
                self.form.controls[i].color = Some(from_vb_color(value.as_i64()?)?);
            }
            "forecolor" if kind.has_font() => {
                self.form.controls[i].fore_color = Some(from_vb_color(value.as_i64()?)?);
            }
            "fontname" | "fontsize" | "fontbold" | "fontitalic" | "fontunderline"
            | "fontstrikethru" => return self.set_font(i, &p[4..], value),
            _ if p.starts_with("font.") => return self.set_font(i, &p[5..], value),
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

/// The font a control has unless one is set (what VB shows as `MS Sans
/// Serif`; here the system font).
pub const DEFAULT_FONT_NAME: &str = "MS Sans Serif";
/// The text colour a control has unless `ForeColor` is set.
pub const DEFAULT_FORE: u32 = 0x1D1D1F;

/// The background a kind of control has unless `BackColor` is set.
fn default_back(kind: ControlKind) -> u32 {
    match kind {
        ControlKind::TextBox
        | ControlKind::ListBox
        | ControlKind::ComboBox
        | ControlKind::PictureBox => 0xFFFFFF,
        ControlKind::CommandButton => 0xF6F6F8,
        _ => crate::FORM_BACK,
    }
}

/// A form colour (`0xRRGGBB`) as VB's `Long` (`&H00BBGGRR`, what `RGB()`
/// returns).
pub fn to_vb_color(rgb: u32) -> i32 {
    let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
    ((b << 16) | (g << 8) | r) as i32
}

/// VB's colour `Long` as a form colour (`0xRRGGBB`). Values outside
/// `0..=&HFFFFFF` (system colours, negative) are an error here.
pub fn from_vb_color(vb: i64) -> ExecResult<u32> {
    if !(0..=0xFF_FFFF).contains(&vb) {
        return err(380, "Invalid property value");
    }
    let vb = vb as u32;
    let (r, g, b) = (vb & 0xFF, (vb >> 8) & 0xFF, (vb >> 16) & 0xFF);
    Ok((r << 16) | (g << 8) | b)
}
impl ObjectHost for FormHost {
    fn has_object(&self, name: &str) -> bool {
        self.is_form(name)
            || self
                .form
                .controls
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(name))
    }

    fn get(&mut self, object: &ObjectRef, property: &str, args: &[Variant]) -> ExecResult<Variant> {
        let p = property.to_ascii_lowercase();
        match self.target(object)? {
            Target::Form => match p.as_str() {
                "caption" => Ok(Variant::String(self.form.caption.clone())),
                "name" => Ok(Variant::String(self.form.name.clone())),
                "width" => Ok(Variant::Single(self.form.width)),
                "height" => Ok(Variant::Single(self.form.height)),
                _ => unsupported(),
            },
            Target::Control(i) => self.get_control(i, &p, args),
        }
    }

    fn set(
        &mut self,
        object: &ObjectRef,
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

    fn call(&mut self, object: &ObjectRef, method: &str, args: &[Variant]) -> ExecResult<Variant> {
        let m = method.to_ascii_lowercase();
        let target = self.target(object)?;
        // a parameterised property read, `List1.List(0)`, arrives as a call
        if let Target::Control(i) = target {
            if matches!(m.as_str(), "list" | "listcount" | "listindex" | "text") {
                return self.get_control(i, &m, args);
            }
        }
        // `SetFocus` is the same for every control that takes the keyboard
        if m == "setfocus" {
            return match target {
                Target::Control(i) if self.form.controls[i].kind.can_focus() => {
                    let c = &self.form.controls[i];
                    if !c.enabled || !c.visible {
                        // VB: "Can't move focus to a disabled or invisible control"
                        return err(5, "Can't move focus to a disabled or invisible control");
                    }
                    self.focus = Some(i);
                    self.changed = true;
                    Ok(Variant::Empty)
                }
                _ => unsupported(),
            };
        }
        // the remaining methods are the list controls'
        let i = match target {
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
        let mut list = ControlSpec::new(
            "List1",
            ControlKind::ListBox,
            Rect::new(0.0, 60.0, 100.0, 80.0),
        );
        list.items = vec!["Apple".into(), "Cherry".into()];
        f.control(list);
        let mut combo = ControlSpec::new(
            "Combo1",
            ControlKind::ComboBox,
            Rect::new(0.0, 150.0, 100.0, 24.0),
        );
        combo.text = Some(String::new());
        combo.items = vec!["Red".into(), "Green".into(), "Blue".into()];
        f.control(combo);
        f.timer("Timer1", 250);
        // indexes 5..: a CheckBox, two OptionButtons, a two-button array
        f.control(ControlSpec::new(
            "Check1",
            ControlKind::CheckBox,
            Rect::new(0.0, 180.0, 110.0, 22.0),
        ));
        for n in 1..=2 {
            f.control(ControlSpec::new(
                &format!("Option{}", n),
                ControlKind::OptionButton,
                Rect::new(0.0, 200.0 + 22.0 * n as f32, 110.0, 22.0),
            ));
        }
        for i in 0..2 {
            let mut b = ControlSpec::new(
                "Cmd",
                ControlKind::CommandButton,
                Rect::new(120.0 * i as f32, 260.0, 100.0, 28.0),
            );
            b.caption = format!("Cmd {}", i);
            b.index = Some(i);
            f.control(b);
        }
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
        assert_eq!(
            snap.touched,
            [true, false, true, false, false, false, false, false, false, false]
        );
        // flags reset after being taken
        run("Text1.Text = \"y\"\n", &h).unwrap();
        let snap = h.borrow_mut().take_changes().unwrap();
        assert_eq!(
            snap.touched,
            [false, true, false, false, false, false, false, false, false, false]
        );
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
    fn check_and_option_values() {
        let h = host();
        let out = run(
            "Print Check1.Value; Option1.Value\n\
             Check1.Value = vbChecked\n\
             Option2.Value = True\n\
             Print Check1.Value; Option1.Value; Option2.Value\n\
             Option1.Value = True\n\
             Print Option1.Value; Option2.Value\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, " 0 False\n 1 FalseTrue\nTrueFalse\n");
        let snap = h.borrow().snapshot();
        assert_eq!(snap.state[5].value, 1);
        assert_eq!(
            (snap.state[6].value, snap.state[7].value),
            (1, 0),
            "selecting Option1 cleared Option2"
        );
        let c = h.borrow_mut().take_changes().unwrap();
        assert!(c.touched[5] && c.touched[6] && c.touched[7]);
        assert!(!c.touched[0]);

        let e = run("Check1.Value = 3\n", &h).unwrap_err();
        assert!(e.contains("380"), "{}", e);
        let e = run("Label1.Value = 1\n", &h).unwrap_err();
        assert!(e.contains("438"), "{}", e);
    }

    #[test]
    fn a_users_click_sets_values_and_clears_the_option_group() {
        let h = host();
        for (control, value) in [(7, 1), (6, 1), (5, 1)] {
            h.borrow_mut()
                .apply_edit(UserEdit::Value { control, value });
        }
        let out = run("Print Option1.Value; Option2.Value; Check1.Value\n", &h).unwrap();
        assert_eq!(out, "TrueFalse 1 \n");
    }
    #[test]
    fn colours_and_fonts() {
        let h = host();
        let out = run(
            "Label1.BackColor = RGB(255, 0, 0)\n\
             Label1.ForeColor = vbBlue\n\
             Print Label1.BackColor = vbRed\n\
             Print Label1.ForeColor\n\
             Label1.Font.Bold = True\n\
             Label1.Font.Size = 14\n\
             Label1.Font.Name = \"Courier\"\n\
             Text1.FontItalic = True\n\
             Print Label1.Font.Bold; Label1.Font.Size; Label1.Font.Name\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "True\n 16711680 \nTrue 14 Courier\n");
        let snap = h.borrow().snapshot();
        let label = &snap.form.controls[0];
        assert_eq!(label.color, Some(0xFF0000), "RGB(255,0,0) is red");
        assert_eq!(label.fore_color, Some(0x0000FF), "vbBlue is blue");
        assert!(label.font.bold);
        assert_eq!(label.font.size, 14.0);
        assert_eq!(label.font.name, "Courier");
        assert!(snap.form.controls[1].font.italic);

        let e = run("Label1.Font.Size = 0\n", &h).unwrap_err();
        assert!(e.contains("380"), "{}", e);
        let e = run("Label1.BackColor = -5\n", &h).unwrap_err();
        assert!(e.contains("380"), "{}", e);
        let e = run("Label1.Font.Wide = 1\n", &h).unwrap_err();
        assert!(e.contains("438"), "{}", e);
        let e = run("Timer1.BackColor = 1\n", &h).unwrap_err();
        assert!(e.contains("438"), "{}", e);
    }

    #[test]
    fn control_arrays() {
        let h = host();
        let out = run(
            "Cmd(0).Caption = \"First\"\n\
             Cmd(1).Enabled = False\n\
             Print Cmd(0).Caption; Cmd(1).Caption; Cmd(1).Index\n\
             Dim i As Long\n\
             For i = 0 To 1\n  Cmd(i).Left = 10 * (i + 1)\nNext\n\
             Print Cmd(0).Left; Cmd(1).Left\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "FirstCmd 1 1 \n 10  20 \n");
        let snap = h.borrow().snapshot();
        assert_eq!(snap.form.controls[8].caption, "First");
        assert!(!snap.form.controls[9].enabled);
        assert_eq!(snap.form.controls[9].rect.left, 20.0);

        let e = run("Cmd(5).Caption = \"x\"\n", &h).unwrap_err();
        assert!(e.contains("340"), "{}", e);
        let e = run("Cmd.Caption = \"x\"\n", &h).unwrap_err();
        assert!(e.contains("343"), "{}", e);
        let e = run("Label1(0).Caption = \"x\"\n", &h).unwrap_err();
        assert!(e.contains("343"), "{}", e);
        let e = run("Print Label1.Index\n", &h).unwrap_err();
        assert!(e.contains("343"), "{}", e);
    }

    #[test]
    fn set_focus_is_published_and_refused_for_the_unfocusable() {
        let h = host();
        run("Text1.SetFocus\n", &h).unwrap();
        let snap = h.borrow_mut().take_changes().unwrap();
        assert_eq!(snap.focus, Some(1));
        // the focus request is delivered once
        run("Label1.Caption = \"x\"\n", &h).unwrap();
        assert_eq!(h.borrow_mut().take_changes().unwrap().focus, None);

        run("Cmd(1).SetFocus\nCall Check1.SetFocus\n", &h).unwrap();
        assert_eq!(h.borrow_mut().take_changes().unwrap().focus, Some(5));

        // a Label can't take it; a disabled control can't either
        let e = run("Label1.SetFocus\n", &h).unwrap_err();
        assert!(e.contains("438"), "{}", e);
        let e = run("Text1.Enabled = False\nText1.SetFocus\n", &h).unwrap_err();
        assert!(e.contains("Can't move focus"), "{}", e);
        let e = run("Timer1.SetFocus\n", &h).unwrap_err();
        assert!(e.contains("438"), "{}", e);
    }

    #[test]
    fn timers_can_be_read_and_set() {
        let h = host();
        assert_eq!(h.borrow().timers(), vec![(4, 250, true)]);
        let out = run("Print Timer1.Interval; Timer1.Enabled\n", &h).unwrap();
        assert_eq!(out, " 250 True\n");
        run("Timer1.Enabled = False\nTimer1.Interval = 1000\n", &h).unwrap();
        assert_eq!(h.borrow().timers(), vec![(4, 1000, false)]);
        let e = run("Timer1.Interval = -1\n", &h).unwrap_err();
        assert!(e.contains("380"), "{}", e);
    }

    #[test]
    fn old_oxform_files_and_the_new_properties_round_trip() {
        // a file from before Value/Font/Index existed
        let old = r#"{"name":"F","caption":"F","width":10,"height":10,"controls":[
            {"name":"L","kind":"Label","rect":{"left":0,"top":0,"width":9,"height":9},
             "caption":"hi","color":null}]}"#;
        let f = FormSpec::from_oxform(old).unwrap();
        let c = &f.controls[0];
        assert_eq!((c.index, c.value, c.fore_color), (None, 0, None));
        assert_eq!(c.font, crate::FontSpec::default());

        let mut f = form();
        f.controls[8].font.bold = true;
        f.controls[8].fore_color = Some(0x112233);
        let back = FormSpec::from_oxform(&f.to_oxform()).unwrap();
        assert_eq!(back.controls[8].index, Some(0));
        assert!(back.controls[8].font.bold);
        assert_eq!(back.controls[8].fore_color, Some(0x112233));
        assert_eq!(back.controls[5].kind, ControlKind::CheckBox);
    }

    #[test]
    fn vb_colours_round_trip() {
        for rgb in [0x000000, 0xFF0000, 0x00FF00, 0x0000FF, 0x123456, 0xFFFFFF] {
            assert_eq!(from_vb_color(to_vb_color(rgb) as i64).unwrap(), rgb);
        }
        assert_eq!(to_vb_color(0xFF0000), 0x0000FF, "red is &H0000FF in VB");
        assert!(from_vb_color(0x1000000).is_err());
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
