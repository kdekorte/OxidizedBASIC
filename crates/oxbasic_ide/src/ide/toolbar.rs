//! The main window's unified toolbar, as macOS apps draw it (Xcode, Finder):
//! the window's own title bar holds the controls. The traffic lights sit at
//! its left; then Run, Stop and Step, the editor mode (Object | Code), and in
//! the middle an activity box with the project, its state and the latest
//! status; at the right the save button.
//!
//! The title bar is transparent (`appears_transparent`), so macOS still
//! drags the window from an empty part of the bar's top and zooms it on a
//! double click there (a prototype confirmed both with real mouse events);
//! a double click lower in the bar is passed on to the system here.

use super::*;

/// The toolbar's height; the traffic lights are centred in it.
pub(super) const TOOLBAR_H: f32 = 52.0;

/// Where macOS draws the traffic lights: centred vertically in the bar.
pub(super) fn traffic_lights() -> gpui::Point<gpui::Pixels> {
    gpui::point(px(18.0), px(TOOLBAR_H / 2.0 - 6.0))
}

/// Room left of the controls for the traffic lights.
const TRAFFIC_LIGHTS_W: f32 = 84.0;

/// The last part of `path`, for a status message that has to fit the bar
/// (the Project panel shows where the project is).
pub(super) fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// What the program is doing, as the activity box and the buttons show it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum RunState {
    Idle,
    Running,
    Paused,
}

/// A toolbar tooltip, styled like the system's.
struct ToolTip(SharedString);

impl Render for ToolTip {
    fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "toolbar-tooltip".into())
            .px_2()
            .py(px(3.0))
            .rounded(px(5.0))
            .bg(rgb(0xF5F5F7))
            .border_1()
            .border_color(rgb(0xD1D1D6))
            .shadow_md()
            .text_size(px(11.0))
            .text_color(rgb(FG))
            .child(self.0.clone())
    }
}

fn tip(text: &'static str) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView {
    move |_w, cx| cx.new(|_| ToolTip(text.into())).into()
}

impl Ide {
    /// Anything unsaved, the working copy included: the bar's "Edited" and
    /// the window title's `*`.
    pub(super) fn has_unsaved_work(&self) -> bool {
        self.working_copy_changed() || self.workspace.is_dirty()
    }

    pub(super) fn run_state(&self) -> RunState {
        if self.paused.is_some() {
            RunState::Paused
        } else if self.session.is_some() {
            RunState::Running
        } else {
            RunState::Idle
        }
    }

    /// A square icon button: a symbol, a tooltip with its shortcut, and
    /// greyed out (and inert) when `enabled` is false.
    fn icon_button(
        &self,
        id: &'static str,
        symbol: &'static str,
        tooltip: &'static str,
        enabled: bool,
        action: impl Fn(&mut Ide, &mut Context<Ide>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let b = div()
            .id(id)
            .debug_selector(move || id.to_string())
            .w(px(32.0))
            .h(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .text_size(px(15.0))
            .child(symbol)
            .tooltip(tip(tooltip));
        if !enabled {
            return b.text_color(rgb(0xC7C7CC));
        }
        b.text_color(rgb(0x48484A))
            .hover(|s| s.bg(rgb(0xE3E3E8)))
            .active(|s| s.bg(rgb(0xD1D1D6)))
            .on_click(cx.listener(move |this: &mut Ide, _ev, _w, cx| {
                action(this, cx);
                cx.notify();
            }))
    }
}

/// The middle of the bar: the editor mode and the activity box.
impl Ide {
    /// Object | Code: which view of the active form is in front, like
    /// Xcode's editor mode control.
    fn mode_control(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let code = self.code_window.is_some();
        let segment = |id: &'static str, label: &'static str, tooltip: &'static str, on: bool| {
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .px_3()
                .h(px(22.0))
                .flex()
                .items_center()
                .rounded(px(5.0))
                .text_size(px(12.0))
                .when(on, |s| s.bg(rgb(0xFFFFFF)).text_color(rgb(FG)).shadow_sm())
                .when(!on, |s| {
                    s.text_color(rgb(0x48484A)).hover(|s| s.bg(rgb(0xDCDCE0)))
                })
                .tooltip(tip(tooltip))
                .child(label)
        };
        div()
            .debug_selector(|| "toolbar-mode".into())
            .flex()
            .flex_none()
            .items_center()
            .p(px(2.0))
            .gap(px(2.0))
            .rounded(px(7.0))
            .bg(rgb(0xE5E5EA))
            .child(
                segment(
                    "toolbar-view-object",
                    "Object",
                    "Show the form (⇧F7)",
                    !code,
                )
                .on_click(cx.listener(|this: &mut Ide, _ev, _w, cx| {
                    this.hide_code_window(cx);
                    cx.notify();
                })),
            )
            .child(
                segment("toolbar-view-code", "Code", "Show the code (F7)", code).on_click(
                    cx.listener(|this: &mut Ide, _ev, _w, cx| {
                        this.view_code(cx);
                        cx.notify();
                    }),
                ),
            )
    }

    /// The activity box: what is being worked on, whether the program runs,
    /// and the latest status message.
    fn activity_view(&self) -> impl IntoElement {
        let dot = match self.run_state() {
            RunState::Idle => None,
            RunState::Running => Some(0x34C759),
            RunState::Paused => Some(0xFF9F0A),
        };
        let unsaved = self.has_unsaved_work();
        let doc = self.workspace.active_doc().name.clone();
        div()
            .debug_selector(|| "toolbar-activity".into())
            .flex_1()
            .max_w(px(560.0))
            .min_w(px(200.0))
            .h(px(30.0))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(7.0))
            .bg(rgb(0xFFFFFF))
            .border_1()
            .border_color(rgb(0xE0E0E5))
            .text_size(px(12.0))
            .overflow_hidden()
            .whitespace_nowrap()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_1()
                    .text_color(rgb(FG))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(SharedString::from(self.workspace.name.clone()))
                    .child(div().text_color(rgb(0xAEAEB2)).child("›"))
                    .child(SharedString::from(doc))
                    .when(unsaved, |s| {
                        s.child(
                            div()
                                .debug_selector(|| "toolbar-unsaved".into())
                                .font_weight(gpui::FontWeight::NORMAL)
                                .text_color(rgb(DIM))
                                .child("— Edited"),
                        )
                    }),
            )
            .child(div().flex_1())
            .when_some(dot, |s, c| {
                s.child(div().flex_none().size(px(7.0)).rounded_full().bg(rgb(c)))
            })
            .child(
                div()
                    .debug_selector(|| "toolbar-status".into())
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(rgb(DIM))
                    .child(SharedString::from(self.status.clone())),
            )
    }
}

