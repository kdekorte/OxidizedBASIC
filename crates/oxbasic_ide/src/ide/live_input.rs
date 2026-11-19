//! The user's input in a running form's window, raised as the program's
//! events in VB's order: the mouse (`MouseDown`, `MouseMove`, `MouseUp`,
//! `Click` on release, `DblClick`), the keyboard focus (`GotFocus`,
//! `LostFocus`, Tab moving it), keys (`KeyDown`, `KeyPress`, `KeyUp`, the
//! form's first with `KeyPreview`), scrolling a list (`Scroll`), and the
//! window itself (`Activate`, `Deactivate`, `Resize`, `Paint`, closing it
//! through `QueryUnload` / `Unload`).
//!
//! What each event means and its arguments are in [`oxbasic_ui::events`];
//! this file connects them to GPUI's input.

use super::debug_session::{EventArg, KeyInput};
use super::*;
use oxbasic_ui::events::{self as ev, Event};

/// A mouse button held on an object of the form: it has captured the
/// mouse, as in VB, until the button comes up.
#[derive(Clone, Copy, Debug)]
pub struct Press {
    /// The control the button went down on, or `None` for the form.
    pub target: Option<usize>,
    /// The button, as VB's `Button` (1 left, 2 right, 4 middle).
    pub button: i16,
    /// The release clicks the object: the press was on a part that clicks
    /// on release (a button's face; not a list row, which picks on press),
    /// and wasn't the second press of a double click.
    pub clicks: bool,
}

/// VB's `Button` for a GPUI mouse button.
fn button_of(b: gpui::MouseButton) -> i16 {
    match b {
        gpui::MouseButton::Right => ev::RIGHT_BUTTON,
        gpui::MouseButton::Middle => ev::MIDDLE_BUTTON,
        _ => ev::LEFT_BUTTON,
    }
}

/// VB's `Shift` mask for GPUI's modifiers (⌘ counts as Ctrl on a Mac).
pub(super) fn shift_of(m: &gpui::Modifiers) -> i16 {
    ev::shift_mask(m.shift, m.control || m.platform, m.alt)
}

fn args_of(a: Vec<oxbasic_runtime::Variant>) -> Vec<EventArg> {
    use oxbasic_runtime::Variant;
    a.into_iter()
        .map(|v| match v {
            Variant::Single(x) => EventArg::Single(x),
            other => EventArg::Integer(other.as_i64().unwrap_or(0) as i16),
        })
        .collect()
}

/// Whether a control of `kind` raises `event` at all (VB's table).
fn raises(kind: Option<ControlKind>, event: &str) -> bool {
    let name = kind.map_or("Form", |k| k.basic_name());
    oxbasic_ui::event_params(name, event).is_some()
}

impl FormRunner {
    fn kind(&self, target: Option<usize>) -> Option<ControlKind> {
        target.map(|i| self.form.controls[i].kind)
    }

    /// Whether the program handles `object`'s `event`. Until the program
    /// has said which handlers it has, every event counts as handled.
    pub(super) fn handles(&self, control: Option<usize>, event: &str) -> bool {
        let Some(set) = &self.handlers else {
            return raises(self.kind(control), event);
        };
        let object = match control {
            Some(i) => self.form.controls[i].name.to_lowercase(),
            None => "form".to_string(),
        };
        set.contains(&(object, event.to_ascii_lowercase()))
    }

    /// Send the program one event, if it has a handler for it.
    pub(super) fn send(&self, e: Event) {
        if !self.handles(e.control, e.name) {
            return;
        }
        self.session.send(DebugCommand::Raise {
            form: self.form_index,
            control: e.control,
            event: e.name.to_string(),
            args: args_of(e.args),
        });
    }

    /// The pointer's position in `target`'s own coordinates (VB's X and Y
    /// are relative to the control, or to the inside of the form).
    fn local(&self, target: Option<usize>, pos: gpui::Point<gpui::Pixels>) -> (f32, f32) {
        let (x, y) = (f32::from(pos.x), f32::from(pos.y));
        match target.filter(|&i| i < self.form.controls.len()) {
            Some(i) => {
                let (left, top) = self.form.form_origin(i);
                (x - left, y - top)
            }
            None => (x, y),
        }
    }

