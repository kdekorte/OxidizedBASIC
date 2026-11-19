//! The GPUI binding engine (Phase 2): renders a classic BASIC
//! [`FormSpec`] as a real GPU-accelerated GPUI window and routes GPUI
//! input events to BASIC subroutines through the [`FormDispatcher`].
//!
//! Controls are drawn by [`crate::gpui_controls`] — the same code the
//! IDE's designer and run window use — and behave as in VB6:
//!
//! | Control | Run-time behaviour |
//! |---------|--------------------|
//! | `CommandButton` | `Click` |
//! | `Label` | `Click` |
//! | `TextBox` | click to focus, type: `Change` per edit; `Click` |
//! | `ListBox` | click an item to select it: `Click` |
//! | `ComboBox` | ▾ drops the list (`DropDown`); picking an item sets `Text` (`Click`); Styles 0/1 accept typing (`Change`) |
//! | `Frame`, `PictureBox` | `Click` |
//! | `Timer` | async background timer task -> `Timer` |

use crate::gpui_controls::{
    edit_text, paint_control, z_order, ControlLook, ControlState, Part, FORM_BACK,
};
use crate::{ControlKind, FormDispatcher, FormSpec};
use gpui::{
    div, prelude::*, px, size, App, Application, Bounds, Context, FocusHandle, KeyDownEvent,
    MouseButton, MouseDownEvent, SharedString, Window, WindowBounds, WindowOptions,
};
use std::collections::HashMap;

/// The window root: owns the form, the dispatcher, and the closure that
/// invokes BASIC subs (`runtime.evaluate_sub`).
pub struct FormWindow {
    form: FormSpec,
    dispatcher: FormDispatcher,
    /// Invokes a BASIC sub by name.
    evaluate: Box<dyn FnMut(&str) -> Result<(), String>>,
    /// Run-time state (text, selection, open list) per control.
    state: Vec<ControlState>,
    /// Focus handles per control that takes the keyboard.
    focus: HashMap<usize, FocusHandle>,
}

impl FormWindow {
    pub fn new(
        form: FormSpec,
        dispatcher: FormDispatcher,
        evaluate: Box<dyn FnMut(&str) -> Result<(), String>>,
        cx: &mut Context<Self>,
    ) -> FormWindow {
        let state = form.controls.iter().map(ControlState::new).collect();
        let mut focus = HashMap::new();
        for (i, c) in form.controls.iter().enumerate() {
            if matches!(
                c.kind,
                ControlKind::TextBox | ControlKind::ComboBox | ControlKind::ListBox
            ) {
                focus.insert(i, cx.focus_handle());
            }
        }

        // Async timers (spec §5: Timer -> non-blocking async loop).
        for c in &form.controls {
            if c.kind == ControlKind::Timer && c.enabled && c.interval_ms > 0 {
                let name = c.name.clone();
                let interval = std::time::Duration::from_millis(c.interval_ms);
                cx.spawn(async move |this, cx| loop {
                    cx.background_executor().timer(interval).await;
                    if this
                        .update(cx, |this, cx| {
                            this.fire(&name, "Timer");
                            cx.notify();
                        })
                        .is_err()
                    {
                        break; // window closed
                    }
                })
                .detach();
            }
        }

        let mut window = FormWindow {
            form,
            dispatcher,
            evaluate,
            state,
            focus,
        };
        // Form_Load fires when the form comes up (classic VB lifecycle):
        // both the conventional `Form_Load` and `<FormName>_Load`.
        let form_name = window.form.name.clone();
        window.fire("Form", "Load");
        window.fire(&form_name, "Load");
        window
    }

    /// Dispatch a UI event to the BASIC program.
    fn fire(&mut self, control: &str, event: &str) {
        let evaluate = &mut self.evaluate;
        if let Err(e) = self.dispatcher.dispatch(control, event, evaluate) {
            eprintln!("oxbasic: {}", e);
        }
    }

    /// Close every dropped-down combo list except `keep`'s.
    fn close_lists(&mut self, keep: Option<usize>) {
        for (i, s) in self.state.iter_mut().enumerate() {
            if Some(i) != keep {
                s.dropped = false;
            }
        }
    }
}