/// The bar.
impl Ide {
    pub(super) fn render_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.run_state();
        // Run starts the program and continues a paused one; while it runs
        // freely there is nothing to start, so it is greyed
        let (run_id, run_tip) = match state {
            RunState::Paused => ("toolbar-run", "Continue (⌘R)"),
            RunState::Running => ("toolbar-live", "The program is running"),
            RunState::Idle => ("toolbar-run", "Run (⌘R)"),
        };
        let run = self.icon_button(
            run_id,
            "▶",
            run_tip,
            state != RunState::Running,
            |this, _cx| this.run_program(),
            cx,
        );
        let stop = self.icon_button(
            "toolbar-stop",
            "■",
            "Stop (⇧⌘R)",
            state != RunState::Idle,
            |this, cx| this.stop_program(cx),
            cx,
        );
        let step = self.icon_button(
            "toolbar-step",
            "↷",
            "Step over: run to the next line",
            state != RunState::Running,
            |this, _cx| this.step_program(),
            cx,
        );
        let save = self
            .icon_button(
                "toolbar-save",
                "↓",
                "Save the project (⌘S)",
                true,
                |_this, _cx| {},
                cx,
            )
            .on_click(cx.listener(|this: &mut Ide, _ev, win, cx| {
                this.save_from_toolbar(win, cx);
                cx.notify();
            }));
        div()
            .id("toolbar")
            .debug_selector(|| "toolbar".into())
            .flex()
            .flex_none()
            .items_center()
            .h(px(TOOLBAR_H))
            .pl(px(TRAFFIC_LIGHTS_W))
            .pr_3()
            .gap_2()
            .bg(rgb(TOOLBAR))
            .border_b_1()
            .border_color(rgb(BORDER))
            // a double click on the bar zooms the window, as on a title bar
            // (the system already sees one on the bar's top part)
            .on_mouse_down(
                gpui::MouseButton::Left,
                |ev: &MouseDownEvent, win: &mut Window, _cx| {
                    if ev.click_count == 2 {
                        win.titlebar_double_click();
                    }
                },
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_1()
                    .child(run)
                    .child(stop)
                    .child(step),
            )
            .child(self.mode_control(cx))
            .child(div().flex_1().min_w(px(8.0)))
            .child(self.activity_view())
            .child(div().flex_1().min_w(px(8.0)))
            .child(save)
    }

    /// The save button is File > Save Project, which asks where to save a
    /// project that has no file yet.
    fn save_from_toolbar(&mut self, win: &mut Window, cx: &mut Context<Self>) {
        if self.workspace.file.is_some() {
            if let Err(e) = self.save_project() {
                self.status = format!("save failed: {}", e);
            }
        } else {
            self.save_project_as(win, cx);
        }
    }
}