    /// The enabled control under window point `pos`, topmost first;
    /// `None` is the form (a disabled control passes the mouse to it).
    pub(super) fn hit(&self, pos: gpui::Point<gpui::Pixels>) -> Option<usize> {
        let (x, y) = (f32::from(pos.x), f32::from(pos.y));
        let f = &self.form;
        oxbasic_ui::gpui_controls::z_order(f)
            .into_iter()
            .rev()
            .find(|&i| {
                let c = &f.controls[i];
                let (left, top) = f.form_origin(i);
                f.shown(i)
                    && c.kind != ControlKind::Timer
                    && x >= left
                    && x < left + c.rect.width
                    && y >= top
                    && y < top + c.rect.height
            })
            .filter(|&i| f.usable(i))
    }

    /// The controls Tab stops on, in the form's `TabIndex` order.
    fn focusable(&self) -> Vec<usize> {
        self.form.tab_stops()
    }

    /// Move the keyboard focus to `to` (`None`: the form itself), raising
    /// `LostFocus` of the control losing it and `GotFocus` of the new one.
    pub(super) fn set_focused(&mut self, to: Option<usize>, win: &mut Window) {
        match to.and_then(|i| self.focus.get(&i)) {
            Some(handle) => win.focus(handle),
            None => win.focus(&self.form_focus),
        }
        let from = std::mem::replace(&mut self.focused, to);
        for e in ev::focus_change(from, to) {
            self.send(e);
        }
    }
}

/// The mouse.
impl FormRunner {
    /// A button went down on `target`. VB: the focus moves to the control
    /// if it takes it, then `MouseDown`; the object captures the mouse
    /// until the button comes up. `clicks`: the release will click it
    /// (see [`Press::clicks`]). A double click's second press raises
    /// `DblClick` instead of waiting for a click.
    pub(super) fn mouse_down(
        &mut self,
        target: Option<usize>,
        clicks: bool,
        ev: &MouseDownEvent,
        win: &mut Window,
    ) {
        let takes_focus = target.is_some_and(|i| {
            let c = &self.form.controls[i];
            c.kind.can_focus() && c.enabled
        });
        let new_focus = if takes_focus { target } else { self.focused };
        match new_focus.and_then(|i| self.focus.get(&i)) {
            Some(handle) => win.focus(handle),
            None => win.focus(&self.form_focus),
        }
        let old_focus = std::mem::replace(&mut self.focused, new_focus);
        let button = button_of(ev.button);
        let (x, y) = self.local(target, ev.position);
        // VB's double click: MouseDown, MouseUp, Click — then DblClick and
        // MouseUp for the second press, which clicks no more. Windows sends
        // a double click only for the second click of a burst (the third
        // starts a single again, the fourth is a double), while the count
        // here keeps rising: so the even counts are the doubles.
        let double = ev.click_count >= 2 && ev.click_count % 2 == 0 && button == ev::LEFT_BUTTON;
        let args = ev::mouse_args(button, shift_of(&ev.modifiers), x, y);
        for e in ev::press(target, old_focus, new_focus, args, double) {
            self.send(e);
        }
        self.pressed = Some(Press {
            target,
            button,
            clicks: clicks && !double,
        });
    }

    /// The pointer moved. While a button is held the object it went down
    /// on gets the move (mouse capture); otherwise the object under it.
    pub(super) fn mouse_move(&mut self, ev: &gpui::MouseMoveEvent) {
        let (target, button) = match (&self.pressed, ev.pressed_button) {
            (Some(p), Some(_)) => (p.target, p.button),
            // the button came up outside the window: the capture is over
            (Some(_), None) => {
                self.pressed = None;
                (self.hit(ev.position), 0)
            }
            (None, _) => (self.hit(ev.position), 0),
        };
        if !self.handles(target, "MouseMove") {
            return;
        }
        let (x, y) = self.local(target, ev.position);
        self.send(Event::on(
            target,
            "MouseMove",
            ev::mouse_args(button, shift_of(&ev.modifiers), x, y),
        ));
    }

    /// The button came up: `MouseUp` on the object that captured the
    /// mouse. Returns the object it clicks (`Some(None)` is the form):
    /// released over the object it went down on, with the left button (the
    /// form clicks with any). The caller runs the object's own action and
    /// its `Click`, as VB does, before `MouseUp`.
    pub(super) fn mouse_up(&mut self, ev: &gpui::MouseUpEvent) -> Option<Option<usize>> {
        let p = self.pressed?;
        if button_of(ev.button) != p.button {
            return None;
        }
        self.pressed = None;
        let over = self.hit(ev.position) == p.target;
        let clicks = p.clicks && over && (p.button == ev::LEFT_BUTTON || p.target.is_none());
        clicks.then_some(p.target)
    }

