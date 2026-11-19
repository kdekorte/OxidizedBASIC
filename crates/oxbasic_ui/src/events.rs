//! The events a user's actions raise, in the order VB raises them, and
//! their arguments as VB passes them: mouse buttons and `Shift` as bit
//! masks, keys as VB key codes (`vbKeyA` = 65, `vbKeyReturn` = 13...),
//! `KeyAscii` as a character code.
//!
//! The live window translates its GUI events into these; nothing here
//! needs a GUI, so the rules are tested headless.

use oxbasic_runtime::Variant;

/// `Button` of the mouse events: `vbLeftButton` = 1, `vbRightButton` = 2,
/// `vbMiddleButton` = 4.
pub const LEFT_BUTTON: i16 = 1;
pub const RIGHT_BUTTON: i16 = 2;
pub const MIDDLE_BUTTON: i16 = 4;

/// `Shift` of the mouse and key events: `vbShiftMask` = 1,
/// `vbCtrlMask` = 2, `vbAltMask` = 4.
pub const SHIFT_MASK: i16 = 1;
pub const CTRL_MASK: i16 = 2;
pub const ALT_MASK: i16 = 4;

/// The modifier keys held, as VB's `Shift` mask. On a Mac, ⌘ counts as
/// Ctrl, as Ctrl+C is ⌘C there.
pub fn shift_mask(shift: bool, ctrl: bool, alt: bool) -> i16 {
    (shift as i16) * SHIFT_MASK | (ctrl as i16) * CTRL_MASK | (alt as i16) * ALT_MASK
}

/// The VB key code (`KeyCode` of `KeyDown`/`KeyUp`) of a key, named as the
/// GUI names it (`"a"`, `"enter"`, `"f1"`, `"left"`...). Letters are their
/// upper-case code whatever the case typed (`vbKeyA` = 65); digits are
/// `vbKey0`..`vbKey9`. `None` for keys VB has no code for.
pub fn key_code(key: &str) -> Option<i16> {
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return match c {
            'a'..='z' => Some(c.to_ascii_uppercase() as i16),
            'A'..='Z' | '0'..='9' => Some(c as i16),
            ' ' => Some(32),
            // punctuation keys: VB's codes for the US layout
            ';' => Some(186),
            '=' => Some(187),
            ',' => Some(188),
            '-' => Some(189),
            '.' => Some(190),
            '/' => Some(191),
            '`' => Some(192),
            '[' => Some(219),
            '\\' => Some(220),
            ']' => Some(221),
            '\'' => Some(222),
            _ => None,
        };
    }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<i16>().ok()) {
        return (1..=16).contains(&n).then_some(111 + n); // vbKeyF1 = 112
    }
    Some(match key {
        "backspace" => 8,
        "tab" => 9,
        "enter" => 13,
        "shift" => 16,
        "control" => 17,
        "alt" => 18,
        "escape" => 27,
        "space" => 32,
        "pageup" => 33,
        "pagedown" => 34,
        "end" => 35,
        "home" => 36,
        "left" => 37,
        "up" => 38,
        "right" => 39,
        "down" => 40,
        "insert" => 45,
        "delete" => 46,
        _ => return None,
    })
}

/// The `KeyAscii` a keystroke types, if it types one: the character's code
/// (`a` 97, `A` 65), Enter 13, Backspace 8, Escape 27, Tab 9; Ctrl+letter
/// is its control code (Ctrl+A = 1) as in VB. Arrows, function keys and
/// other non-typing keys raise no `KeyPress`.
pub fn key_ascii(key: &str, typed: Option<&str>, ctrl: bool) -> Option<i16> {
    match key {
        "enter" => return Some(13),
        "backspace" => return Some(8),
        "escape" => return Some(27),
        "tab" => return Some(9),
        _ => {}
    }
    if ctrl {
        let mut chars = key.chars();
        if let (Some(c @ 'a'..='z'), None) = (chars.next(), chars.next()) {
            return Some(c as i16 - 'a' as i16 + 1);
        }
        return None;
    }
    let text = match typed {
        Some(t) => t,
        None if key == "space" => " ",
        None => return None,
    };
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        // VB's KeyAscii is ANSI: characters past 255 have none
        (Some(c), None) if (c as u32) < 256 && !c.is_control() => Some(c as i16),
        _ => None,
    }
}

