//! Shared GPUI drawing of the classic VB6 controls, used by the IDE's form
//! designer, the IDE's run window and the standalone `oxbasic gui` window,
//! so a control looks (and, at run time, behaves) the same everywhere:
//!
//! | Control | Looks like |
//! |---------|------------|
//! | `Label` | bare text — no border, no box |
//! | `CommandButton` | a push button with a centred caption |
//! | `TextBox` | a white single-line field |
//! | `Frame` | a group box: thin outline with the caption set into its top edge |
//! | `ListBox` | a white scrolling list, the selected item highlighted |
//! | `ComboBox` | a field with a ▾ button that drops a pick list (`Style` 0 and 2), or a field above an always-visible list (`Style` 1) |
//! | `PictureBox` | a plain bordered canvas |
//! | `Timer` | invisible at run time; a small ◷ icon in the designer |

use crate::{ControlKind, ControlSpec, FormSpec, DEFAULT_FONT_SIZE};
use gpui::{
    deferred, div, prelude::*, px, rgb, AnyElement, Context, Div, FontWeight, Keystroke,
    SharedString, Stateful,
};

pub use crate::FORM_BACK;
const TEXT: u32 = 0x1D1D1F;
const DIM: u32 = 0x8E8E93;
const ACCENT: u32 = 0x007AFF;
const BUTTON_FACE: u32 = 0xF6F6F8;
const BUTTON_HOVER: u32 = 0xE8E8ED;
const BUTTON_ACTIVE: u32 = 0xDCEEFF;
const BUTTON_BORDER: u32 = 0xB4B4BA;
const FIELD_BORDER: u32 = 0x8E8E93;
const ROW_HOVER: u32 = 0xE8F1FD;
/// Height of one list row.
const ROW_H: f32 = 20.0;
/// Width of a combo box's ▾ button.
const ARROW_W: f32 = 20.0;

/// The interactive pieces of a control; a caller attaches its event
/// handlers to them through the `hook` of [`paint_control`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The whole control (a button, a text field, a label, a combo box...).
    Body,
    /// A combo box's text field (the part left of the ▾ button).
    Field,
    /// A combo box's ▾ button.
    Arrow,
    /// Row `i` of a list box, or of a Simple combo's list.
    ListItem(usize),
    /// Row `i` of a combo box's dropped-down list.
    DropItem(usize),
}

/// What changes how a control is drawn.
#[derive(Clone, Debug, Default)]
pub struct ControlLook {
    /// Drawn in the designer: no hover effects, placeholders for empty lists.
    pub design: bool,
    /// Current text of a TextBox / ComboBox.
    pub text: String,
    /// Selected row of a ListBox / ComboBox.
    pub list_index: Option<usize>,
    /// A combo box's list is open.
    pub dropped: bool,
    /// The control has the keyboard focus.
    pub focused: bool,
    /// `Value` of a CheckBox (0, 1, 2) or OptionButton (0, 1).
    pub value: i32,
}

impl ControlLook {
    /// The designer's static view of a control.
    pub fn design(c: &ControlSpec) -> ControlLook {
        ControlLook {
            design: true,
            text: c.text().to_string(),
            value: c.value,
            ..Default::default()
        }
    }
}

pub use crate::host::ControlState;

/// The text a keystroke types, if any: the platform's `key_char` (so
/// shift-' types `"` and shift-9 `(`), never a control character.
pub fn typed_text(ks: &Keystroke) -> Option<String> {
    if ks.modifiers.platform || ks.modifiers.control {
        return None;
    }
    let s = match &ks.key_char {
        Some(s) => s.clone(),
        None if ks.key == "space" => " ".to_string(),
        None if ks.key.chars().count() == 1 => {
            if ks.modifiers.shift {
                ks.key.to_uppercase()
            } else {
                ks.key.clone()
            }
        }
        None => return None,
    };
    (!s.is_empty() && !s.chars().any(|c| c.is_control())).then_some(s)
}

/// Apply a keystroke to single-line text. Returns whether it changed.
pub fn edit_text(text: &mut String, ks: &Keystroke) -> bool {
    if ks.modifiers.platform || ks.modifiers.control {
        return false;
    }
    if ks.key == "backspace" {
        return text.pop().is_some();
    }
    match typed_text(ks) {
        Some(s) => {
            text.push_str(&s);
            true
        }
        None => false,
    }
}