    /// `MouseUp` of the press `ev` ends, sent after the click it caused.
    pub(super) fn send_mouse_up(&self, target: Option<usize>, ev: &gpui::MouseUpEvent) {
        let (x, y) = self.local(target, ev.position);
        self.send(Event::on(
            target,
            "MouseUp",
            ev::mouse_args(button_of(ev.button), shift_of(&ev.modifiers), x, y),
        ));
    }

    /// The scroll wheel over a list: its `Scroll` event.
    pub(super) fn scroll(&self, target: usize) {
        if matches!(
            self.form.controls[target].kind,
            ControlKind::ListBox | ControlKind::ComboBox
        ) {
            self.send(Event::on(Some(target), "Scroll", vec![]));
        }
    }
}

/// The keyboard.
impl FormRunner {
    /// A key pressed with `target` focused (`None`: the form). Tab moves
    /// the focus, as in VB (no `KeyDown` for it). Other keys go to the
    /// program first, the form's handlers first with `KeyPreview`; the
    /// control's own action waits for the answer, so a handler setting
    /// `KeyAscii` to 0 (or another character) changes what is typed.
    /// Returns whether the key was taken.
    pub(super) fn key_down(
        &mut self,
        target: Option<usize>,
        ks: &gpui::Keystroke,
        win: &mut Window,
    ) -> bool {
        let m = &ks.modifiers;
        if ks.key == "tab" && !m.control && !m.platform {
            let to = ev::tab_target(&self.focusable(), self.focused, m.shift);
            if to.is_some() {
                self.set_focused(to, win);
            }
            return true;
        }
        let ctrl = m.control || m.platform;
        let code = ev::key_code(&ks.key);
        let ascii = ev::key_ascii(&ks.key, ks.key_char.as_deref(), ctrl);
        let key = KeyInput {
            code,
            shift: shift_of(m),
            ascii,
        };
        let wanted = |ev: &str| {
            self.handles(target, ev) || (self.form.key_preview && self.handles(None, ev))
        };
        let asks = (code.is_some() && wanted("KeyDown")) || (ascii.is_some() && wanted("KeyPress"));
        let send_key = |this: &mut FormRunner, control: Option<usize>| {
            this.key_serial += 1;
            this.session.send(DebugCommand::Key {
                form: this.form_index,
                control,
                key,
                serial: this.key_serial,
            });
            this.key_serial
        };
        let Some(idx) = target else {
            // the form itself: there is no control action to wait for
            if asks {
                send_key(self, None);
            }
            return asks;
        };
        if !asks {
            // nobody listens: the control acts on the key at once
            return self.key(idx, ks);
        }
        if self.handlers.is_none() {
            // which handlers the program has isn't known (a window run
            // without the IDE): raise the events, but don't wait for them
            send_key(self, Some(idx));
            return self.key(idx, ks);
        }
        let serial = send_key(self, Some(idx));
        self.pending_keys.insert(serial, (idx, ks.clone()));
        true
    }

    /// The program answered key `serial`: apply it to its control unless a
    /// handler cancelled it, typing `typed` (the handler's `KeyAscii`) in
    /// place of the key's own character.
    pub(super) fn key_answered(&mut self, serial: u64, keep: bool, typed: Option<char>) -> bool {
        let Some((idx, mut ks)) = self.pending_keys.remove(&serial) else {
            return false;
        };
        if !keep {
            return false;
        }
        if let Some(c) = typed {
            ks.key_char = Some(c.to_string());
            if ks.key.chars().count() == 1 {
                ks.key = c.to_lowercase().to_string();
            }
        }
        self.key(idx, &ks)
    }

    /// A key released: `KeyUp`.
    pub(super) fn key_up(&self, target: Option<usize>, ks: &gpui::Keystroke) {
        let Some(code) = ev::key_code(&ks.key) else {
            return;
        };
        let wanted = self.handles(target, "KeyUp")
            || (self.form.key_preview && target.is_some() && self.handles(None, "KeyUp"));
        if !wanted {
            return;
        }
        self.session.send(DebugCommand::KeyUp {
            form: self.form_index,
            control: target,
            code,
            shift: shift_of(&ks.modifiers),
        });
    }
}

