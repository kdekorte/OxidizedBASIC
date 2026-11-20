//! The find bar's view: the Find box (and the Replace box when replacing),
//! the match count, and the buttons, under the code window's header. The
//! state and the actions are in [`super::find_bar`].

use super::*;

impl Ide {
    /// The find bar as the code window shows it. `win` says which box has
    /// the keyboard, so that box is outlined.
    pub(super) fn render_find_bar(&self, win: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(bar) = self.find.as_ref() else {
            return div().into_any_element();
        };
        let count = match (bar.found.len(), bar.current) {
            _ if bar.find.is_empty() => String::new(),
            (0, _) => "No matches".to_string(),
            (n, Some(i)) => format!("{} of {}", i + 1, n),
            (n, None) => format!("{} matches", n),
        };
        let find_has_focus = self.find_focus.is_focused(win);
        let replace_has_focus = self.replace_focus.is_focused(win);
        let box_ = |id: &'static str,
                    text: String,
                    focused: bool,
                    replace: bool,
                    cx: &mut Context<Self>| {
            let focus = if replace {
                self.replace_focus.clone()
            } else {
                self.find_focus.clone()
            };
            let focus_click = focus.clone();
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .track_focus(&focus)
                .flex_1()
                .min_w_0()
                .h(px(24.0))
                .px_2()
                .flex()
                .items_center()
                .rounded_sm()
                .bg(rgb(WINDOW_BG))
                .border_1()
                .border_color(rgb(if focused { ACCENT } else { BORDER }))
                .overflow_hidden()
                .whitespace_nowrap()
                .child(SharedString::from(text))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |_this: &mut Ide, _ev: &MouseDownEvent, win, cx| {
                        cx.stop_propagation();
                        win.focus(&focus_click);
                        cx.notify();
                    }),
                )
                .on_key_down(
                    cx.listener(move |this: &mut Ide, ev: &KeyDownEvent, win, cx| {
                        if this.find_key(replace, ev, win) {
                            cx.stop_propagation();
                        }
                        cx.notify();
                    }),
                )
        };
        let label = |text: &'static str| {
            div()
                .w(px(64.0))
                .flex_none()
                .text_size(px(12.0))
                .text_color(rgb(DIM))
                .child(text)
        };
        let button =
            |id: &'static str, text: &'static str, act: fn(&mut Ide), cx: &mut Context<Self>| {
                div()
                    .id(id)
                    .debug_selector(move || id.to_string())
                    .flex_none()
                    .px_2()
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .text_size(px(12.0))
                    .text_color(rgb(FG))
                    .bg(rgb(WINDOW_BG))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .hover(|s| s.bg(rgb(SEL_BG)))
                    .cursor_pointer()
                    .child(text)
                    .on_click(cx.listener(move |this: &mut Ide, _ev, _win, cx| {
                        act(this);
                        cx.notify();
                    }))
            };
        let mut rows = div().flex().flex_col().gap_1().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(label("Find"))
                .child(box_(
                    "find-text",
                    bar.find.clone(),
                    find_has_focus,
                    false,
                    cx,
                ))
                .child(button(
                    "find-previous",
                    "◀",
                    |this| this.find_step(false),
                    cx,
                ))
                .child(button("find-next", "▶", |this| this.find_step(true), cx))
                .child(
                    div()
                        .debug_selector(|| "find-count".into())
                        .flex_none()
                        .w(px(96.0))
                        .text_size(px(12.0))
                        .text_color(rgb(DIM))
                        .child(count),
                ),
        );
        if bar.replacing {
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(label("Replace"))
                    .child(box_(
                        "replace-text",
                        bar.replace.clone(),
                        replace_has_focus,
                        true,
                        cx,
                    ))
                    .child(button(
                        "replace-one",
                        "Replace",
                        |this| this.replace_current(),
                        cx,
                    ))
                    .child(button(
                        "replace-all",
                        "All",
                        |this| {
                            this.replace_everything();
                        },
                        cx,
                    )),
            );
        }
        div()
            .debug_selector(|| "find-bar".into())
            .flex_none()
            .px_2()
            .py_1p5()
            .bg(rgb(TOOLBAR))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(rows)
            .into_any_element()
    }
}
