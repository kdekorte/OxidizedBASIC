//! The live form as a BASIC program sees it: `Label1.Caption = "x"`,
//! `Text1.Text`, `List1.AddItem "a"`, `Combo1.ListIndex = 2`.
//!
//! [`FormHost`] implements the interpreter's [`ObjectHost`] over a form
//! model. The program's changes are published as [`FormSnapshot`]s for
//! the window that shows the form, and the user's edits come back as
//! [`UserEdit`]s so the program reads what the user typed or picked.
//!
//! No GUI is needed here, so this module is usable and tested headless.

use crate::{
    form::{list_items, volume_names},
    ControlKind, ControlSpec, FormSpec,
};
use oxbasic_runtime::{ExecResult, ObjectHost, ObjectRef, RaisedEvent, Variant, VbError};

/// A control's run-time state: the text of a TextBox or ComboBox, the
/// selected row of a list, and a CheckBox's or OptionButton's `Value`.
#[derive(Clone, Debug, Default)]
pub struct ControlState {
    pub text: String,
    pub list_index: Option<usize>,
    pub dropped: bool,
    pub value: i32,
    /// `Tab` of a TabControl: the selected page, as the program has it.
    pub tab: u8,
    /// The caret in `text`, in characters (`SelStart` + `SelLength` is
    /// where it is when a selection was made rightwards).
    pub caret: usize,
    /// The other end of the selection in `text`, if any: the selected
    /// text is between it and `caret`.
    pub anchor: Option<usize>,
    /// The rows a MultiSelect ListBox has selected (`Selected(i)`).
    pub selected: Vec<usize>,
    /// What the program drew on a PictureBox (`Line`, `Circle`...).
    pub canvas: crate::canvas::Canvas,
}

/// The word around character `at` of `text` (characters, `start..end`):
/// a run of letters, digits and `_`, or else the run of spaces or the one
/// other character there. At the end of the text, the word before it.
pub fn word_around(text: &str, at: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return (0, 0);
    }
    let i = at.min(chars.len() - 1);
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            0
        } else if c.is_whitespace() {
            1
        } else {
            2
        }
    };
    let k = class(chars[i]);
    let (mut a, mut b) = (i, i + 1);
    // a punctuation mark is a word of its own
    if k == 2 {
        return (a, b);
    }
    while a > 0 && class(chars[a - 1]) == k {
        a -= 1;
    }
    while b < chars.len() && class(chars[b]) == k {
        b += 1;
    }
    (a, b)
}

/// The line around character `at` of `text` (`start..end`, the line break
/// left out): what a triple click selects.
pub fn line_around(text: &str, at: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    let at = at.min(chars.len());
    let a = chars[..at]
        .iter()
        .rposition(|&c| c == '\n')
        .map_or(0, |p| p + 1);
    let b = chars[at..]
        .iter()
        .position(|&c| c == '\n')
        .map_or(chars.len(), |p| at + p);
    (a, b)
}

impl ControlState {
    pub fn new(c: &ControlSpec) -> ControlState {
        let text = match c.kind {
            ControlKind::TextBox | ControlKind::ComboBox => c.text().to_string(),
            _ => String::new(),
        };
        ControlState {
            caret: text.chars().count(),
            text,
            list_index: None,
            dropped: false,
            value: c.value,
            tab: c.tab,
            anchor: None,
            selected: Vec::new(),
            canvas: Default::default(),
        }
    }

    /// The selection as `(start, length)` in characters: VB's `SelStart`
    /// and `SelLength`.
    pub fn selection(&self) -> (usize, usize) {
        let len = self.text.chars().count();
        let caret = self.caret.min(len);
        match self.anchor.map(|a| a.min(len)) {
            Some(a) if a != caret => (a.min(caret), a.max(caret) - a.min(caret)),
            _ => (caret, 0),
        }
    }

    /// Select `length` characters from `start` (clamped to the text): the
    /// caret goes to the end of the selection, as VB's `SelStart` /
    /// `SelLength` leave it.
    pub fn select(&mut self, start: usize, length: usize) {
        let len = self.text.chars().count();
        let start = start.min(len);
        let end = (start + length).min(len);
        self.anchor = (end != start).then_some(start);
        self.caret = end;
    }

    /// Replace the selection with `s` (`SelText = s`, or typing); the caret
    /// ends after it.
    pub fn replace_selection(&mut self, s: &str) {
        let (start, length) = self.selection();
        let chars: Vec<char> = self.text.chars().collect();
        let mut out: String = chars[..start].iter().collect();
        out.push_str(s);
        out.extend(chars[start + length..].iter());
        self.text = out;
        self.caret = start + s.chars().count();
        self.anchor = None;
    }

    /// The selected text (`SelText`).
    pub fn selected_text(&self) -> String {
        let (start, length) = self.selection();
        self.text.chars().skip(start).take(length).collect()
    }

    /// Set the whole text (the program's `Text = ...`, or a pick): the
    /// caret goes to the end, as typing would leave it.
    pub fn set_text(&mut self, s: String) {
        self.caret = s.chars().count();
        self.anchor = None;
        self.text = s;
    }

    /// A key in a text field, as a TextBox / ComboBox does it: typing
    /// replaces the selection, Backspace / Delete remove it or the
    /// character beside the caret, arrows / Home / End move the caret
    /// (Shift extends the selection), Enter starts a new line in a
    /// MultiLine box. `Locked` refuses changes but allows moving; `MaxLength`
    /// refuses typing past it. Returns `(text changed, caret moved)`.
    pub fn edit_key(
        &mut self,
        c: &ControlSpec,
        key: &str,
        typed: Option<&str>,
        shift: bool,
    ) -> (bool, bool) {
        let len = self.text.chars().count();
        let caret = self.caret.min(len);
        let multi = c.multi_line && c.kind == ControlKind::TextBox;
        let move_to = |s: &mut ControlState, to: usize| {
            if shift {
                if s.anchor.is_none() {
                    s.anchor = Some(caret);
                }
            } else {
                s.anchor = None;
            }
            s.caret = to.min(len);
            if s.anchor == Some(s.caret) {
                s.anchor = None;
            }
        };
        let (start, sel_len) = self.selection();
        let has_sel = sel_len > 0;
        let chars: Vec<char> = self.text.chars().collect();
        // the start and end of the line the caret is on (MultiLine)
        let line_start = chars[..caret]
            .iter()
            .rposition(|&ch| ch == '\n')
            .map_or(0, |p| p + 1);
        let line_end = chars[caret..]
            .iter()
            .position(|&ch| ch == '\n')
            .map_or(len, |p| caret + p);
        match key {
            "left" => {
                let to = if has_sel && !shift {
                    start
                } else {
                    caret.saturating_sub(1)
                };
                move_to(self, to);
                return (false, true);
            }
            "right" => {
                let to = if has_sel && !shift {
                    start + sel_len
                } else {
                    caret + 1
                };
                move_to(self, to);
                return (false, true);
            }
            "home" => {
                move_to(self, if multi { line_start } else { 0 });
                return (false, true);
            }
            "end" => {
                move_to(self, if multi { line_end } else { len });
                return (false, true);
            }
            "up" | "down" if multi => {
                let col = caret - line_start;
                let to = if key == "up" {
                    if line_start == 0 {
                        0
                    } else {
                        let prev_start = chars[..line_start - 1]
                            .iter()
                            .rposition(|&ch| ch == '\n')
                            .map_or(0, |p| p + 1);
                        (prev_start + col).min(line_start - 1)
                    }
                } else if line_end >= len {
                    len
                } else {
                    let next_end = chars[line_end + 1..]
                        .iter()
                        .position(|&ch| ch == '\n')
                        .map_or(len, |p| line_end + 1 + p);
                    (line_end + 1 + col).min(next_end)
                };
                move_to(self, to);
                return (false, true);
            }
            _ => {}
        }
        if c.locked {
            return (false, false);
        }
        let max = c.max_length as usize;
        // what typing may add without going past MaxLength
        let room = |s: &ControlState| {
            if max == 0 {
                usize::MAX
            } else {
                max.saturating_sub(s.text.chars().count() - s.selection().1)
            }
        };
        match key {
            "backspace" => {
                if !has_sel {
                    if caret == 0 {
                        return (false, false);
                    }
                    self.anchor = Some(caret - 1);
                    self.caret = caret;
                }
                self.replace_selection("");
                (true, true)
            }
            "delete" => {
                if !has_sel {
                    if caret >= len {
                        return (false, false);
                    }
                    self.anchor = Some(caret + 1);
                    self.caret = caret;
                }
                self.replace_selection("");
                (true, true)
            }
            "enter" if multi => {
                if room(self) == 0 {
                    return (false, false);
                }
                self.replace_selection("\n");
                (true, true)
            }
            _ => match typed {
                Some(t) if !t.is_empty() => {
                    let t: String = t.chars().take(room(self)).collect();
                    if t.is_empty() {
                        return (false, false);
                    }
                    self.replace_selection(&t);
                    (true, true)
                }
                _ => (false, false),
            },
        }
    }