/// The window.
impl FormRunner {
    /// The window became active or stopped being: `Form_Activate` /
    /// `Form_Deactivate` (VB raises them when the focus moves between the
    /// application's forms).
    pub(super) fn activation_changed(&mut self, active: bool) {
        if active == self.active {
            return;
        }
        self.active = active;
        let name = if active { "Activate" } else { "Deactivate" };
        self.send(Event::on(None, name, vec![]));
        // a window brought to the front may have been covered: GPUI says
        // nothing of that, but raising a window activates it, so it is
        // painted again then
        if active {
            self.paint_all();
        }
    }

    /// `Form_Paint`, then each visible PictureBox's `Paint`: the window's
    /// contents are to be drawn again.
    fn paint_all(&self) {
        self.send(Event::on(None, "Paint", vec![]));
        for (i, c) in self.form.controls.iter().enumerate() {
            if c.kind == ControlKind::PictureBox && c.visible {
                self.send(Event::on(Some(i), "Paint", vec![]));
            }
        }
    }

    /// The window's inside is `size`: when it changed, the form's `Width`
    /// and `Height` follow (the program reads them), then `Form_Resize`
    /// and the `Paint` events run, as VB raises them (also as the form
    /// first shows).
    pub(super) fn window_resized(&mut self, size: (f32, f32)) {
        if self.last_size == Some(size) {
            return;
        }
        self.last_size = Some(size);
        self.session.send(DebugCommand::Edit(
            self.form_index,
            UserEdit::FormSize {
                width: size.0,
                height: size.1,
            },
        ));
        self.send(Event::on(None, "Resize", vec![]));
        self.paint_all();
    }
}

impl Render for FormRunner {
    fn render(&mut self, win: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // the window's size: Resize (and the first Paint) when it changes
        let size = win.viewport_size();
        self.window_resized((f32::from(size.width), f32::from(size.height)));
        let mut area = div()
            .id("form-runner")
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(FORM_BACK))
            .text_color(rgb(FG))
            .track_focus(&self.form_focus)
            // a press on no control is the form's; it closes any open list
            .on_any_mouse_down(cx.listener(
                |this: &mut FormRunner, ev: &MouseDownEvent, win, cx| {
                    this.close_lists(None);
                    this.mouse_down(None, true, ev, win);
                    cx.notify();
                },
            ))
            // capture: moves and the release go to the pressed object
            // wherever the pointer is, so the form takes them all (a
            // release outside the window ends the capture in mouse_move)
            .on_mouse_move(cx.listener(
                |this: &mut FormRunner, ev: &gpui::MouseMoveEvent, _w, _cx| {
                    this.mouse_move(ev);
                },
            ))
            .capture_any_mouse_up(cx.listener(
                |this: &mut FormRunner, ev: &gpui::MouseUpEvent, _w, cx| {
                    this.release(ev);
                    cx.notify();
                },
            ))
            // keys with no control focused are the form's
            .on_key_down(
                cx.listener(|this: &mut FormRunner, ev: &KeyDownEvent, win, cx| {
                    if this.focused.is_none() && this.key_down(None, &ev.keystroke, win) {
                        cx.notify();
                    }
                }),
            )
            .on_key_up(
                cx.listener(|this: &mut FormRunner, ev: &gpui::KeyUpEvent, _w, _cx| {
                    if this.focused.is_none() {
                        this.key_up(None, &ev.keystroke);
                    }
                }),
            );
        let form = self.form.clone();
        for idx in oxbasic_ui::gpui_controls::z_order(&form) {
            let c = &form.controls[idx];
            // a control in a hidden frame is hidden too
            if !form.shown(idx) || c.kind == ControlKind::Timer {
                continue;
            }
            let focus = self.focus.get(&idx).cloned();
            let focused = focus.as_ref().is_some_and(|f| f.contains_focused(win, cx));
            let mut look = ControlLook::running(&self.state[idx], form.form_origin(idx), focused);
            look.base_dir = self.base_dir.clone();
            // in a disabled frame, a control takes no input either
            let usable = form.usable(idx);
            let mut hook =
                |part: Part, el: gpui::Stateful<gpui::Div>, cx: &mut Context<FormRunner>| {
                    run_part(c, idx, part, el, focus.clone(), usable, cx)
                };
            area = area.child(paint_control(c, &look, &mut hook, cx));
        }
        area
    }
}