/// The character a `KeyAscii` types into a field, once the program has
/// seen (and maybe changed) it in `KeyPress`. `None` for 0 (the program
/// swallowed the key) and for control codes, which are keys, not text.
pub fn typed_char(key_ascii: i16) -> Option<char> {
    if key_ascii < 32 {
        return None;
    }
    char::from_u32(key_ascii as u32).filter(|c| !c.is_control())
}

/// The arguments of a mouse event, as VB passes them:
/// `Button As Integer, Shift As Integer, X As Single, Y As Single`, X and
/// Y in the object's own coordinates.
pub fn mouse_args(button: i16, shift: i16, x: f32, y: f32) -> Vec<Variant> {
    vec![
        Variant::Integer(button),
        Variant::Integer(shift),
        Variant::Single(x),
        Variant::Single(y),
    ]
}

/// The arguments of `KeyDown` / `KeyUp`: `KeyCode As Integer, Shift As
/// Integer`.
pub fn key_args(code: i16, shift: i16) -> Vec<Variant> {
    vec![Variant::Integer(code), Variant::Integer(shift)]
}

/// One event for the program: `<object>_<event>` with its arguments
/// (without a control array's `Index`, which the session puts first).
#[derive(Debug, Clone)]
pub struct Event {
    /// The control's index in the form, or `None` for the form itself.
    pub control: Option<usize>,
    pub name: &'static str,
    pub args: Vec<Variant>,
}

impl Event {
    pub fn on(control: Option<usize>, name: &'static str, args: Vec<Variant>) -> Event {
        Event {
            control,
            name,
            args,
        }
    }
}

/// The events a mouse press raises in VB, in order: a press on a control
/// that takes the focus moves the focus first (`LostFocus` of the old
/// control, `GotFocus` of the new), then `MouseDown` — or, for the second
/// press of a double click, `DblClick` in its place (VB's order for a
/// double click is MouseDown, MouseUp, Click, DblClick, MouseUp).
pub fn press(
    target: Option<usize>,
    old_focus: Option<usize>,
    new_focus: Option<usize>,
    args: Vec<Variant>,
    double: bool,
) -> Vec<Event> {
    let mut out = focus_change(old_focus, new_focus);
    if double {
        out.push(Event::on(target, "DblClick", vec![]));
    } else {
        out.push(Event::on(target, "MouseDown", args));
    }
    out
}

/// The events of a release over `target`: `MouseUp`, then `Click` when
/// the release clicks it (pressed and released over it with the left
/// button, and not the end of a double click).
pub fn release(target: Option<usize>, args: Vec<Variant>, click: bool) -> Vec<Event> {
    let mut out = vec![Event::on(target, "MouseUp", args)];
    if click {
        out.push(Event::on(target, "Click", vec![]));
    }
    out
}

/// `LostFocus` of the control losing the keyboard, then `GotFocus` of the
/// one getting it; nothing when the focus doesn't move.
pub fn focus_change(old: Option<usize>, new: Option<usize>) -> Vec<Event> {
    if old == new {
        return vec![];
    }
    let mut out = vec![];
    if let Some(o) = old {
        out.push(Event::on(Some(o), "LostFocus", vec![]));
    }
    if let Some(n) = new {
        out.push(Event::on(Some(n), "GotFocus", vec![]));
    }
    out
}

