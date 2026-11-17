//! The GPUI binding engine (Phase 2): renders a classic BASIC
//! [`FormSpec`] as a real GPU-accelerated GPUI window and routes GPUI
//! input events to BASIC subroutines through the [`FormDispatcher`].
//!
//! Classic control → element mapping (spec §5):
//!
//! | BASIC control | GPUI element |
//! |---------------|--------------|
//! | `Form`        | GPUI window, absolutely-positioned children |
//! | `CommandButton` | stateful `div` with hover/active states + `on_click` |
//! | `Label`       | text `div` |
//! | `TextBox`     | focusable `div` with key handling → `Change` events |
//! | `Frame`       | bordered container `div` |
//! | `ListBox`     | column of clickable item rows |
//! | `PictureBox`  | colored `div` |
//! | `Timer`       | async background timer task → `Timer` event |

use crate::{ControlKind, FormDispatcher, FormSpec};
use gpui::{
    App, AppContext as _, Application, Bounds, Context, FocusHandle, KeyDownEvent, SharedString,
    Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use std::collections::HashMap;

/// Classic VB form gray.
const FORM_BG: u32 = 0xECE9D8;
const BUTTON_BG: u32 = 0xE1E1E1;
const BUTTON_BG_HOVER: u32 = 0xD9E4F2;
const BUTTON_BG_ACTIVE: u32 = 0xB8CCE0;
const CONTROL_BORDER: u32 = 0x8E8E8E;
const TEXT_COLOR: u32 = 0x1A1A1A;

/// The window root: owns the form, the dispatcher, and the closure that
/// invokes BASIC subs (`runtime.evaluate_sub`).
pub struct FormWindow {
    form: FormSpec,
    dispatcher: FormDispatcher,
    /// Invokes a BASIC sub by name.
    evaluate: Box<dyn FnMut(&str) -> Result<(), String>>,
    /// Live `Text` content per TextBox control.
    text_values: HashMap<String, String>,
    /// Focus handles per TextBox.
    focus: HashMap<String, FocusHandle>,
    /// Click counters per button.
    counters: HashMap<String, u32>,
}

impl FormWindow {
    pub fn new(
        form: FormSpec,
        dispatcher: FormDispatcher,
        evaluate: Box<dyn FnMut(&str) -> Result<(), String>>,
        cx: &mut Context<Self>,
    ) -> FormWindow {
        let mut focus = HashMap::new();
        let mut text_values = HashMap::new();
        for c in &form.controls {
            if c.kind == ControlKind::TextBox {
                focus.insert(c.name.clone(), cx.focus_handle());
                text_values.insert(c.name.clone(), c.text().to_string());
            }
        }

        // Async timers (spec §5: Timer → non-blocking async loop).
        for c in &form.controls {
            if c.kind == ControlKind::Timer && c.enabled && c.interval_ms > 0 {
                let name = c.name.clone();
                let interval = std::time::Duration::from_millis(c.interval_ms);
                cx.spawn(async move |this, cx| {
                    loop {
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
                    }
                })
                .detach();
            }
        }

        let mut window = FormWindow {
            form,
            dispatcher,
            evaluate,
            text_values,
            focus,
            counters: HashMap::new(),
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
}

impl Render for FormWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .relative()
            .size_full()
            .bg(rgb(FORM_BG))
            .text_color(rgb(TEXT_COLOR));

        for control in self.form.controls.clone() {
            if !control.visible || control.kind == ControlKind::Timer {
                continue;
            }
            root = root.child(render_control(&control, self, cx));
        }
        root
    }
}

fn render_control(
    control: &crate::ControlSpec,
    state: &mut FormWindow,
    cx: &mut Context<FormWindow>,
) -> gpui::Stateful<gpui::Div> {
    let positioned = div()
        .id(SharedString::from(control.name.clone()))
        .absolute()
        .left(px(control.rect.left))
        .top(px(control.rect.top))
        .w(px(control.rect.width))
        .h(px(control.rect.height));

    match control.kind {
        ControlKind::CommandButton => positioned
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(BUTTON_BG))
            .border_1()
            .border_color(rgb(CONTROL_BORDER))
            .rounded_sm()
            .hover(|s| s.bg(rgb(BUTTON_BG_HOVER)))
            .active(|s| s.bg(rgb(BUTTON_BG_ACTIVE)))
            .child(SharedString::from(control.caption.clone()))
            .on_click({
                let name = control.name.clone();
                cx.listener(
                    move |this: &mut FormWindow, _ev: &gpui::ClickEvent, _win, _cx| {
                        *this.counters.entry(name.clone()).or_insert(0) += 1;
                        this.fire(&name, "Click");
                    },
                )
            }),
        ControlKind::Label => positioned
            .flex()
            .items_start()
            .child(SharedString::from(control.caption.clone())),
        ControlKind::TextBox => {
            let name = control.name.clone();
            let fh = state
                .focus
                .get(&name)
                .cloned()
                .expect("focus handle for textbox");
            let text = state.text_values.get(&name).cloned().unwrap_or_default();
            positioned
                .track_focus(&fh)
                .border_1()
                .border_color(rgb(CONTROL_BORDER))
                .bg(rgb(0xFFFFFF))
                .flex()
                .items_center()
                .pl_2()
                .child(SharedString::from(text))
                .on_key_down(cx.listener(
                    move |this: &mut FormWindow, ev: &KeyDownEvent, _win, cx| {
                        let key = ev.keystroke.key.as_str();
                        let cur = this.text_values.entry(name.clone()).or_default();
                        if key == "backspace" {
                            cur.pop();
                        } else if key == "enter" || key == "tab" {
                            // let navigation keys through
                        } else if key.chars().count() == 1 {
                            cur.push(key.chars().next().unwrap());
                        }
                        let changed = name.clone();
                        this.fire(&changed, "Change");
                        cx.notify();
                    },
                ))
        }
        ControlKind::Frame => positioned
            .border_1()
            .border_color(rgb(CONTROL_BORDER))
            .p_1()
            .child(SharedString::from(control.caption.clone())),
        ControlKind::ListBox => {
            let mut list = positioned
                .border_1()
                .border_color(rgb(CONTROL_BORDER))
                .bg(rgb(0xFFFFFF))
                .flex()
                .flex_col();
            for (i, item) in control.items.iter().enumerate() {
                let name = control.name.clone();
                list = list.child(
                    div()
                        .id((SharedString::from(control.name.clone()), i))
                        .pl_2()
                        .flex()
                        .items_center()
                        .h_6()
                        .hover(|s| s.bg(rgb(BUTTON_BG_HOVER)))
                        .child(SharedString::from(item.clone()))
                        .on_click(cx.listener(
                            move |this: &mut FormWindow, _ev: &gpui::ClickEvent, _win, _cx| {
                                this.fire(&name, "Click");
                            },
                        )),
                );
            }
            list
        }
        ControlKind::ComboBox | ControlKind::PictureBox => positioned
            .border_1()
            .border_color(rgb(CONTROL_BORDER))
            .bg(rgb(control.color.unwrap_or(BUTTON_BG)))
            .flex()
            .items_center()
            .pl_2()
            .child(SharedString::from(if control.kind == ControlKind::ComboBox {
                control
                    .items
                    .first()
                    .cloned()
                    .unwrap_or_else(|| control.caption.clone())
            } else {
                control.caption.clone()
            })),
        ControlKind::Timer => unreachable!("timers are not rendered"),
    }
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