    /// The user picked item `i` of the control's list.
    pub fn pick(&mut self, c: &ControlSpec, i: usize) {
        self.list_index = Some(i);
        self.dropped = false;
        if c.kind == ControlKind::ComboBox {
            if let Some(item) = c.items.get(i) {
                self.set_text(item.clone());
            }
        }
        if c.multi_select == 0 {
            self.selected = vec![i];
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
    /// Which form of the project this is (0 for a single-form program).
    pub form_index: usize,
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
                state[i].caret = mine.caret;
                state[i].anchor = mine.anchor;
                state[i].selected = mine.selected.clone();
                state[i].canvas = mine.canvas.clone();
            }
        }
    }
}

/// Something the user did in the live window that the program must see.
#[derive(Clone, Debug)]
pub enum UserEdit {
    /// A scrollbar dragged to `value` (clamped to its `Min`..`Max`).
    Scroll { control: usize, value: i32 },
    /// The field's whole text after a keystroke.
    Text { control: usize, text: String },
    /// The user picked row `index` of a list or combo box.
    Pick { control: usize, index: usize },
    /// The user clicked a CheckBox or OptionButton, which now has `value`
    /// (an OptionButton also clears the others of its group).
    Value { control: usize, value: i32 },
    /// The user resized the form's window: its inside is now this big.
    FormSize { width: f32, height: f32 },
    /// The user moved the caret or selection in a text field (after its
    /// `Text` edit, if the text changed too).
    Caret {
        control: usize,
        caret: usize,
        anchor: Option<usize>,
    },
    /// The rows of a MultiSelect ListBox the user has selected, and the row
    /// with the focus (`ListIndex`).
    Selected {
        control: usize,
        rows: Vec<usize>,
        index: Option<usize>,
    },
}