/// Drawing order: frames first, so they never cover the controls on them.
pub fn z_order(form: &FormSpec) -> Vec<usize> {
    let mut order: Vec<usize> = (0..form.controls.len()).collect();
    order.sort_by_key(|&i| form.controls[i].kind != ControlKind::Frame);
    order
}

fn caret() -> Div {
    div().flex_none().w(px(1.5)).h(px(14.0)).bg(rgb(ACCENT))
}

/// A single-line white field showing `text`.
fn text_field(base: Stateful<Div>, text: &str, focused: bool) -> Stateful<Div> {
    base.flex()
        .items_center()
        .px_1()
        .bg(rgb(0xFFFFFF))
        .border_1()
        .border_color(rgb(if focused { ACCENT } else { FIELD_BORDER }))
        .overflow_hidden()
        .whitespace_nowrap()
        .child(SharedString::from(text.to_string()))
        .when(focused, |s| s.child(caret()))
}

/// A list's rows: `items`, the selected one highlighted, each wired by
/// `hook` as `part(i)`. Scrolls vertically when it overflows.
fn list_rows<V: 'static>(
    id: SharedString,
    c: &ControlSpec,
    look: &ControlLook,
    part: fn(usize) -> Part,
    hook: &mut dyn FnMut(Part, Stateful<Div>, &mut Context<V>) -> Stateful<Div>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let mut list = div().id(id.clone()).flex().flex_col().overflow_y_scroll();
    if c.items.is_empty() && look.design {
        // VB's designer shows the control's name in an empty list.
        list = list.child(
            div()
                .px_1()
                .h(px(ROW_H))
                .flex()
                .items_center()
                .child(SharedString::from(c.name.clone())),
        );
    }
    for (i, item) in c.items.iter().enumerate() {
        let selected = look.list_index == Some(i);
        let mut row = div()
            .id((id.clone(), i))
            .debug_selector(|| format!("{}-item-{}", c.name, i))
            .flex_none()
            .h(px(ROW_H))
            .px_1()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .overflow_hidden()
            .child(SharedString::from(item.clone()));
        if selected {
            row = row.bg(rgb(ACCENT)).text_color(rgb(0xFFFFFF));
        } else if !look.design {
            row = row.hover(|s| s.bg(rgb(ROW_HOVER)));
        }
        list = list.child(hook(part(i), row, cx));
    }
    list
}