impl Render for FormWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .id("form-window")
            .relative()
            .size_full()
            .bg(gpui::rgb(FORM_BACK))
            .text_color(gpui::rgb(0x1D1D1F))
            // a click on the form itself closes any open combo list
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this: &mut FormWindow, _ev: &MouseDownEvent, _win, cx| {
                    this.close_lists(None);
                    cx.notify();
                }),
            );

        let form = self.form.clone();
        for idx in z_order(&form) {
            let c = &form.controls[idx];
            if !form.shown(idx) || c.kind == ControlKind::Timer {
                continue;
            }
            let focused = self
                .focus
                .get(&idx)
                .is_some_and(|f| f.contains_focused(window, cx));
            let look = ControlLook::running(&self.state[idx], form.form_origin(idx), focused);
            let mut hook = |part: Part,
                            el: gpui::Stateful<gpui::Div>,
                            cx: &mut Context<FormWindow>|
             -> gpui::Stateful<gpui::Div> {
                wire(idx, c.kind, part, el, cx, self.focus.get(&idx).cloned())
            };
            root = root.child(paint_control(c, &look, &mut hook, cx));
        }
        root
    }
}

/// Attach the run-time behaviour of `part` of control `idx`.
fn wire(
    idx: usize,
    kind: ControlKind,
    part: Part,
    el: gpui::Stateful<gpui::Div>,
    cx: &mut Context<FormWindow>,
    focus: Option<FocusHandle>,
) -> gpui::Stateful<gpui::Div> {
    let el = match &focus {
        Some(f) if matches!(part, Part::Body) => el.track_focus(f),
        _ => el,
    };
    let focus_click = focus.clone();
    el.on_mouse_down(
        MouseButton::Left,
        cx.listener(
            move |this: &mut FormWindow, _ev: &MouseDownEvent, win, cx| {
                cx.stop_propagation();
                let name = this.form.controls[idx].name.clone();
                if let Some(f) = &focus_click {
                    win.focus(f);
                }
                match part {
                    Part::Arrow | Part::Field if kind == ControlKind::ComboBox => {
                        let editable = this.form.controls[idx].style == 0;
                        // The ▾ button toggles the list; so does the field of a
                        // Dropdown List (Style 2), as in VB.
                        if part == Part::Arrow || !editable {
                            let open = !this.state[idx].dropped;
                            this.close_lists(Some(idx));
                            this.state[idx].dropped = open;
                            if open {
                                this.fire(&name, "DropDown");
                            }
                        } else {
                            this.close_lists(None);
                        }
                    }
                    Part::ListItem(i) | Part::DropItem(i) => {
                        let spec = this.form.controls[idx].clone();
                        let was = this.state[idx].text.clone();
                        this.state[idx].pick(&spec, i);
                        this.close_lists(None);
                        if kind == ControlKind::ComboBox && this.state[idx].text != was {
                            this.fire(&name, "Change");
                        }
                        this.fire(&name, "Click");
                    }
                    _ => {
                        this.close_lists(None);
                        this.fire(&name, "Click");
                    }
                }
                cx.notify();
            },
        ),
    )
    .on_key_down(
        cx.listener(move |this: &mut FormWindow, ev: &KeyDownEvent, _win, cx| {
            let spec = this.form.controls[idx].clone();
            let editable = match kind {
                ControlKind::TextBox => true,
                ControlKind::ComboBox => spec.style != 2,
                _ => false,
            };
            if !editable {
                return;
            }
            if edit_text(&mut this.state[idx].text, &ev.keystroke) {
                this.state[idx].list_index = None;
                this.fire(&spec.name, "Change");
                cx.notify();
            }
        }),
    )
}

/// Open the GPUI window for a form and run the app loop until the window
/// closes. `evaluate` invokes BASIC subs by name (spec §5.1).
pub fn run_form_window(
    form: FormSpec,
    dispatcher: FormDispatcher,
    evaluate: Box<dyn FnMut(&str) -> Result<(), String>>,
) {
    Application::new().run(move |cx: &mut App| {
        let width = form.width;
        let height = form.height;
        let title = form.caption.clone();
        let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(SharedString::from(title)),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| FormWindow::new(form, dispatcher, evaluate, cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
