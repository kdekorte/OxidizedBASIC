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
use oxbasic_ui::host::{line_around, word_around};

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
        // a press on a TabControl's strip selects the tab under it, as VB does
        if let (Some(i), ev::LEFT_BUTTON) = (target, button) {
            if self.form.controls[i].kind == oxbasic_ui::ControlKind::TabControl
                && y < oxbasic_ui::gpui_controls::TAB_STRIP_H
            {
                if let Some(tab) =
                    oxbasic_ui::gpui_controls::tab_at(&self.form.controls[i].tabs, x)
                {
                    self.tell(UserEdit::Tab {
                        control: i,
                        tab: tab as u8,
                    });
                }
            }
        }
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
        // a button held in a text field drags out its selection
        if ev.pressed_button.is_none() {
            self.text_drag = None;
        }
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

    /// Typing into a focused DataGrid edits its current cell: the first
    /// character starts it over the cell's text. Enter and Tab keep the text,
    /// Escape puts the old text back; a commit sends `GridCell` if it changed.
    /// Returns whether the key was taken.
    fn cell_key(&mut self, idx: usize, ks: &gpui::Keystroke) -> bool {
        let c = self.form.controls[idx].clone();
        if c.kind != ControlKind::DataGrid || !c.enabled {
            return false;
        }
        let m = &ks.modifiers;
        if m.control || m.platform || m.alt {
            return false;
        }
        let key = ks.key.as_str();
        // an edit is open: it takes the keys
        if let Some((ci, edit)) = self.cell_edit.clone() {
            if ci == idx {
                return self.edit_cell_key(idx, edit, ks);
            }
        }
        // no edit: a printable key starts one over the current cell
        let typed = ks_text(ks);
        if let Some(text) = typed.clone().filter(|t| !t.is_empty()) {
            if let Some(mut edit) = oxbasic_ui::grid_edit::CellEdit::begin(
                &c.grid,
                c.grid.row,
                c.grid.col,
                true,
            ) {
                edit.insert(&text);
                self.tell_editing(idx, &edit);
                self.cell_edit = Some((idx, edit));
                return true;
            }
        }
        // F2 edits the text as it is, the caret at its end
        if key == "f2" {
            if let Some(edit) = oxbasic_ui::grid_edit::CellEdit::begin(
                &c.grid,
                c.grid.row,
                c.grid.col,
                false,
            ) {
                self.tell_editing(idx, &edit);
                self.cell_edit = Some((idx, edit));
                return true;
            }
        }
        false
    }

    /// The text being typed into the cell reaches the program now, so its
    /// ValidateEdit reads it as the grid's Text.
    fn tell_editing(&self, idx: usize, edit: &oxbasic_ui::grid_edit::CellEdit) {
        self.tell(UserEdit::GridEditing { control: idx, text: Some(edit.text.clone()) });
    }

    /// One key into the open cell edit `edit` of control `idx`.
    fn edit_cell_key(
        &mut self,
        idx: usize,
        mut edit: oxbasic_ui::grid_edit::CellEdit,
        ks: &gpui::Keystroke,
    ) -> bool {
        use oxbasic_ui::grid_edit::EditEnd;
        let key = ks.key.as_str();
        let end = match key {
            "enter" | "tab" => Some(EditEnd::Commit),
            "escape" => Some(EditEnd::Cancel),
            _ => None,
        };
        if let Some(end) = end {
            self.cell_edit = None;
            let text = edit.result(end).to_string();
            let (row, col) = (edit.row, edit.col);
            let original = edit.original.clone();
            if text != original {
                self.tell(UserEdit::GridCell { control: idx, row, col, text });
            }
            return true;
        }
        match key {
            "backspace" => {
                edit.backspace();
            }
            "delete" => {
                edit.delete();
            }
            "left" | "right" | "home" | "end" => {
                edit.move_caret(key);
            }
            _ => {
                if let Some(text) = ks_text(ks).filter(|t| !t.is_empty()) {
                    edit.insert(&text);
                }
            }
        }
        self.tell_editing(idx, &edit);
        self.cell_edit = Some((idx, edit));
        true
    }

    /// A navigation key with a DataGrid focused moves its current cell (the
    /// arrows, Tab, Enter, the paging keys), as `grid_edit::move_by_key`
    /// says. Returns whether the key was taken.
    fn grid_key(&mut self, idx: usize, ks: &gpui::Keystroke) -> bool {
        let c = self.form.controls[idx].clone();
        if c.kind != ControlKind::DataGrid || !c.enabled {
            return false;
        }
        let m = &ks.modifiers;
        if m.control || m.platform || m.alt {
            return false;
        }
        let (w, h) = oxbasic_ui::grid_layout::inside(&c.rect);
        let layout = oxbasic_ui::grid_layout::GridLayout::new(&c.grid, w, h);
        // a page is the scrolling rows in view
        let page = layout.rows.len().saturating_sub(c.grid.fixed_rows).max(1);
        let key = ks.key.as_str();
        let Some((row, col)) = oxbasic_ui::grid_edit::move_by_key(&c.grid, key, m.shift, page)
        else {
            return false;
        };
        if (row, col) != (c.grid.row, c.grid.col) {
            self.tell(UserEdit::GridSelect { control: idx, row, col });
        }
        true
    }

    /// A left press on a DataGrid's cells makes that cell the current one:
    /// the program is told (`LeaveCell`, `EnterCell`, `RowColChange`).
    fn grid_press(&mut self, idx: usize, part: Part, ev: &MouseDownEvent) {
        let c = self.form.controls[idx].clone();
        if c.kind != ControlKind::DataGrid || part != Part::Body || !c.enabled {
            return;
        }
        let (x, y) = self.local(Some(idx), ev.position);
        let (w, h) = oxbasic_ui::grid_layout::inside(&c.rect);
        let layout = oxbasic_ui::grid_layout::GridLayout::new(&c.grid, w, h);
        // the cells sit inside the control's border, 1 pixel in
        if let Some((row, col)) = layout.cell_at(x - 1.0, y - 1.0) {
            self.tell(UserEdit::GridSelect { control: idx, row, col });
        }
    }

    /// The scroll wheel over a DataGrid: its view moves up or down a row per
    /// notch, the program is told (its `Scroll` follows if the view moved).
    pub(super) fn grid_wheel(&self, target: usize, ev: &gpui::ScrollWheelEvent) {
        let c = &self.form.controls[target];
        if c.kind != ControlKind::DataGrid || !c.enabled {
            return;
        }
        let dy = f32::from(ev.delta.pixel_delta(gpui::px(16.0)).y);
        if dy == 0.0 {
            return;
        }
        let (w, h) = oxbasic_ui::grid_layout::inside(&c.rect);
        let max = oxbasic_ui::grid_layout::GridLayout::new(&c.grid, w, h).max_top_row;
        let top = c.grid.top_row;
        let to = if dy < 0.0 { (top + 1).min(max) } else { top.saturating_sub(1) };
        self.tell(UserEdit::GridView {
            control: target,
            top_row: to,
            left_col: c.grid.left_col,
        });
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
        if let Some(idx) = target {
            if self.cell_key(idx, ks) || self.grid_key(idx, ks) {
                return true;
            }
        }
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
    /// A menu item of this form was chosen: run its `Name_Click` handler in
    /// the program. Disabled items don't run.
    pub(super) fn menu_clicked(&mut self, index: usize) {
        let Some(item) = self.form.menus.get(index).cloned() else {
            return;
        };
        if !item.enabled || item.is_separator() {
            return;
        }
        let handler = oxbasic_ui::menu::click_handler(&item);
        self.session
            .send(DebugCommand::Call(self.form_index, handler, Vec::new()));
    }

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
            // a menu item of the form's bar was chosen: run its Name_Click
            .on_action(cx.listener(
                |this: &mut FormRunner, act: &oxbasic_ui::menu_bar::FormMenuClick, _win, _cx| {
                    this.menu_clicked(act.index);
                },
            ))
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
                |this: &mut FormRunner, ev: &gpui::MouseMoveEvent, win, cx| {
                    this.mouse_move(ev);
                    if this.drag_selection(ev.position, win) || this.drag_scroll(ev.position, win) {
                        cx.notify();
                    }
                },
            ))
            .capture_any_mouse_up(cx.listener(
                |this: &mut FormRunner, ev: &gpui::MouseUpEvent, _w, cx| {
                    this.text_drag = None;
                    this.scroll_drag = None;
                    this.scroll_start = None;
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
            look.form_size = Some((form.width, form.height));
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
        move |this: &mut FormRunner, ev: &gpui::ScrollWheelEvent, _w, _cx| {
            this.scroll(idx);
            this.grid_wheel(idx, ev);
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
            self.grid_press(idx, part, ev);
        }
        // a press on a scrollbar starts a drag from the value it has: each
        // move adds the pointer's travel, so the window's offset cancels
        if left && self.form.controls[idx].kind.is_draggable() && self.form.controls[idx].enabled {
            self.scroll_drag = Some(idx);
            self.scroll_start = Some((ev.position, self.state[idx].value));
        }
        // a click in a text field puts the caret under the pointer (measured
        // in the field's font); Shift-click selects up to there, a double
        // click selects the word, a triple click the whole line or text; the
        // press starts a drag that extends the selection
        let c = &self.form.controls[idx];
        if left && c.kind.has_text() && part != Part::Arrow && c.kind != ControlKind::ListBox {
            let at = self.text_index(idx, ev.position, win);
            let s = &mut self.state[idx];
            let text = s.text.clone();
            match ev.click_count {
                0 | 1 => {
                    if ev.modifiers.shift {
                        if s.anchor.is_none() {
                            s.anchor = Some(s.caret);
                        }
                    } else {
                        s.anchor = Some(at);
                    }
                    s.caret = at;
                    self.text_drag = Some(idx);
                }
                2 => {
                    let (a, b) = word_around(&text, at);
                    s.anchor = Some(a);
                    s.caret = b;
                    self.text_drag = Some(idx);
                }
                _ => {
                    let (a, b) = line_around(&text, at);
                    s.anchor = Some(a);
                    s.caret = b;
                }
            }
            if s.anchor == Some(s.caret) {
                s.anchor = None;
            }
            self.tell_caret(idx);
        }
    }

    /// Which character of text field `idx` the window point `pos` is on.
    fn text_index(&self, idx: usize, pos: gpui::Point<gpui::Pixels>, win: &Window) -> usize {
        let (left, top) = self.form.form_origin(idx);
        let at = gpui::point(pos.x - gpui::px(left), pos.y - gpui::px(top));
        oxbasic_ui::gpui_text_hit::index_at(
            win,
            &self.form.controls[idx],
            &self.state[idx].text,
            at,
        )
    }

    /// Tell the program where the caret and selection of field `idx` are
    /// (`SelStart`, `SelLength`).
    pub(super) fn tell_caret(&self, idx: usize) {
        self.tell(UserEdit::Caret {
            control: idx,
            caret: self.state[idx].caret,
            anchor: self.state[idx].anchor,
        });
    }

    /// The pointer moved with the button held in a text field: the selection
    /// grows to the character under it. Returns whether it changed.
    /// Set the scrollbar being dragged from the pointer's movement along its
    /// track. A press on the thumb or track starts the drag at the value it
    /// had; each move adds the pointer's travel, scaled to the track's length
    /// and the range. Both points are window positions, so the window's own
    /// offset cancels out. Tells the program only when the value changes.
    pub(super) fn drag_scroll(&mut self, pos: gpui::Point<gpui::Pixels>, _win: &Window) -> bool {
        let Some(idx) = self.scroll_drag else {
            return false;
        };
        let Some(c) = self.form.controls.get(idx).cloned() else {
            self.scroll_drag = None;
            return false;
        };
        let Some(start) = self.scroll_start else {
            return false;
        };
        let horizontal = match c.kind {
            ControlKind::HScrollBar => true,
            ControlKind::Slider => c.orientation == 0,
            _ => false,
        };
        let (travel, length) = if horizontal {
            (f32::from(pos.x) - f32::from(start.0.x), c.rect.width)
        } else {
            (f32::from(pos.y) - f32::from(start.0.y), c.rect.height)
        };
        let range = (c.scroll_max - c.scroll_min) as f32;
        let value = (start.1 as f32 + travel / length.max(1.0) * range)
            .round()
            .clamp(c.scroll_min as f32, c.scroll_max as f32) as i32;
        if self.state[idx].value == value {
            return false;
        }
        self.state[idx].value = value;
        self.tell(UserEdit::Scroll {
            control: idx,
            value,
        });
        true
    }

    pub(super) fn drag_selection(&mut self, pos: gpui::Point<gpui::Pixels>, win: &Window) -> bool {
        let Some(idx) = self.text_drag else {
            return false;
        };
        if idx >= self.form.controls.len() {
            self.text_drag = None;
            return false;
        }
        let at = self.text_index(idx, pos, win);
        let s = &mut self.state[idx];
        if s.caret == at {
            return false;
        }
        if s.anchor.is_none() {
            s.anchor = Some(s.caret);
        }
        s.caret = at;
        if s.anchor == Some(at) {
            s.anchor = None;
        }
        self.tell_caret(idx);
        true
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