/// Draw control `c` as VB6 shows it, at its form position. `hook` attaches
/// the caller's event handlers (and debug selectors) to each [`Part`]; the
/// designer and the run windows differ only in their hooks.
pub fn paint_control<V: 'static>(
    c: &ControlSpec,
    look: &ControlLook,
    hook: &mut dyn FnMut(Part, Stateful<Div>, &mut Context<V>) -> Stateful<Div>,
    cx: &mut Context<V>,
) -> AnyElement {
    let id = SharedString::from(match c.index {
        Some(n) => format!("control-{}-{}", c.name, n),
        None => format!("control-{}", c.name),
    });
    let (w, h) = if c.kind == ControlKind::Timer {
        (28.0, 28.0)
    } else {
        (c.rect.width.max(4.0), c.rect.height.max(4.0))
    };
    let font = &c.font;
    let mut base = div()
        .id(id.clone())
        .absolute()
        .left(px(c.rect.left))
        .top(px(c.rect.top))
        .w(px(w))
        .h(px(h))
        .text_color(rgb(if !c.enabled {
            DIM
        } else {
            c.fore_color.unwrap_or(TEXT)
        }));
    // `Font`: points to pixels at 96 dpi (VB's 8.25 pt MS Sans Serif is
    // 11 px); an unset size keeps the size controls are drawn at
    if c.kind.has_font() {
        if font.size != DEFAULT_FONT_SIZE && font.size > 0.0 {
            base = base.text_size(px(font.size * 96.0 / 72.0));
        }
        if !font.name.is_empty() {
            base = base.font_family(SharedString::from(font.name.clone()));
        }
        if font.bold {
            base = base.font_weight(FontWeight::BOLD);
        }
        if font.italic {
            base = base.italic();
        }
        if font.underline {
            base = base.underline();
        }
        if font.strikethrough {
            base = base.line_through();
        }
    }
    // `BackColor`, where the control has a background of its own
    let back = |usual: u32| rgb(c.color.unwrap_or(usual));
    let el = match c.kind {
        // VB6 Label: BorderStyle 0 (None), so it is just text — on the
        // form's background unless it has a BackColor.
        ControlKind::Label => {
            let mut body = base
                .overflow_hidden()
                .child(SharedString::from(c.caption.clone()));
            if let Some(bg) = c.color {
                body = body.bg(rgb(bg));
            }
            hook(Part::Body, body, cx)
        }
        ControlKind::CommandButton => {
            let mut body = base
                .flex()
                .items_center()
                .justify_center()
                .px_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .rounded_md()
                .bg(back(BUTTON_FACE))
                .border_1()
                .border_color(rgb(BUTTON_BORDER))
                .child(SharedString::from(c.caption.clone()));
            if !look.design && c.enabled {
                body = body
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(BUTTON_HOVER)))
                    .active(|s| s.bg(rgb(BUTTON_ACTIVE)));
            }
            if look.focused {
                body = body.border_color(rgb(ACCENT));
            }
            hook(Part::Body, body, cx)
        }
        ControlKind::TextBox => hook(
            Part::Body,
            text_field(base, &look.text, look.focused).bg(back(0xFFFFFF)),
            cx,
        ),
        // VB6 Frame: a group box. The caption interrupts the top border.
        ControlKind::Frame => {
            let outline = div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(8.0))
                .bottom_0()
                .border_1()
                .border_color(rgb(BUTTON_BORDER))
                .rounded_sm();
            let mut body = base.child(outline);
            if !c.caption.is_empty() {
                body = body.child(
                    div()
                        .absolute()
                        .left(px(8.0))
                        .top_0()
                        .px_1()
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .bg(rgb(FORM_BACK))
                        .whitespace_nowrap()
                        .child(SharedString::from(c.caption.clone())),
                );
            }
            hook(Part::Body, body, cx)
        }
        ControlKind::ListBox => {
            let rows = list_rows(
                SharedString::from(format!("{}-list", c.name)),
                c,
                look,
                Part::ListItem,
                hook,
                cx,
            )
            .size_full();
            let body = base
                .bg(back(0xFFFFFF))
                .border_1()
                .border_color(rgb(if look.focused { ACCENT } else { FIELD_BORDER }))
                .overflow_hidden()
                .child(rows);
            hook(Part::Body, body, cx)
        }
        ControlKind::ComboBox => paint_combo(c, look, base, hook, cx),
        ControlKind::PictureBox => {
            let body = base
                .bg(back(0xFFFFFF))
                .border_1()
                .border_color(rgb(FIELD_BORDER));
            hook(Part::Body, body, cx)
        }
        // VB6 CheckBox / OptionButton: a box (☑) or circle (◉) and the
        // caption beside it; `Value` decides the mark.
        ControlKind::CheckBox | ControlKind::OptionButton => {
            let radio = c.kind == ControlKind::OptionButton;
            let mark = match (radio, look.value) {
                (true, 0) => "",
                (true, _) => "●",
                (false, 1) => "✓",
                (false, 2) => "■",
                (false, _) => "",
            };
            let indicator = div()
                .flex_none()
                .size(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(10.0))
                .bg(rgb(0xFFFFFF))
                .border_1()
                .border_color(rgb(if look.focused { ACCENT } else { FIELD_BORDER }))
                .text_color(rgb(if c.kind == ControlKind::CheckBox && look.value == 2 {
                    DIM
                } else {
                    ACCENT
                }))
                .when(radio, |s| s.rounded_full())
                .when(!radio, |s| s.rounded_sm())
                .child(mark);
            let mut body = base
                .flex()
                .items_center()
                .gap_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(indicator)
                .child(SharedString::from(c.caption.clone()));
            if let Some(bg) = c.color {
                body = body.bg(rgb(bg));
            }
            if !look.design && c.enabled {
                body = body.cursor_pointer();
            }
            hook(Part::Body, body, cx)
        }
        // Invisible at run time; the designer shows an icon to grab.
        ControlKind::Timer => {
            let body = base
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(rgb(BUTTON_FACE))
                .border_1()
                .border_color(rgb(BUTTON_BORDER))
                .child("◷");
            hook(Part::Body, body, cx)
        }
    };
    el.into_any_element()
}