/// Wire `part` of control `c` (number `idx`) to the window's input: a
/// debug selector for tests, its focus, presses, keys and the wheel. A
/// disabled control takes no input; its presses fall to the form.
fn run_part(
    c: &ControlSpec,
    idx: usize,
    part: Part,
    el: gpui::Stateful<gpui::Div>,
    focus: Option<FocusHandle>,
    usable: bool,
    cx: &mut Context<FormRunner>,
) -> gpui::Stateful<gpui::Div> {
    let name = match c.index {
        Some(n) => format!("{}({})", c.name, n),
        None => c.name.clone(),
    };
    let mut el = el.debug_selector(move || match part {
        Part::Body => format!("run-{}", name),
        Part::Field => format!("run-{}-field", name),
        Part::Arrow => format!("run-{}-arrow", name),
        Part::ListItem(i) => format!("run-{}-item-{}", name, i),
        Part::DropItem(i) => format!("run-{}-drop-{}", name, i),
    });
    if let (Part::Body, Some(f)) = (part, &focus) {
        el = el.track_focus(f);
    }
    if !usable {
        return el;
    }
    let el = el.on_any_mouse_down(cx.listener(
        move |this: &mut FormRunner, ev: &MouseDownEvent, win, cx| {
            cx.stop_propagation();
            this.press_part(idx, part, ev, win);
            cx.notify();
        },
    ));
    if part != Part::Body {
        return el;
    }
    el.on_key_down(
        cx.listener(move |this: &mut FormRunner, ev: &KeyDownEvent, win, cx| {
            cx.stop_propagation();
            if this.key_down(Some(idx), &ev.keystroke, win) {
                cx.notify();
            }
        }),
    )
    .on_key_up(cx.listener(
        move |this: &mut FormRunner, ev: &gpui::KeyUpEvent, _w, cx| {
            cx.stop_propagation();
            this.key_up(Some(idx), &ev.keystroke);
        },
    ))
    .on_scroll_wheel(cx.listener(
        move |this: &mut FormRunner, _ev: &gpui::ScrollWheelEvent, _w, _cx| {
            this.scroll(idx);
        },
    ))
}

impl FormRunner {
    /// A press on `part` of control `idx`: the focus moves and `MouseDown`
    /// runs, then the control's own action on the press (a row picked, a
    /// list dropped); the release decides the click.
    fn press_part(&mut self, idx: usize, part: Part, ev: &MouseDownEvent, win: &mut Window) {
        let left = ev.button == gpui::MouseButton::Left;
        let clicks = left && self.clicks_on_release(idx, part);
        self.press_mods = ev.modifiers;
        self.mouse_down(Some(idx), clicks, ev, win);
        if left {
            self.act_on_press(idx, part);
        }
        // a click in a text field puts the caret at the end of what it
        // shows (where a click lands in the text isn't measured here),
        // Shift-click selects up to there
        if left && self.form.controls[idx].kind.has_text() && part != Part::Arrow {
            let s = &mut self.state[idx];
            let end = s.text.chars().count();
            if ev.modifiers.shift {
                if s.anchor.is_none() {
                    s.anchor = Some(s.caret);
                }
            } else {
                s.anchor = None;
            }
            s.caret = end;
            if ev.click_count >= 2 {
                // a double click selects everything, as a word select would
                // in a one-word field
                s.anchor = Some(0);
            }
            let (caret, anchor) = (s.caret, s.anchor);
            self.tell(UserEdit::Caret {
                control: idx,
                caret,
                anchor,
            });
        }
    }

    /// A button came up: the click it makes (on the object it went down
    /// on, if it is released over it), then `MouseUp`, in VB's order.
    fn release(&mut self, ev: &gpui::MouseUpEvent) {
        let Some(target) = self.pressed.map(|p| p.target) else {
            return;
        };
        let clicked = self.mouse_up(ev);
        if self.pressed.is_some() {
            return; // another button than the one that was pressed
        }
        match clicked {
            Some(Some(idx)) => self.click(idx),
            Some(None) => self.send(Event::on(None, "Click", vec![])),
            None => {}
        }
        self.send_mouse_up(target, ev);
    }
}