/// The OptionButtons that act as one group with `i`: as in VB, the ones in
/// the same container (the same Frame or PictureBox, or the form itself).
pub fn option_group(form: &FormSpec, i: usize) -> Vec<usize> {
    if form.controls.get(i).map(|c| c.kind) != Some(ControlKind::OptionButton) {
        return vec![];
    }
    let home = form.container_of(i);
    form.controls
        .iter()
        .enumerate()
        .filter(|(j, c)| {
            *j != i && c.kind == ControlKind::OptionButton && form.container_of(*j) == home
        })
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
#[derive(Clone, Copy)]
pub(crate) enum Target {
    Form,
    Control(usize),
}

/// What of a control the program can change that VB raises an event for.
#[derive(PartialEq, Clone)]
pub(crate) struct Watched {
    text: String,
    caption: String,
    list_index: Option<usize>,
    value: i32,
    size: (f32, f32),
    path: String,
    tab: u8,
}

/// The live form, owned by the program's side. The interpreter reads and
/// writes its objects through [`ObjectHost`].
pub struct FormHost {
    pub(crate) form: FormSpec,
    pub(crate) state: Vec<ControlState>,
    /// Something changed since the last [`FormHost::take_changes`].
    pub(crate) changed: bool,
    /// Controls whose state the program changed since then.
    pub(crate) touched: Vec<bool>,
    /// `SetFocus` was called on this control since then.
    pub(crate) focus: Option<usize>,
    /// Events the program's own changes raised, for the interpreter to run
    /// (`Text1.Text = "x"` raises `Text1_Change`).
    raised: Vec<RaisedEvent>,
    /// The controls the form had at design time, by name and index: their
    /// array elements can't be unloaded.
    pub(crate) designed: Vec<(String, Option<i32>)>,
    /// The project's folder, which a picture's file name is relative to
    /// (needed to read its size for `AutoSize`).
    pub base_dir: Option<std::path::PathBuf>,
}

impl FormHost {
    pub fn new(mut form: FormSpec) -> FormHost {
        // a volume list shows the disks mounted now; a directory list the
        // folders in its Path, a file list the files in it that match its
        // Pattern (the first volume when Path is empty)
        let volumes = volume_names();
        for c in form.controls.iter_mut() {
            if c.kind == ControlKind::VolumeListBox {
                c.items = volumes.clone();
            } else if c.kind == ControlKind::DirListBox || c.kind == ControlKind::FileListBox {
                let path = if c.path.is_empty() {
                    volumes
                        .first()
                        .map(|v| format!("/Volumes/{}", v))
                        .unwrap_or_default()
                } else {
                    c.path.clone()
                };
                c.items = list_items(c.kind, &path, &c.pattern);
            }
        }
        let state = form.controls.iter().map(ControlState::new).collect();
        let touched = vec![false; form.controls.len()];
        let designed = form
            .controls
            .iter()
            .map(|c| (c.name.clone(), c.index))
            .collect();
        FormHost {
            form,
            state,
            changed: false,
            touched,
            focus: None,
            raised: Vec::new(),
            designed,
            base_dir: None,
        }
    }

    /// What of control `i` VB raises events for when the program changes it.
    pub(crate) fn watched(&self, i: usize) -> Watched {
        let c = &self.form.controls[i];
        let s = &self.state[i];
        Watched {
            text: s.text.clone(),
            caption: c.caption.clone(),
            list_index: s.list_index,
            value: s.value,
            size: (c.rect.width, c.rect.height),
            path: c.path.clone(),
            tab: s.tab,
        }
    }

    /// Queue `control`'s `event` for the interpreter (with its `Index` in a
    /// control array).
    fn raise(&mut self, i: usize, event: &str) {
        let c = &self.form.controls[i];
        self.raised.push(RaisedEvent {
            form: self.form.name.clone(),
            handler: format!("{}_{}", c.name, event),
            args: c
                .index
                .map(|n| vec![Variant::Integer(n as i16)])
                .unwrap_or_default(),
        });
    }

    /// The events VB raises for what the program just changed in control
    /// `i` (`before` is how it was), as VB raises them inside the
    /// statement: a TextBox's or ComboBox's `Text` and a Label's `Caption`
    /// raise `Change`; a list's `ListIndex` and a CheckBox's or a chosen
    /// OptionButton's `Value` raise `Click`; a PictureBox's size `Resize`.
    pub(crate) fn raise_changes(&mut self, i: usize, before: &Watched) {
        let after = self.watched(i);
        if after == *before {
            return;
        }
        let kind = self.form.controls[i].kind;
        match kind {
            ControlKind::TextBox if after.text != before.text => self.raise(i, "Change"),
            ControlKind::Label if after.caption != before.caption => self.raise(i, "Change"),
            ControlKind::ComboBox => {
                if after.text != before.text {
                    self.raise(i, "Change");
                }
                if after.list_index != before.list_index && after.list_index.is_some() {
                    self.raise(i, "Click");
                }
            }
            ControlKind::ListBox if after.list_index != before.list_index => self.raise(i, "Click"),
            // a TabControl's Tab changing shows another page: VB's Click
            ControlKind::TabControl if after.tab != before.tab => self.raise(i, "Click"),
            // a DirListBox's Path changing lists another folder: VB's Change
            ControlKind::DirListBox if after.path != before.path => self.raise(i, "Change"),
            ControlKind::CheckBox if after.value != before.value => self.raise(i, "Click"),
            // a scrollbar's or slider's Value changing raises Change, as VB does
            ControlKind::HScrollBar | ControlKind::VScrollBar | ControlKind::Slider
                if after.value != before.value =>
            {
                self.raise(i, "Change")
            }
            // setting an option button True clicks it; the others it clears
            // raise nothing
            ControlKind::OptionButton if after.value != 0 && before.value == 0 => {
                self.raise(i, "Click")
            }
            ControlKind::PictureBox if after.size != before.size => self.raise(i, "Resize"),
            _ => {}
        }
    }

    /// The form's name.
    pub fn name(&self) -> &str {
        &self.form.name
    }

    /// The form as the program has it now.
    pub fn spec(&self) -> &FormSpec {
        &self.form
    }

    /// Whether the form has a control called `name`.
    pub fn has_control(&self, name: &str) -> bool {
        self.form
            .controls
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// The form as it stands now, every control's state included.
    pub fn snapshot(&self) -> FormSnapshot {
        FormSnapshot {
            form: self.form.clone(),
            state: self.state.clone(),
            touched: vec![true; self.form.controls.len()],
            focus: self.focus,
            form_index: 0,
        }
    }

    /// What the program changed since the last call, else `None`.
    /// The handler names queued by events raised so far (`Dir1_Change`...).
    #[cfg(test)]
    pub(crate) fn raised_handlers(&self) -> Vec<String> {
        self.raised.iter().map(|e| e.handler.clone()).collect()
    }

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
            form_index: 0,
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
                    s.set_text(text);
                    s.list_index = None;
                }
            }
            UserEdit::Caret {
                control,
                caret,
                anchor,
            } => {
                if let Some(s) = self.state.get_mut(control) {
                    let len = s.text.chars().count();
                    s.caret = caret.min(len);
                    s.anchor = anchor.map(|a| a.min(len));
                }
            }
            UserEdit::Selected {
                control,
                rows,
                index,
            } => {
                if let Some(s) = self.state.get_mut(control) {
                    s.selected = rows;
                    s.list_index = index;
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
            // a scrollbar dragged to a new Value, kept within its range
            UserEdit::Scroll { control, value } => {
                // only a thumb the user can drag (a scrollbar or slider), not a progress bar
                if control < self.state.len() && self.form.controls[control].kind.is_draggable() {
                    let before = self.watched(control);
                    let c = &self.form.controls[control];
                    let v = value.clamp(c.scroll_min, c.scroll_max);
                    if self.state[control].value != v {
                        self.state[control].value = v;
                        self.changed = true;
                        self.touched[control] = true;
                        // the drag is a change of Value, so the program's
                        // Change handler runs, as it does for a set in code
                        self.raise_changes(control, &before);
                    }
                }
            }
            UserEdit::FormSize { width, height } => {
                if width > 0.0 && height > 0.0 {
                    self.form.width = width;
                    self.form.height = height;
                }
            }
        }
    }

    /// The control a program name refers to: `Label1`, or element `index`
    /// of a control array (`Command1(2)`).
    pub(crate) fn control_index(&self, name: &str, index: Option<i32>) -> Option<usize> {
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

    pub(crate) fn is_form(&self, object: &str) -> bool {
        object.eq_ignore_ascii_case("me") || object.eq_ignore_ascii_case(&self.form.name)
    }

    pub(crate) fn target(&self, object: &ObjectRef) -> ExecResult<Target> {
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
        let is_list = matches!(
            c.kind,
            ControlKind::ListBox
                | ControlKind::ComboBox
                | ControlKind::VolumeListBox
                | ControlKind::DirListBox
                | ControlKind::FileListBox
        );
        if let Some(fp) = p.strip_prefix("font.") {
            return self.get_font(i, fp);
        }
        Ok(match p {
            "caption" if c.kind.has_caption() => Variant::String(c.caption.clone()),
            "text" if c.kind.has_text() => Variant::String(s.text.clone()),
            // a ListBox's Text is its selected row
            "text" if is_list && c.kind != ControlKind::ComboBox => Variant::String(
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
            "tabindex" => Variant::Integer(self.form.tab_index_of(i) as i16),
            "tabstop" => Variant::Boolean(c.tab_stop),
            "tooltiptext" => Variant::String(c.tool_tip.clone()),
            "mousepointer" => Variant::Integer(c.mouse_pointer),
            "mouseicon" => Variant::String(c.mouse_icon.clone()),
            "container" => match &c.container {
                Some(name) => Variant::String(name.clone()),
                None => Variant::String(self.form.name.clone()),
            },
            "currentx" if c.kind == ControlKind::PictureBox => Variant::Single(s.canvas.current.0),
            "currenty" if c.kind == ControlKind::PictureBox => Variant::Single(s.canvas.current.1),
            "stretch" if c.kind == ControlKind::Image => Variant::Boolean(c.stretch),
            "autosize" if c.kind == ControlKind::PictureBox => Variant::Boolean(c.auto_size),
            "picture" if c.kind.has_picture() => Variant::String(c.picture.clone()),
            // the text controls' selection and limits
            "selstart" if c.kind.has_text() => Variant::Long(s.selection().0 as i32),
            "sellength" if c.kind.has_text() => Variant::Long(s.selection().1 as i32),
            "seltext" if c.kind.has_text() => Variant::String(s.selected_text()),
            "locked" if c.kind.has_text() => Variant::Boolean(c.locked),
            "maxlength" if c.kind == ControlKind::TextBox => Variant::Long(c.max_length as i32),
            "multiline" if c.kind == ControlKind::TextBox => Variant::Boolean(c.multi_line),
            "scrollbars" if c.kind == ControlKind::TextBox => {
                Variant::Integer(c.scroll_bars as i16)
            }
            // a ListBox's multiple selection
            "multiselect" if c.kind == ControlKind::ListBox => {
                Variant::Integer(c.multi_select as i16)
            }
            "selected" if is_list => {
                let n = index_arg(args)?;
                if n >= c.items.len() {
                    return err(381, "Invalid property array index");
                }
                Variant::Boolean(s.selected.contains(&n))
            }
            "selcount" if c.kind == ControlKind::ListBox => Variant::Long(s.selected.len() as i32),
            "enabled" => Variant::Boolean(c.enabled),
            "interval" if c.kind == ControlKind::Timer => Variant::Long(c.interval_ms as i32),
            // a scrollbar's range and its Value (kept within Min..Max)
            "min" if c.kind.is_scroll() => Variant::Long(c.scroll_min),
            "max" if c.kind.is_scroll() => Variant::Long(c.scroll_max),
            "smallchange" if c.kind.is_scroll() => Variant::Long(c.small_change),
            "largechange" if c.kind.is_scroll() => Variant::Long(c.large_change),
            "orientation" if c.kind == ControlKind::Slider => {
                Variant::Integer(c.orientation as i16)
            }
            "tab" if c.kind == ControlKind::TabControl => Variant::Integer(s.tab as i16),
            "tabs" if c.kind == ControlKind::TabControl => Variant::Integer(c.tabs.len() as i16),
            "value" if c.kind.is_scroll() => Variant::Long(s.value),
            "style" if c.kind == ControlKind::ComboBox => Variant::Integer(c.style as i16),
            "shape" if c.kind == ControlKind::Shape => Variant::Integer(c.shape_type as i16),
            "fillstyle" if c.kind == ControlKind::Shape => Variant::Integer(c.fill_style as i16),
            "diagonal" if c.kind == ControlKind::Line => Variant::Boolean(c.diagonal),
            "path" if matches!(c.kind, ControlKind::DirListBox | ControlKind::FileListBox) => {
                Variant::String(c.path.clone())
            }
            "pattern" if c.kind == ControlKind::FileListBox => Variant::String(c.pattern.clone()),
            // FileName: the selected file's path (Path and the file's name); "" when none is selected
            "filename" if c.kind == ControlKind::FileListBox => Variant::String(
                s.list_index
                    .and_then(|n| c.items.get(n))
                    .map(|name| {
                        if c.path.is_empty() {
                            name.clone()
                        } else {
                            std::path::Path::new(&c.path)
                                .join(name)
                                .to_string_lossy()
                                .into_owned()
                        }
                    })
                    .unwrap_or_default(),
            ),
            // a CheckBox's Value is 0/1/2; an OptionButton's is a Boolean
            "value" if c.kind == ControlKind::CheckBox => Variant::Integer(s.value as i16),
            "value" if c.kind == ControlKind::OptionButton => Variant::Boolean(s.value != 0),
            "backcolor" if c.kind != ControlKind::Timer => match c.back_system {
                Some(sys) => Variant::Long(sys as i32),
                None => Variant::Long(to_vb_color(c.color.unwrap_or(default_back(c.kind)))),
            },
            "forecolor" if c.kind.has_font() => match c.fore_system {
                Some(sys) => Variant::Long(sys as i32),
                None => Variant::Long(to_vb_color(c.fore_color.unwrap_or(DEFAULT_FORE))),
            },
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
            "keypreview" => self.form.key_preview = value.as_bool()?,
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
        let is_list = matches!(
            kind,
            ControlKind::ListBox
                | ControlKind::ComboBox
                | ControlKind::VolumeListBox
                | ControlKind::DirListBox
                | ControlKind::FileListBox
        );
        match p {
            "caption" if kind.has_caption() => {
                self.form.controls[i].caption = value.as_string()?;
            }
            "text" if kind.has_text() => {
                let mut text = value.as_string()?;
                // MaxLength limits what the program sets, as VB does
                let max = self.form.controls[i].max_length as usize;
                if max > 0 && text.chars().count() > max {
                    text = text.chars().take(max).collect();
                }
                self.state[i].set_text(text);
            }
            "tooltiptext" => self.form.controls[i].tool_tip = value.as_string()?,
            "mouseicon" => self.form.controls[i].mouse_icon = value.as_string()?,
            "mousepointer" => {
                let n = value.as_i64()?;
                if !(0..=15).contains(&n) && n != 99 {
                    return err(380, "Invalid property value");
                }
                self.form.controls[i].mouse_pointer = n as i16;
            }
            "selstart" if kind.has_text() => {
                let n = value.as_i64()?;
                if n < 0 {
                    return err(380, "Invalid property value");
                }
                // VB: setting SelStart drops the selection
                self.state[i].select(n as usize, 0);
            }
            "sellength" if kind.has_text() => {
                let n = value.as_i64()?;
                if n < 0 {
                    return err(380, "Invalid property value");
                }
                let start = self.state[i].selection().0;
                self.state[i].select(start, n as usize);
            }
            "seltext" if kind.has_text() => {
                let s = value.as_string()?;
                self.state[i].replace_selection(&s);
                let max = self.form.controls[i].max_length as usize;
                if max > 0 && self.state[i].text.chars().count() > max {
                    let cut: String = self.state[i].text.chars().take(max).collect();
                    let caret = self.state[i].caret.min(max);
                    self.state[i].text = cut;
                    self.state[i].caret = caret;
                }
            }
            "locked" if kind.has_text() => self.form.controls[i].locked = value.as_bool()?,
            "maxlength" if kind == ControlKind::TextBox => {
                let n = value.as_i64()?;
                if n < 0 {
                    return err(380, "Invalid property value");
                }
                self.form.controls[i].max_length = n as u32;
            }
            // VB makes these read-only at run time: they shape the window
            "multiline" | "scrollbars" if kind == ControlKind::TextBox => {
                return err(382, "Property is read-only at run time")
            }
            "multiselect" if kind == ControlKind::ListBox => {
                return err(382, "Property is read-only at run time")
            }
            "selected" if is_list => {
                let n = index_arg(args)?;
                if n >= self.form.controls[i].items.len() {
                    return err(381, "Invalid property array index");
                }
                let on = value.as_bool()?;
                let multi = self.form.controls[i].multi_select != 0;
                let s = &mut self.state[i];
                if !multi {
                    // a one-row list: selecting a row is choosing it
                    if on {
                        s.selected = vec![n];
                        s.list_index = Some(n);
                    } else if s.list_index == Some(n) {
                        s.selected.clear();
                        s.list_index = None;
                    }
                } else if on && !s.selected.contains(&n) {
                    s.selected.push(n);
                    s.selected.sort_unstable();
                } else if !on {
                    s.selected.retain(|&r| r != n);
                }
            }
            "picture" if kind.has_picture() => {
                let p = value.as_string()?;
                if p != self.form.controls[i].picture {
                    self.form.controls[i].picture = p;
                    let base = self.base_dir.clone();
                    self.auto_size_to_picture(i, base.as_deref());
                    // VB raises a PictureBox's Change when its Picture changes
                    if kind == ControlKind::PictureBox {
                        self.raise(i, "Change");
                    }
                }
            }
            "currentx" if kind == ControlKind::PictureBox => {
                self.state[i].canvas.current.0 = value.as_f64()? as f32;
            }
            "currenty" if kind == ControlKind::PictureBox => {
                self.state[i].canvas.current.1 = value.as_f64()? as f32;
            }
            "stretch" if kind == ControlKind::Image => {
                self.form.controls[i].stretch = value.as_bool()?;
            }
            "autosize" if kind == ControlKind::PictureBox => {
                self.form.controls[i].auto_size = value.as_bool()?;
                let base = self.base_dir.clone();
                self.auto_size_to_picture(i, base.as_deref());
            }
            "container" => return self.set_container(i, value),
            "text" if is_list && kind != ControlKind::ComboBox => {
                return err(382, "Property is read-only")
            }
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
            "tabindex" => {
                let n = value.as_i64()?;
                if n < 0 || n >= self.form.controls.len() as i64 {
                    return err(380, "Invalid property value");
                }
                self.form.set_tab_index(i, n as i32);
            }
            "tabstop" => self.form.controls[i].tab_stop = value.as_bool()?,
            "enabled" => self.form.controls[i].enabled = value.as_bool()?,
            "interval" if kind == ControlKind::Timer => {
                let n = value.as_i64()?;
                if n < 0 {
                    return err(380, "Invalid property value");
                }
                self.form.controls[i].interval_ms = n as u64;
            }
            // a scrollbar's range: Min must stay below Max; Value is kept
            // within the range, as VB does
            "min" if kind.is_scroll() => {
                let n = value.as_i64()? as i32;
                let c = &mut self.form.controls[i];
                if n >= c.scroll_max {
                    return err(380, "Invalid property value");
                }
                c.scroll_min = n;
                self.state[i].value = self.state[i].value.clamp(n, c.scroll_max);
            }
            "max" if kind.is_scroll() => {
                let n = value.as_i64()? as i32;
                let c = &mut self.form.controls[i];
                if n <= c.scroll_min {
                    return err(380, "Invalid property value");
                }
                c.scroll_max = n;
                self.state[i].value = self.state[i].value.clamp(c.scroll_min, n);
            }
            "smallchange" if kind.is_scroll() => {
                let n = value.as_i64()? as i32;
                if n < 1 {
                    return err(380, "Invalid property value");
                }
                self.form.controls[i].small_change = n;
            }
            "orientation" if kind == ControlKind::Slider => {
                let n = value.as_i64()?;
                if !(0..=1).contains(&n) {
                    return err(380, "Invalid property value");
                }
                self.form.controls[i].orientation = n as u8;
            }
            // TabControl.Tab: the selected page, 0 up to the last tab. Setting
            // it shows that page and raises Click, as VB's TabStrip does.
            "tab" if kind == ControlKind::TabControl => {
                let n = value.as_i64()?;
                let count = self.form.controls[i].tabs.len() as i64;
                if n < 0 || n >= count {
                    return err(380, "Invalid property value");
                }
                self.state[i].tab = n as u8;
                // the form's own copy too: shown and usable read it there
                self.form.controls[i].tab = n as u8;
            }
            "largechange" if kind.is_scroll() => {
                let n = value.as_i64()? as i32;
                if n < 1 {
                    return err(380, "Invalid property value");
                }
                self.form.controls[i].large_change = n;
            }
            "value" if kind.is_scroll() => {
                let n = value.as_i64()? as i32;
                let c = &self.form.controls[i];
                if n < c.scroll_min || n > c.scroll_max {
                    return err(380, "Invalid property value");
                }
                self.state[i].value = n;
            }
            "style" if kind == ControlKind::ComboBox => {
                let n = value.as_i64()?;
                if !(0..=2).contains(&n) {
                    return err(380, "Invalid property value");
                }
                self.form.controls[i].style = n as u8;
            }
            // Shape: 0 rectangle, 1 square, 2 oval, 3 circle; FillStyle 0 solid, 1 transparent
            "shape" if kind == ControlKind::Shape => {
                let n = value.as_i64()?;
                if !(0..=3).contains(&n) {
                    return err(380, "Invalid property value");
                }
                self.form.controls[i].shape_type = n as u8;
            }
            "fillstyle" if kind == ControlKind::Shape => {
                let n = value.as_i64()?;
                if !(0..=1).contains(&n) {
                    return err(380, "Invalid property value");
                }
                self.form.controls[i].fill_style = n as u8;
            }
            "diagonal" if kind == ControlKind::Line => {
                self.form.controls[i].diagonal = value.as_bool()?;
            }
            // DirListBox / FileListBox Path: the folder listed. A path that
            // isn't a folder is VB's error 76; setting it reloads the list.
            "path" if matches!(kind, ControlKind::DirListBox | ControlKind::FileListBox) => {
                let path = value.as_string()?;
                if !std::path::Path::new(&path).is_dir() {
                    return err(76, "Path not found");
                }
                let c = &mut self.form.controls[i];
                c.path = path.clone();
                c.items = list_items(kind, &path, &c.pattern.clone());
                self.state[i].list_index = None;
            }
            // FileListBox.Pattern: which files it lists; the list is reloaded
            "pattern" if kind == ControlKind::FileListBox => {
                let pattern = value.as_string()?;
                let c = &mut self.form.controls[i];
                c.pattern = pattern.clone();
                let path = if c.path.is_empty() {
                    String::new()
                } else {
                    c.path.clone()
                };
                c.items = list_items(kind, &path, &pattern);
                self.state[i].list_index = None;
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
                let vb = value.as_i64()?;
                let c = &mut self.form.controls[i];
                c.color = Some(from_vb_color(vb)?);
                c.back_system = system_bits(vb);
            }
            "forecolor" if kind.has_font() => {
                let vb = value.as_i64()?;
                let c = &mut self.form.controls[i];
                c.fore_color = Some(from_vb_color(vb)?);
                c.fore_system = system_bits(vb);
            }
            "fontname" | "fontsize" | "fontbold" | "fontitalic" | "fontunderline"
            | "fontstrikethru" => return self.set_font(i, &p[4..], value),
            _ if p.starts_with("font.") => return self.set_font(i, &p[5..], value),
            "listindex" if is_list => {
                let n = value.as_i64()?;
                if n == -1 {
                    self.state[i].list_index = None;
                    if self.form.controls[i].multi_select == 0 {
                        self.state[i].selected.clear();
                    }
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

pub(crate) fn err<T>(number: i32, description: &str) -> ExecResult<T> {
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
        | ControlKind::VolumeListBox
        | ControlKind::DirListBox
        | ControlKind::FileListBox
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

/// VB's colour `Long` as a form colour (`0xRRGGBB`). A system colour
/// (`&H800000nn`, `vbButtonFace` and the like) resolves to this IDE's look
/// for it; other values outside `0..=&HFFFFFF` raise 380.
pub fn from_vb_color(vb: i64) -> ExecResult<u32> {
    // a negative Long is the same bits as &H8000_00nn
    let bits = (vb as i32) as u32 as i64;
    if vb < 0 || vb >= 0x8000_0000 {
        return match system_color(bits as u32) {
            Some(rgb) => Ok(rgb),
            None => err(380, "Invalid property value"),
        };
    }
    if !(0..=0xFF_FFFF).contains(&vb) {
        return err(380, "Invalid property value");
    }
    let vb = vb as u32;
    let (r, g, b) = (vb & 0xFF, (vb >> 8) & 0xFF, (vb >> 16) & 0xFF);
    Ok((r << 16) | (g << 8) | b)
}

/// The system colour bits (`&H800000nn`) of a colour value, if it is one.
pub fn system_bits(vb: i64) -> Option<u32> {
    let bits = (vb as i32) as u32;
    (vb < 0 || vb >= 0x8000_0000).then_some(bits)
}

/// The colour (`0xRRGGBB`) of VB system colour `vb` (`&H80000000` +
/// the Windows `COLOR_*` index), in this IDE's light look. `None` for an
/// index Windows doesn't have.
pub fn system_color(vb: u32) -> Option<u32> {
    if vb & 0xFFFF_FF00 != 0x8000_0000 {
        return None;
    }
    Some(match vb & 0xFF {
        0x00 => 0xC8C8C8,         // vbScrollBars
        0x01 => 0x3A6EA5,         // vbDesktop
        0x02 => 0x0A64D8,         // vbActiveTitleBar
        0x03 => 0x7A96DF,         // vbInactiveTitleBar
        0x04 => crate::FORM_BACK, // vbMenuBar
        0x05 => 0xFFFFFF,         // vbWindowBackground
        0x06 => 0x646464,         // vbWindowFrame
        0x07 => 0x1D1D1F,         // vbMenuText
        0x08 => 0x1D1D1F,         // vbWindowText
        0x09 => 0xFFFFFF,         // vbTitleBarText
        0x0A => 0xB4B4BA,         // vbActiveBorder
        0x0B => 0xD8D8DC,         // vbInactiveBorder
        0x0C => 0xABABAB,         // vbApplicationWorkspace
        0x0D => 0x007AFF,         // vbHighlight
        0x0E => 0xFFFFFF,         // vbHighlightText
        0x0F => crate::FORM_BACK, // vbButtonFace (3D face)
        0x10 => 0xA0A0A0,         // vbButtonShadow
        0x11 => 0x8E8E93,         // vbGrayText
        0x12 => 0x1D1D1F,         // vbButtonText
        0x13 => 0x3C3C43,         // vbInactiveCaptionText
        0x14 => 0xFFFFFF,         // vb3DHighlight
        0x15 => 0x404040,         // vb3DDKShadow
        0x16 => 0xE3E3E3,         // vb3DLight
        0x17 => 0x1D1D1F,         // vbInfoText
        0x18 => 0xFFFFE1,         // vbInfoBackground
        _ => return None,
    })
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
                "keypreview" => Ok(Variant::Boolean(self.form.key_preview)),
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
        let before = match target {
            Target::Control(i) => Some(self.watched(i)),
            Target::Form => None,
        };
        let form_size = (self.form.width, self.form.height);
        let result = match target {
            Target::Form => self.set_form(&p, &value),
            Target::Control(i) => self.set_control(i, &p, args, &value),
        };
        if result.is_ok() {
            self.changed = true;
            match target {
                Target::Control(i) => {
                    self.touched[i] = true;
                    if let Some(before) = before {
                        self.raise_changes(i, &before);
                    }
                }
                Target::Form if (self.form.width, self.form.height) != form_size => {
                    self.raised.push(RaisedEvent {
                        form: self.form.name.clone(),
                        handler: "Form_Resize".into(),
                        args: vec![],
                    });
                }
                Target::Form => {}
            }
        }
        result
    }

    fn call(&mut self, object: &ObjectRef, method: &str, args: &[Variant]) -> ExecResult<Variant> {
        let m = method.to_ascii_lowercase();
        // `Load Cmd(3)` adds an element that doesn't exist yet, so it is
        // handled before the element is looked up
        if let (Some(n), "load") = (object.index, m.as_str()) {
            return self.load_element(&object.name, n);
        }
        if let (Some(n), "unload") = (object.index, m.as_str()) {
            return self.unload_element(&object.name, n);
        }
        let target = self.target(object)?;
        if let Target::Control(i) = target {
            match m.as_str() {
                "cls" if self.form.controls[i].kind == ControlKind::PictureBox => {
                    return self.cls(i)
                }
                "move" => return self.move_control(i, args),
                "zorder" => return self.zorder(i, args),
                _ => {}
            }
        }
        // a parameterised property read, `List1.List(0)`, arrives as a call
        if let Target::Control(i) = target {
            if matches!(
                m.as_str(),
                "list" | "listcount" | "listindex" | "text" | "selected"
            ) {
                return self.get_control(i, &m, args);
            }
        }
        // `SetFocus` is the same for every control that takes the keyboard
        if m == "setfocus" {
            return match target {
                Target::Control(i) if self.form.controls[i].kind.can_focus() => {
                    // in a disabled or hidden frame counts too
                    if !self.form.usable(i) || !self.form.shown(i) {
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
                    ControlKind::ListBox
                        | ControlKind::ComboBox
                        | ControlKind::VolumeListBox
                        | ControlKind::DirListBox
                        | ControlKind::FileListBox
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

    fn take_raised(&mut self) -> Vec<RaisedEvent> {
        std::mem::take(&mut self.raised)
    }

    fn draw(
        &mut self,
        object: Option<&ObjectRef>,
        shape: oxbasic_runtime::DrawCmd,
    ) -> ExecResult<()> {
        self.draw_on(object, shape)
    }

    fn current_point(&self, object: Option<&ObjectRef>) -> (f32, f32) {
        self.current_of(object)
    }

    fn unloading_form(&self, object: &ObjectRef) -> Option<String> {
        (object.index.is_none() && self.is_form(&object.name)).then(|| self.form.name.clone())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::Rect;
    use std::cell::RefCell;
    use std::rc::Rc;

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

    pub(super) fn host() -> Rc<RefCell<FormHost>> {
        Rc::new(RefCell::new(FormHost::new(form())))
    }

    /// Run `src` with `host` as its object host; returns the printed
    /// output, or the error text.
    pub(crate) fn run(src: &str, host: &Rc<RefCell<FormHost>>) -> Result<String, String> {
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
    fn system_colours_resolve_and_read_back_as_set() {
        // vbButtonFace is the form's face; vbWindowBackground is white
        assert_eq!(from_vb_color(0x8000_000F).unwrap(), crate::FORM_BACK);
        assert_eq!(from_vb_color(-2147483643).unwrap(), 0xFFFFFF, "as a Long");
        assert!(from_vb_color(0x8000_0040).is_err(), "no such index");
        let h = host();
        let out = run(
            "Label1.BackColor = vbInfoBackground\nLabel1.ForeColor = &H80000012\n\
             Print Label1.BackColor = vbInfoBackground; Label1.ForeColor = vbButtonText\n\
             Label1.BackColor = vbRed\nPrint Label1.BackColor\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "TrueTrue\n 255 \n", "system values read back as set");
        let b = h.borrow();
        let c = &b.spec().controls[0];
        assert_eq!(
            c.color,
            Some(0xFF0000),
            "then red, and no longer a system colour"
        );
        assert_eq!((c.back_system, c.fore_system), (None, Some(0x8000_0012)));
        assert_eq!(
            c.fore_color,
            Some(0x1D1D1F),
            "drawn in the IDE's text colour"
        );
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

    /// Each event handler of the sample form prints its name, so the
    /// output lists the events a statement raised, in order.
    const TRACE: &str = "Sub Text1_Change()\n  Print \"Text1_Change \" & Text1.Text\nEnd Sub\n\
        Sub Label1_Change()\n  Print \"Label1_Change\"\nEnd Sub\n\
        Sub List1_Click()\n  Print \"List1_Click \" & List1.ListIndex\nEnd Sub\n\
        Sub Combo1_Change()\n  Print \"Combo1_Change \" & Combo1.Text\nEnd Sub\n\
        Sub Combo1_Click()\n  Print \"Combo1_Click\"\nEnd Sub\n\
        Sub Check1_Click()\n  Print \"Check1_Click \" & Check1.Value\nEnd Sub\n\
        Sub Option1_Click()\n  Print \"Option1_Click\"\nEnd Sub\n\
        Sub Option2_Click()\n  Print \"Option2_Click\"\nEnd Sub\n\
        Sub Form_Resize()\n  Print \"Form_Resize\"\nEnd Sub\n";

    #[test]
    fn program_changes_raise_vbs_events_inside_the_statement() {
        let h = host();
        let src = format!(
            "{}Text1.Text = \"typed\"\nPrint \"next\"\nLabel1.Caption = \"x\"\n\
             List1.ListIndex = 1\nCombo1.ListIndex = 2\nCheck1.Value = 1\n\
             Option2.Value = True\nMe.Width = 500\n",
            TRACE
        );
        assert_eq!(
            run(&src, &h).unwrap(),
            "Text1_Change typed\nnext\nLabel1_Change\nList1_Click 1\n\
             Combo1_Change Blue\nCombo1_Click\nCheck1_Click 1\nOption2_Click\nForm_Resize\n"
        );
    }

    #[test]
    fn setting_the_same_value_or_clearing_an_option_raises_nothing() {
        let h = host();
        let src = format!(
            "{}Text1.Text = \"start\"\nList1.ListIndex = -1\nCheck1.Value = 0\n\
             Option1.Value = True\nOption2.Value = True\nOption2.Value = False\n",
            TRACE
        );
        // Text1 already says "start", List1 has no row, Check1 is 0;
        // choosing Option2 clears Option1 without clicking it
        assert_eq!(run(&src, &h).unwrap(), "Option1_Click\nOption2_Click\n");
    }

    #[test]
    fn tab_index_tab_stop_and_key_preview_read_and_write_from_code() {
        let h = host();
        let out = run(
            "Print Text1.TabIndex; Combo1.TabIndex\n\
             Combo1.TabIndex = 0\n\
             Print Combo1.TabIndex; Label1.TabIndex; Text1.TabIndex\n\
             Text1.TabStop = False\n\
             Print Text1.TabStop; Combo1.TabStop\n\
             Print Me.KeyPreview\nMe.KeyPreview = True\nPrint Me.KeyPreview\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, " 1  3 \n 0  1  2 \nFalseTrue\nFalse\nTrue\n");
        // the program's copy of the form has it all
        let b = h.borrow();
        assert!(b.spec().key_preview);
        assert_eq!(b.spec().tab_stops()[0], 3, "Combo1 first, Text1 skipped");
        drop(b);
        for bad in ["Text1.TabIndex = -1", "Text1.TabIndex = 99"] {
            assert!(run(bad, &h).unwrap_err().contains("380"), "{}", bad);
        }
    }

    #[test]
    fn a_control_array_elements_event_gets_its_index() {
        let h = host();
        h.borrow_mut().form.controls[8].kind = ControlKind::TextBox;
        h.borrow_mut().form.controls[9].kind = ControlKind::TextBox;
        let src = "Sub Cmd_Change(Index As Integer)\n  Print \"changed \" & Index\nEnd Sub\n\
                   Cmd(1).Text = \"x\"\n";
        assert_eq!(run(src, &h).unwrap(), "changed 1\n");
    }

    #[test]
    fn user_edits_raise_nothing_by_themselves() {
        // the window fires the user's events; applying its edits must not
        // raise them a second time
        let h = host();
        h.borrow_mut().apply_edit(UserEdit::Text {
            control: 1,
            text: "typed".into(),
        });
        h.borrow_mut().apply_edit(UserEdit::Pick {
            control: 2,
            index: 0,
        });
        assert!(h.borrow_mut().take_raised().is_empty());
    }
}

#[cfg(test)]
mod more_members {
    use super::tests::{host, run};
    use super::*;

    #[test]
    fn load_and_unload_array_elements_at_run_time() {
        let h = host();
        let out = run(
            "Load Cmd(5)\nPrint Cmd(5).Visible; Cmd(5).Caption\n\
             Cmd(5).Visible = True\nCmd(5).Caption = \"new\"\n\
             Print Cmd(5).Caption; Cmd(0).Caption\nUnload Cmd(5)\n",
            &h,
        )
        .unwrap();
        // a copy of the lowest element, hidden until shown
        assert_eq!(out, "FalseCmd 0\nnewCmd 0\n");
        assert_eq!(h.borrow().spec().controls.len(), 10, "unloaded again");
        let fails = [
            ("Load Cmd(1)", "360"),    // already loaded
            ("Unload Cmd(0)", "362"),  // designed on the form
            ("Unload Cmd(7)", "340"),  // no such element
            ("Load Label1(1)", "343"), // not an array
            ("Print Cmd(5).Caption", "340"),
        ];
        for (src, num) in fails {
            let e = run(src, &h).unwrap_err();
            assert!(e.contains(num), "{} -> {}", src, e);
        }
    }

    #[test]
    fn a_loaded_element_raises_its_arrays_events_with_its_index() {
        let h = host();
        h.borrow_mut().form.controls[8].kind = ControlKind::TextBox;
        h.borrow_mut().form.controls[9].kind = ControlKind::TextBox;
        let out = run(
            "Sub Cmd_Change(Index As Integer)\n  Print \"change \" & Index\nEnd Sub\n\
             Load Cmd(4)\nCmd(4).Text = \"x\"\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "change 4\n");
    }

    #[test]
    fn move_sets_position_and_size_and_zorder_restacks() {
        let h = host();
        let out = run(
            "Label1.Move 5, 6\nPrint Label1.Left; Label1.Top; Label1.Width\n\
             Label1.Move 1, 2, 30, 40\nPrint Label1.Width; Label1.Height\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, " 5  6  100 \n 30  40 \n");
        assert!(run("Label1.Move 1, 2, 0", &h).unwrap_err().contains("380"));
        // ZOrder: Label1 (first) to the front = last drawn; the tab order
        // stays as it was
        let before = h
            .borrow()
            .spec()
            .tab_order()
            .iter()
            .map(|&i| h.borrow().spec().controls[i].name.clone())
            .collect::<Vec<_>>();
        run("Label1.ZOrder\nCmd(1).ZOrder vbSendToBack\n", &h).unwrap();
        let b = h.borrow();
        let names: Vec<_> = b.spec().controls.iter().map(|c| c.name.clone()).collect();
        assert_eq!(names.first().unwrap(), "Cmd");
        assert_eq!(names.last().unwrap(), "Label1");
        let after: Vec<_> = b
            .spec()
            .tab_order()
            .iter()
            .map(|&i| b.spec().controls[i].name.clone())
            .collect();
        assert_eq!(after, before, "restacking keeps the tab order");
        drop(b);
        assert!(run("Label1.ZOrder 2", &h).unwrap_err().contains("5"));
    }
}

#[cfg(test)]
mod text_list_and_container_members {
    use super::tests::{host, run};
    use super::*;

    #[test]
    fn sel_start_sel_length_and_sel_text_edit_the_text() {
        let h = host();
        // Text1 starts as "start"
        let out = run(
            "Print Text1.SelStart; Text1.SelLength\n\
             Text1.SelStart = 1\nText1.SelLength = 3\nPrint Text1.SelText\n\
             Text1.SelText = \"XY\"\nPrint Text1.Text; Text1.SelStart; Text1.SelLength\n\
             Text1.SelStart = 99\nPrint Text1.SelStart\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, " 5  0 \ntar\nsXYt 3  0 \n 4 \n");
        assert!(run("Text1.SelStart = -1", &h).unwrap_err().contains("380"));
        assert!(run("x = Label1.SelText", &h).unwrap_err().contains("438"));
    }

    #[test]
    fn max_length_locked_multi_line_and_scroll_bars() {
        let h = host();
        let out = run(
            "Text1.MaxLength = 3\nText1.Text = \"abcdef\"\nPrint Text1.Text; Text1.MaxLength\n\
             Text1.Locked = True\nPrint Text1.Locked; Text1.MultiLine; Text1.ScrollBars\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "abc 3 \nTrueFalse 0 \n");
        // read-only at run time, as in VB
        for src in [
            "Text1.MultiLine = True",
            "Text1.ScrollBars = 2",
            "List1.MultiSelect = 1",
        ] {
            assert!(run(src, &h).unwrap_err().contains("382"), "{}", src);
        }
    }

    #[test]
    fn selected_on_one_row_and_multi_select_lists() {
        let h = host();
        // List1 (one row at a time): Selected chooses the row
        let out = run(
            "List1.Selected(1) = True\nPrint List1.ListIndex; List1.Selected(0); List1.Selected(1)\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, " 1 FalseTrue\n");
        // with MultiSelect several rows can be selected at once
        h.borrow_mut().form.controls[2].multi_select = 1;
        let out = run(
            "List1.Selected(0) = True\nPrint List1.Selected(0); List1.Selected(1); List1.SelCount\n\
             List1.Selected(1) = False\nPrint List1.SelCount\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "TrueTrue 2 \n 1 \n");
        assert!(run("x = List1.Selected(9)", &h)
            .unwrap_err()
            .contains("381"));
    }

    #[test]
    fn tool_tip_mouse_pointer_and_picture() {
        let h = host();
        h.borrow_mut().form.controls.push(ControlSpec::new(
            "Picture1",
            ControlKind::PictureBox,
            crate::Rect::new(0.0, 0.0, 50.0, 50.0),
        ));
        let n = h.borrow().form.controls.len();
        h.borrow_mut().state.push(ControlState::default());
        h.borrow_mut().touched.resize(n, false);
        let out = run(
            "Sub Picture1_Change()\n  Print \"changed\"\nEnd Sub\n\
             Label1.ToolTipText = \"Hi\"\nLabel1.MousePointer = vbHourglass\n\
             Print Label1.ToolTipText; Label1.MousePointer\n\
             Picture1.Picture = LoadPicture(\"Cargo.toml\")\nPrint Picture1.Picture\n\
             Picture1.Picture = LoadPicture(\"Cargo.toml\")\n",
            &h,
        )
        .unwrap();
        assert_eq!(
            out, "Hi 11 \nchanged\nCargo.toml\n",
            "one Change: the second set is the same"
        );
        assert!(run("Label1.MousePointer = 42", &h)
            .unwrap_err()
            .contains("380"));
    }

    #[test]
    fn option_buttons_group_by_container() {
        let h = host();
        let mut frame = ControlSpec::new(
            "Frame1",
            ControlKind::Frame,
            crate::Rect::new(200.0, 0.0, 150.0, 100.0),
        );
        frame.caption = "Group".into();
        h.borrow_mut().form.controls.push(frame);
        let n = h.borrow().form.controls.len();
        h.borrow_mut().state.push(ControlState::default());
        h.borrow_mut().touched.resize(n, false);
        // Option1 and Option2 start on the form; Option2 moves into Frame1
        let out = run(
            "Set Option2.Container = Frame1\nPrint Option2.Container\n\
             Option1.Value = True\nOption2.Value = True\nPrint Option1.Value; Option2.Value\n\
             Set Option2.Container = Me\nOption2.Value = True\nPrint Option1.Value\n",
            &h,
        )
        .unwrap();
        // in different groups both stay chosen; back on the form they share one
        assert_eq!(out, "Frame1\nTrueTrue\nFalse\n");
        let bad = [
            ("Set Option1.Container = Label1", "380"), // a Label holds nothing
            ("Set Frame1.Container = Frame1", "380"),  // nor itself
        ];
        for (src, num) in bad {
            assert!(run(src, &h).unwrap_err().contains(num), "{}", src);
        }
    }

    #[test]
    fn a_container_offsets_hides_and_disables_what_it_holds() {
        let mut f = FormSpec::new("F", "F", 400.0, 300.0);
        let r = crate::Rect::new(10.0, 20.0, 100.0, 80.0);
        f.control(ControlSpec::new("Frame1", ControlKind::Frame, r));
        let mut t = ControlSpec::new(
            "Text1",
            ControlKind::TextBox,
            crate::Rect::new(5.0, 6.0, 50.0, 20.0),
        );
        t.container = Some("Frame1".into());
        f.control(t);
        assert_eq!(
            f.form_origin(1),
            (15.0, 26.0),
            "Left/Top are from the frame"
        );
        assert!(f.contains(0, 1) && !f.contains(1, 0));
        assert_eq!(f.tab_stops(), [1]);
        f.controls[0].visible = false;
        assert!(!f.shown(1), "a hidden frame hides what is in it");
        assert!(f.tab_stops().is_empty());
        f.controls[0].visible = true;
        f.controls[0].enabled = false;
        assert!(!f.usable(1), "and a disabled one disables it");
    }
}

#[cfg(test)]
mod drawing_and_images {
    use super::*;
    use crate::project_host::ProjectHost;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Buf(String);
    impl oxbasic_runtime::Output for Buf {
        fn write(&mut self, s: &str) {
            self.0.push_str(s);
        }
    }

    /// A form with a PictureBox, an Image and a Label.
    fn art() -> Rc<RefCell<ProjectHost>> {
        let mut f = FormSpec::new("Form1", "Art", 400.0, 300.0);
        f.control(ControlSpec::new(
            "Picture1",
            ControlKind::PictureBox,
            crate::Rect::new(10.0, 10.0, 120.0, 100.0),
        ));
        f.control(ControlSpec::new(
            "Image1",
            ControlKind::Image,
            crate::Rect::new(150.0, 10.0, 60.0, 60.0),
        ));
        f.label("Label1", "x", crate::Rect::new(0.0, 200.0, 50.0, 20.0));
        Rc::new(RefCell::new(ProjectHost::new(vec![f])))
    }

    fn run(src: &str, host: &Rc<RefCell<ProjectHost>>) -> Result<String, String> {
        run_in(src, host, None)
    }

    /// `src` run with `base` as the folder relative file names are in.
    fn run_in(
        src: &str,
        host: &Rc<RefCell<ProjectHost>>,
        base: Option<std::path::PathBuf>,
    ) -> Result<String, String> {
        let prog = oxbasic_parser::parse(src)?;
        let mut out = Buf(String::new());
        {
            let mut rt = oxbasic_runtime::Interpreter::new(&prog, &mut out);
            rt.base_dir = base;
            rt.set_object_host(host.clone());
            rt.run().map_err(|e| e.to_string())?;
        }
        Ok(out.0)
    }

    fn shapes(h: &Rc<RefCell<ProjectHost>>) -> Vec<crate::canvas::Shape> {
        h.borrow().snapshot(0).unwrap().state[0]
            .canvas
            .shapes
            .clone()
    }

    #[test]
    fn line_pset_and_circle_draw_on_a_picture_box() {
        use crate::canvas::Shape;
        let h = art();
        let out = run(
            "Picture1.Line (5, 6)-(50, 60), vbRed\n\
             Picture1.Line -(70, 60)\n\
             Picture1.Line (1, 1)-(9, 9), RGB(0, 255, 0), BF\n\
             Picture1.PSet (3, 4)\n\
             Picture1.Circle (40, 40), 10\n\
             Print Picture1.CurrentX; Picture1.CurrentY\n",
            &h,
        )
        .unwrap();
        let s = shapes(&h);
        assert_eq!(s.len(), 4, "two lines, a box and a circle; PSet is a pixel");
        // VB's colours: vbRed is &H0000FF, which is red here
        assert_eq!(
            s[0],
            Shape::Line {
                from: (5.0, 6.0),
                to: (50.0, 60.0),
                color: 0xFF0000
            }
        );
        // `Line -(x, y)` carries on from where the last line ended
        assert_eq!(
            s[1],
            Shape::Line {
                from: (50.0, 60.0),
                to: (70.0, 60.0),
                color: crate::host::DEFAULT_FORE
            }
        );
        assert_eq!(
            s[2],
            Shape::Rect {
                from: (1.0, 1.0),
                to: (9.0, 9.0),
                color: 0x00FF00,
                filled: true
            }
        );
        // PSet sets a pixel of the bitmap, not a shape: the circle is next
        assert!(matches!(s[3], Shape::Ellipse { radius, .. } if radius == 10.0));
        let raster = h.borrow().snapshot(0).unwrap().state[0]
            .canvas
            .raster
            .clone();
        assert_eq!(raster.pixel(3, 4), Some(crate::host::DEFAULT_FORE));
        assert_eq!(out, " 40  40 \n", "CurrentX/Y is the circle's centre");
    }

    #[test]
    fn step_counts_from_the_current_point_and_cls_clears() {
        let h = art();
        run(
            "Picture1.CurrentX = 10\nPicture1.CurrentY = 20\n\
             Picture1.PSet Step(5, 5)\n\
             Picture1.Line -Step(10, 0)\n",
            &h,
        )
        .unwrap();
        let s = shapes(&h);
        let raster = h.borrow().snapshot(0).unwrap().state[0]
            .canvas
            .raster
            .clone();
        assert!(
            raster.pixel(15, 25).is_some(),
            "PSet Step(5, 5) from (10, 20)"
        );
        assert!(matches!(
            s[0],
            crate::canvas::Shape::Line {
                from: (15.0, 25.0),
                to: (25.0, 25.0),
                ..
            }
        ));
        let out = run("Picture1.Cls\nPrint Picture1.CurrentX\n", &h).unwrap();
        assert_eq!(out, " 0 \n");
        assert!(shapes(&h).is_empty(), "Cls clears it");
        let raster = h.borrow().snapshot(0).unwrap().state[0]
            .canvas
            .raster
            .clone();
        assert_eq!(raster.count(), 0, "and the pixels");
    }

    #[test]
    fn drawing_errors_are_vbs() {
        let h = art();
        // not a PictureBox, a missing control, a bad colour
        assert!(run("Label1.Line (0, 0)-(1, 1)\n", &h)
            .unwrap_err()
            .contains("438"));
        assert!(run("Nope.PSet (1, 1)\n", &h).unwrap_err().contains("424"));
        assert!(run("Picture1.PSet (1, 1), &H7FFFFFFF\n", &h)
            .unwrap_err()
            .contains("380"));
        assert!(shapes(&h).is_empty(), "a bad draw leaves nothing");
        // a drawing statement doesn't make `Line` a keyword elsewhere
        let out = run("Dim line As Long\nline = 7\nPrint line\n", &h).unwrap();
        assert_eq!(out, " 7 \n");
    }

    #[test]
    fn image_has_picture_and_stretch_but_no_drawing() {
        let h = art();
        let out = run(
            "Image1.Stretch = True\nImage1.Picture = LoadPicture()\nPrint Image1.Stretch\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "True\n");
        // an Image isn't a drawing surface, and Cls is a PictureBox's
        assert!(run("Image1.Line (0, 0)-(1, 1)\n", &h)
            .unwrap_err()
            .contains("438"));
        assert!(run("Image1.Cls\n", &h).unwrap_err().contains("438"));
        // a PictureBox has no Stretch, an Image no AutoSize
        assert!(run("Picture1.Stretch = True\n", &h)
            .unwrap_err()
            .contains("438"));
        assert!(run("Image1.AutoSize = True\n", &h)
            .unwrap_err()
            .contains("438"));
        // an Image takes no focus
        assert!(run("Image1.SetFocus\n", &h).unwrap_err().contains("438"));
    }

    #[test]
    fn auto_size_takes_the_pictures_size_and_a_missing_file_is_53() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase");
        let (w, h_px) = crate::host_draw::picture_size(&dir.join("logo.png")).expect("logo.png");
        assert_eq!((w, h_px), (512.0, 512.0));
        let h = art();
        h.borrow_mut().set_base_dir(Some(dir.clone()));
        run_in(
            "Picture1.AutoSize = True\nPicture1.Picture = LoadPicture(\"logo.png\")\n",
            &h,
            Some(dir),
        )
        .unwrap();
        let c = h.borrow().snapshot(0).unwrap().form.controls[0].clone();
        assert_eq!(
            (c.rect.width, c.rect.height),
            (516.0, 516.0),
            "the picture and its border"
        );
        assert!(run("x = LoadPicture(\"missing.png\")\n", &h)
            .unwrap_err()
            .contains("53"));
    }
}

#[cfg(test)]
mod word_and_line_selection {
    use super::{line_around, word_around};

    #[test]
    fn a_word_is_a_run_of_word_characters_spaces_or_one_other_character() {
        let t = "Hello, big_world  x";
        assert_eq!(word_around(t, 2), (0, 5), "Hello");
        assert_eq!(word_around(t, 5), (5, 6), "a comma is a word of its own");
        assert_eq!(word_around(t, 8), (7, 16), "big_world, underscore and all");
        assert_eq!(word_around(t, 16), (16, 18), "the spaces between");
        assert_eq!(word_around(t, 99), (18, 19), "past the end: the last word");
        assert_eq!(word_around("", 0), (0, 0));
        // characters, not bytes
        assert_eq!(word_around("héllo wörld", 8), (6, 11));
    }

    #[test]
    fn a_line_is_what_lies_between_its_breaks() {
        let t = "one\ntwo\n\nfour";
        assert_eq!(line_around(t, 1), (0, 3));
        assert_eq!(line_around(t, 5), (4, 7));
        assert_eq!(line_around(t, 8), (8, 8), "an empty line");
        assert_eq!(line_around(t, 99), (9, 13), "past the end: the last line");
        assert_eq!(line_around("single", 3), (0, 6));
    }

    #[test]
    fn edit_keys_follow_the_caret_and_selection() {
        use crate::host::ControlState;
        use crate::{ControlKind, ControlSpec, Rect};
        let c = ControlSpec::new(
            "Text1",
            ControlKind::TextBox,
            Rect::new(0.0, 0.0, 100.0, 20.0),
        );
        let mut s = ControlState::default();
        s.set_text("hello world".into());
        s.select(6, 5); // "world"
        assert_eq!(s.selected_text(), "world");
        // typing replaces the selection
        s.edit_key(&c, "x", Some("x"), false);
        assert_eq!(s.text, "hello x");
        assert_eq!(s.selection(), (7, 0));
        // Shift+Left selects back over the "x"
        s.edit_key(&c, "left", None, true);
        assert_eq!(s.selected_text(), "x");
        // a plain Left drops it, at its start
        s.edit_key(&c, "left", None, false);
        assert_eq!(s.selection(), (6, 0));
        // Backspace removes the character before the caret
        let (changed, _) = s.edit_key(&c, "backspace", None, false);
        assert!(changed);
        assert_eq!(s.text, "hellox");
    }
}

#[cfg(test)]
mod mouse_icon_member {
    use super::tests::{host, run};

    #[test]
    fn mouse_icon_reads_and_writes_as_a_picture_file_name() {
        let h = host();
        let out = run(
            "Print \"[\" & Label1.MouseIcon & \"]\"\n\
             Label1.MouseIcon = LoadPicture(\"Cargo.toml\")\nLabel1.MousePointer = vbCustom\n\
             Print Label1.MouseIcon; Label1.MousePointer\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "[]\nCargo.toml 99 \n");
        let b = h.borrow();
        let c = &b.spec().controls[0];
        assert_eq!((c.mouse_pointer, c.mouse_icon.as_str()), (99, "Cargo.toml"));
        // saved with the form, and older files have none
        let mut f = crate::FormSpec::new("F", "F", 10.0, 10.0);
        assert!(!f.to_oxform().contains("mouseIcon"));
        f.label("L", "", crate::Rect::new(0.0, 0.0, 1.0, 1.0));
        f.controls[0].mouse_icon = "a.png".into();
        let back = crate::FormSpec::from_oxform(&f.to_oxform()).unwrap();
        assert_eq!(back.controls[0].mouse_icon, "a.png");
    }
}

#[cfg(test)]
mod scroll_tests {
    use super::*;
    use crate::Rect;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Form1 with HScroll1 (0..100) and VScroll1 (10..20, Value 10).
    fn host() -> Rc<RefCell<FormHost>> {
        let mut f = FormSpec::new("Form1", "Scroll", 400.0, 300.0);
        f.control(ControlSpec::new(
            "HScroll1",
            ControlKind::HScrollBar,
            Rect::new(0.0, 0.0, 120.0, 17.0),
        ));
        f.controls[0].scroll_max = 100;
        let mut v = ControlSpec::new(
            "VScroll1",
            ControlKind::VScrollBar,
            Rect::new(0.0, 30.0, 17.0, 120.0),
        );
        v.scroll_min = 10;
        v.scroll_max = 20;
        v.value = 10;
        f.control(v);
        Rc::new(RefCell::new(FormHost::new(f)))
    }

    #[test]
    fn a_scrollbar_has_vb_defaults_and_its_range_is_settable() {
        let h = host();
        let out = tests::run(
            "Print HScroll1.Min; HScroll1.Max; HScroll1.SmallChange; HScroll1.LargeChange\n\
             HScroll1.Max = 50\nHScroll1.Value = 25\nPrint HScroll1.Value; HScroll1.Max\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, " 0  100  1  1 \n 25  50 \n");
    }

    #[test]
    fn value_outside_min_and_max_is_an_error_380() {
        let h = host();
        for src in [
            "VScroll1.Value = 21",
            "VScroll1.Value = 9",
            "VScroll1.Min = 20",
            "VScroll1.Max = 10",
            "VScroll1.SmallChange = 0",
        ] {
            assert!(tests::run(src, &h).unwrap_err().contains("380"), "{}", src);
        }
    }

    #[test]
    fn narrowing_the_range_keeps_value_inside_it() {
        let h = host();
        tests::run("VScroll1.Max = 15\nPrint VScroll1.Value\n", &h).unwrap();
        let out = tests::run("VScroll1.Min = 12\nPrint VScroll1.Value\n", &h).unwrap();
        assert_eq!(out, " 12 \n", "the Min change moved Value up to 12");
    }

    #[test]
    fn a_drag_edit_is_clamped_to_the_range_and_reported() {
        let h = host();
        h.borrow_mut().apply_edit(UserEdit::Scroll {
            control: 0,
            value: 500,
        });
        assert_eq!(h.borrow().state[0].value, 100, "clamped to Max");
        h.borrow_mut().apply_edit(UserEdit::Scroll {
            control: 1,
            value: 99,
        });
        assert_eq!(h.borrow().state[1].value, 20, "99 is clamped to Max 20");
        let out = tests::run("Print HScroll1.Value; VScroll1.Value\n", &h).unwrap();
        assert_eq!(out, " 100  20 \n", "{}", out);
    }
}

#[cfg(test)]
mod volume_tests {
    use super::*;
    use crate::Rect;

    #[test]
    fn a_volume_list_holds_the_mounted_volumes_in_order() {
        let mut f = FormSpec::new("Form1", "Volumes", 400.0, 300.0);
        f.control(ControlSpec::new(
            "Volume1",
            ControlKind::VolumeListBox,
            Rect::new(0.0, 0.0, 120.0, 84.0),
        ));
        let h = FormHost::new(f);
        let items = &h.form.controls[0].items;
        assert_eq!(items, &volume_names(), "the list is the volumes now");
        assert!(items.windows(2).all(|w| w[0] <= w[1]), "sorted");
        assert!(
            items.iter().all(|n| !n.starts_with('.')),
            "no hidden entries"
        );
    }

    #[test]
    fn a_volume_list_is_read_by_basic_like_any_list() {
        let mut f = FormSpec::new("Form1", "Volumes", 400.0, 300.0);
        f.control(ControlSpec::new(
            "Volume1",
            ControlKind::VolumeListBox,
            Rect::new(0.0, 0.0, 120.0, 84.0),
        ));
        let h = std::rc::Rc::new(std::cell::RefCell::new(FormHost::new(f)));
        let out = tests::run("Print Volume1.ListCount\n", &h).unwrap();
        assert_eq!(out, format!(" {} \n", volume_names().len()));
    }
}

#[cfg(test)]
mod figure_tests {
    use super::*;
    use crate::Rect;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn host() -> Rc<RefCell<FormHost>> {
        let mut f = FormSpec::new("Form1", "Figures", 400.0, 300.0);
        f.control(ControlSpec::new(
            "Shape1",
            ControlKind::Shape,
            Rect::new(0.0, 0.0, 100.0, 60.0),
        ));
        f.control(ControlSpec::new(
            "Line1",
            ControlKind::Line,
            Rect::new(0.0, 80.0, 120.0, 40.0),
        ));
        Rc::new(RefCell::new(FormHost::new(f)))
    }

    #[test]
    fn a_shape_and_its_fill_style_are_settable_with_vb_defaults() {
        let h = host();
        let out = tests::run(
            "Print Shape1.Shape; Shape1.FillStyle\nShape1.Shape = 3\nShape1.FillStyle = 0\nPrint Shape1.Shape; Shape1.FillStyle\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, " 0  1 \n 3  0 \n");
    }

    #[test]
    fn an_out_of_range_shape_or_fill_style_is_an_error_380() {
        let h = host();
        for src in ["Shape1.Shape = 4", "Shape1.FillStyle = 2"] {
            assert!(tests::run(src, &h).unwrap_err().contains("380"), "{}", src);
        }
    }

    #[test]
    fn a_line_can_be_set_diagonal() {
        let h = host();
        let out = tests::run(
            "Print Line1.Diagonal\nLine1.Diagonal = True\nPrint Line1.Diagonal\n",
            &h,
        )
        .unwrap();
        assert_eq!(out, "False\nTrue\n");
    }
}