/// The VB6 ComboBox, by `Style`:
///
/// * 0 *Dropdown Combo* — an editable text field with a ▾ button that
///   drops the list down;
/// * 1 *Simple Combo* — an editable text field with the list always shown
///   below it (the control is as tall as both);
/// * 2 *Dropdown List* — like 0, but the text can only be picked.
///
/// A dropped list floats over the controls below and closes on a pick.
fn paint_combo<V: 'static>(
    c: &ControlSpec,
    look: &ControlLook,
    base: Stateful<Div>,
    hook: &mut dyn FnMut(Part, Stateful<Div>, &mut Context<V>) -> Stateful<Div>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let id = SharedString::from(format!("{}-combo", c.name));
    let field_h = ROW_H + 6.0;
    if c.style == 1 {
        // Text field on top, list below.
        let field = text_field(
            div().id((id.clone(), 0usize)).h(px(field_h)).w_full(),
            &look.text,
            look.focused,
        );
        let rows = list_rows(
            SharedString::from(format!("{}-list", c.name)),
            c,
            look,
            Part::ListItem,
            hook,
            cx,
        )
        .flex_1()
        .min_h_0();
        let list = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(0xFFFFFF))
            .border_1()
            .border_color(rgb(FIELD_BORDER))
            .overflow_hidden()
            .child(rows);
        let body = base
            .flex()
            .flex_col()
            .child(hook(Part::Field, field, cx))
            .child(list);
        return hook(Part::Body, body, cx);
    }

    let editable = c.style == 0;
    let shown = if editable {
        look.text.clone()
    } else {
        // a Dropdown List shows the picked item, or nothing yet
        look.list_index
            .and_then(|i| c.items.get(i).cloned())
            .unwrap_or_else(|| look.text.clone())
    };
    let arrow = hook(
        Part::Arrow,
        div()
            .id((id.clone(), 1usize))
            .flex_none()
            .w(px(ARROW_W))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(rgb(FIELD_BORDER))
            .bg(rgb(if look.dropped {
                BUTTON_ACTIVE
            } else {
                BUTTON_FACE
            }))
            .when(!look.design, |s| s.cursor_pointer())
            .child(if look.dropped { "▴" } else { "▾" }),
        cx,
    );
    let mut field = hook(
        Part::Field,
        div()
            .id((id.clone(), 2usize))
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .px_1()
            .overflow_hidden()
            .whitespace_nowrap()
            .when(!editable && !look.design, |s| s.cursor_pointer())
            .child(SharedString::from(shown)),
        cx,
    );
    if editable && look.focused {
        field = field.child(caret());
    }
    let mut body = base
        .flex()
        .items_center()
        .bg(rgb(0xFFFFFF))
        .border_1()
        .border_color(rgb(if look.focused || look.dropped {
            ACCENT
        } else {
            FIELD_BORDER
        }))
        .child(field)
        .child(arrow);
    if look.dropped && !look.design {
        let rows = list_rows(
            SharedString::from(format!("{}-drop", c.name)),
            c,
            look,
            Part::DropItem,
            hook,
            cx,
        );
        let visible = (c.items.len().max(1) as f32).min(8.0);
        body = body.child(deferred(
            div()
                .id((id, 3usize))
                .occlude()
                .absolute()
                .top(px(c.rect.height.max(4.0) - 1.0))
                .left(px(-1.0))
                .w(px(c.rect.width.max(4.0)))
                .h(px(visible * ROW_H + 2.0))
                .bg(rgb(0xFFFFFF))
                .border_1()
                .border_color(rgb(FIELD_BORDER))
                .shadow_md()
                .overflow_hidden()
                .child(rows.size_full()),
        ));
    }
    hook(Part::Body, body, cx)
}