/// Where Tab (Shift-Tab) moves the focus: the next (previous) control in
/// `focusable`, which lists in tab order the controls that can take it
/// (VB's `TabIndex` order: here the order they were added). Wraps.
pub fn tab_target(focusable: &[usize], current: Option<usize>, back: bool) -> Option<usize> {
    if focusable.is_empty() {
        return None;
    }
    let pos = current.and_then(|c| focusable.iter().position(|&f| f == c));
    let n = focusable.len();
    let next = match (pos, back) {
        (None, false) => 0,
        (None, true) => n - 1,
        (Some(p), false) => (p + 1) % n,
        (Some(p), true) => (p + n - 1) % n,
    };
    Some(focusable[next])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(evs: Vec<Event>) -> Vec<(Option<usize>, &'static str)> {
        evs.into_iter().map(|e| (e.control, e.name)).collect()
    }

    #[test]
    fn key_codes_are_vbs() {
        assert_eq!(key_code("a"), Some(65), "vbKeyA, whatever the case");
        assert_eq!(key_code("A"), Some(65));
        assert_eq!(key_code("7"), Some(55), "vbKey7");
        assert_eq!(key_code("enter"), Some(13), "vbKeyReturn");
        assert_eq!(key_code("escape"), Some(27));
        assert_eq!(key_code("left"), Some(37));
        assert_eq!(key_code("delete"), Some(46));
        assert_eq!(key_code("f1"), Some(112));
        assert_eq!(key_code("f12"), Some(123));
        assert_eq!(key_code("space"), Some(32));
        assert_eq!(key_code("f99"), None);
        assert_eq!(key_code("capslock"), None);
    }

    #[test]
    fn key_ascii_is_the_typed_character() {
        assert_eq!(key_ascii("a", Some("a"), false), Some(97));
        assert_eq!(key_ascii("a", Some("A"), false), Some(65), "with Shift");
        assert_eq!(key_ascii("enter", None, false), Some(13));
        assert_eq!(key_ascii("backspace", None, false), Some(8));
        assert_eq!(key_ascii("space", None, false), Some(32));
        assert_eq!(key_ascii("a", None, true), Some(1), "Ctrl+A");
        assert_eq!(key_ascii("left", None, false), None, "arrows type nothing");
        assert_eq!(key_ascii("f1", None, false), None);
        assert_eq!(key_ascii("x", Some("\u{263A}"), false), None, "not ANSI");
    }

    #[test]
    fn a_changed_key_ascii_types_its_character_and_zero_types_nothing() {
        assert_eq!(typed_char(65), Some('A'));
        assert_eq!(typed_char(32), Some(' '));
        assert_eq!(typed_char(0), None);
        assert_eq!(typed_char(13), None, "Enter adds nothing to one line");
        assert_eq!(typed_char(8), None, "Backspace is handled as a key");
    }

    #[test]
    fn shift_is_a_mask() {
        assert_eq!(shift_mask(false, false, false), 0);
        assert_eq!(shift_mask(true, false, false), SHIFT_MASK);
        assert_eq!(shift_mask(false, true, false), CTRL_MASK);
        assert_eq!(shift_mask(true, true, true), 7);
    }

    #[test]
    fn a_press_moves_the_focus_before_mouse_down() {
        let args = mouse_args(LEFT_BUTTON, 0, 3.0, 4.0);
        assert_eq!(
            names(press(Some(2), Some(1), Some(2), args.clone(), false)),
            [
                (Some(1), "LostFocus"),
                (Some(2), "GotFocus"),
                (Some(2), "MouseDown")
            ]
        );
        // a label (takes no focus), or the control that has it already
        assert_eq!(
            names(press(Some(0), Some(1), Some(1), args.clone(), false)),
            [(Some(0), "MouseDown")]
        );
        // the second press of a double click
        assert_eq!(
            names(press(Some(2), Some(2), Some(2), args, true)),
            [(Some(2), "DblClick")]
        );
    }

    #[test]
    fn a_release_is_mouse_up_then_click() {
        let args = mouse_args(LEFT_BUTTON, 0, 3.0, 4.0);
        assert_eq!(
            names(release(Some(2), args.clone(), true)),
            [(Some(2), "MouseUp"), (Some(2), "Click")]
        );
        // released somewhere else than pressed, or a double click's end
        assert_eq!(names(release(None, args, false)), [(None, "MouseUp")]);
    }

    #[test]
    fn tab_moves_through_the_controls_in_order_and_wraps() {
        let f = [1, 3, 4];
        assert_eq!(tab_target(&f, Some(1), false), Some(3));
        assert_eq!(tab_target(&f, Some(4), false), Some(1), "wraps");
        assert_eq!(
            tab_target(&f, Some(1), true),
            Some(4),
            "Shift-Tab wraps back"
        );
        assert_eq!(tab_target(&f, None, false), Some(1));
        assert_eq!(tab_target(&[], None, false), None);
    }
}
