//! The code window: the module's source in its own OS window, opened on
//! request (View Code, double-clicking a control in the designer). Like
//! VB's code window, its header holds two select boxes — the object list
//! (`(General)`, `Form`, then every control) and, beside it, the events
//! that object raises. Picking an event jumps to its handler, inserting a
//! `Private Sub <Object>_<Event>()` placeholder when it doesn't exist yet;
//! both lists follow the caret as you move through the code.
//!
//! The editor state (lines, caret, breakpoints) stays on the [`Ide`], so
//! the designer, the debugger and Save Code all see the same source; the
//! [`CodeWindow`] view only renders it.

use super::*;

/// The root view of the code window: a thin view over the IDE's state.
pub struct CodeWindow {
    pub(super) ide: Entity<Ide>,
    _observe: Subscription,
}

impl CodeWindow {
    pub(super) fn new(ide: Entity<Ide>, cx: &mut Context<Self>) -> CodeWindow {
        // Re-render whenever the IDE changes (edits, renames, debugger).
        let observe = cx.observe(&ide, |_, _, cx| cx.notify());
        CodeWindow {
            ide,
            _observe: observe,
        }
    }
}

impl Render for CodeWindow {
    fn render(&mut self, win: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.ide.read(cx).code_window_title();
        win.set_window_title(&title);
        self.ide
            .update(cx, |ide, cx| ide.render_code_window(win, cx))
    }
}

/// Code-window navigation: the object / event lists and the jump to (or
/// creation of) an event handler.
impl Ide {
    /// The object list: `(General)`, `Form`, then every control by name.
    pub(super) fn code_objects(&self) -> Vec<String> {
        let mut objects = vec![GENERAL.to_string(), "Form".to_string()];
        // a control array is one object, whatever its number of elements
        for c in &self.form.controls {
            if !objects.iter().any(|o| o.eq_ignore_ascii_case(&c.name)) {
                objects.push(c.name.clone());
            }
        }
        objects
    }

    /// Whether `object` names a control array (its controls have an Index).
    pub(super) fn is_control_array(&self, object: &str) -> bool {
        self.form
            .controls
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(object) && c.index.is_some())
    }

    /// The control kind behind an object list entry (`Form` for the form).
    pub(super) fn object_kind(&self, object: &str) -> Option<&'static str> {
        if object.eq_ignore_ascii_case("Form") {
            return Some("Form");
        }
        self.form
            .controls
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(object))
            .map(|c| c.kind.basic_name())
    }

    /// Split a handler name into an object list entry and its event, when
    /// the name is one: `Command1_Click` -> (`Command1`, `Click`).
    pub(super) fn split_handler(&self, proc_name: &str) -> Option<(String, &'static str)> {
        for object in self.code_objects().into_iter().skip(1) {
            let n = object.len();
            if proc_name.len() > n + 1
                && proc_name.is_char_boundary(n)
                && proc_name[..n].eq_ignore_ascii_case(&object)
                && proc_name.as_bytes()[n] == b'_'
            {
                let kind = self.object_kind(&object)?;
                let event = &proc_name[n + 1..];
                if let Some((e, _)) = event_signatures(kind)
                    .iter()
                    .find(|(e, _)| e.eq_ignore_ascii_case(event))
                {
                    return Some((object, *e));
                }
            }
        }
        None
    }

    /// What the two select boxes show for the caret's position: the object
    /// and the event (or procedure name) of the block the caret is in.
    pub(super) fn code_selection(&self) -> (String, String) {
        let procs = code_model::find_procedures(&self.lines);
        let (object, proc_label) = match code_model::proc_at(&procs, self.cursor.0) {
            Some(p) => match self.split_handler(&p.name) {
                Some((object, event)) => (object, event.to_string()),
                None => (GENERAL.to_string(), p.name.clone()),
            },
            None => (GENERAL.to_string(), DECLARATIONS.to_string()),
        };
        match &self.code_object {
            // A different object was picked but no event yet.
            Some(picked) if !picked.eq_ignore_ascii_case(&object) => {
                (picked.clone(), String::new())
            }
            _ => (object, proc_label),
        }
    }

    /// The procedure list for `object`: its events with whether code
    /// exists for each, or — for `(General)` — `(Declarations)` and the
    /// module's procedures that aren't event handlers.
    pub(super) fn code_events(&self, object: &str) -> Vec<(String, bool)> {
        let procs = code_model::find_procedures(&self.lines);
        if object == GENERAL {
            let mut items = vec![(DECLARATIONS.to_string(), true)];
            items.extend(
                procs
                    .iter()
                    .filter(|p| self.split_handler(&p.name).is_none())
                    .map(|p| (p.name.clone(), true)),
            );
            return items;
        }
        let Some(kind) = self.object_kind(object) else {
            return vec![];
        };
        event_signatures(kind)
            .iter()
            .map(|(event, _)| {
                let handler = format!("{}_{}", object, event);
                let exists = procs.iter().any(|p| p.name.eq_ignore_ascii_case(&handler));
                (event.to_string(), exists)
            })
            .collect()
    }
}

/// Jumping to handlers.
impl Ide {
    /// Pick an entry in the object list. `(General)` goes to the
    /// declarations; an object with code goes to its first handler, as VB
    /// does; otherwise its default event handler opens (inserting the
    /// placeholder) so the procedure list matches what's on screen.
    pub(super) fn pick_object(&mut self, object: &str) {
        self.code_menu = None;
        if object == GENERAL {
            self.pick_event(GENERAL, DECLARATIONS);
            return;
        }
        let Some(kind) = self.object_kind(object) else {
            return;
        };
        let event = self
            .code_events(object)
            .into_iter()
            .find(|(_, exists)| *exists)
            .map(|(e, _)| e)
            .or_else(|| default_event(kind).map(|e| e.to_string()))
            .unwrap_or_default();
        self.pick_event(object, &event);
    }

    /// Pick `event` of `object` (or a `(General)` procedure): move the
    /// caret to its handler, inserting the placeholder when there is none.
    pub(super) fn pick_event(&mut self, object: &str, event: &str) {
        self.code_menu = None;
        self.code_object = None;
        self.reveal_cursor = true;
        if object == GENERAL {
            let procs = code_model::find_procedures(&self.lines);
            self.cursor = match procs.iter().find(|p| p.name == event) {
                Some(p) => code_model::body_position(&self.lines, p),
                None => (0, 0),
            };
            return;
        }
        let Some(kind) = self.object_kind(object) else {
            return;
        };
        // Use the object's spelling as the list shows it.
        let object = self
            .code_objects()
            .into_iter()
            .find(|o| o.eq_ignore_ascii_case(object))
            .unwrap_or_else(|| object.to_string());
        let params = event_params(kind, event).unwrap_or("");
        // a control array's handlers are told which element raised them
        let params = match (self.is_control_array(&object), params) {
            (false, p) => p.to_string(),
            (true, "") => "Index As Integer".to_string(),
            (true, p) => format!("Index As Integer, {}", p),
        };
        let handler = format!("{}_{}", object, event);
        let before = self.lines.clone();
        let cursor_before = self.cursor;
        let (row, col, inserted) = code_model::ensure_handler(&mut self.lines, &handler, &params);
        if inserted {
            // the new placeholder undoes as one step
            let after = std::mem::replace(&mut self.lines, before);
            self.cursor = cursor_before;
            self.begin_ide_edit();
            self.lines = after;
        }
        self.anchor = None;
        self.cursor = (row, col);
        if inserted {
            self.status = format!("added {} placeholder", handler);
        }
    }

    /// Show the code window at `object`'s `event` handler (creating its
    /// placeholder if needed); `None` for `event` uses the object's default
    /// event — what double-clicking a control in the designer does.
    pub(super) fn open_event(&mut self, object: &str, event: Option<&str>, cx: &mut Context<Self>) {
        let Some(kind) = self.object_kind(object) else {
            return;
        };
        let event = event
            .map(|e| e.to_string())
            .or_else(|| default_event(kind).map(|e| e.to_string()))
            .unwrap_or_default();
        self.pick_event(object, &event);
        self.show_code_window(cx);
    }

    /// Open the default event of control `idx`, or of the form (`None`).
    pub(super) fn open_default_event(&mut self, idx: Option<usize>, cx: &mut Context<Self>) {
        self.commit_pending_rename();
        let object = match idx.and_then(|i| self.form.controls.get(i)) {
            Some(c) => c.name.clone(),
            None => "Form".to_string(),
        };
        self.open_event(&object, None, cx);
    }

    pub(super) fn code_window_title(&self) -> String {
        let file = self.workspace.active_doc().file_label();
        format!("{} ({}) — Code", self.form.name, file)
    }
}

/// Opening and focusing the code window.
impl Ide {
    /// Bring up the code window (opening it the first time) with the
    /// editor focused. The window is opened from a deferred callback: GPUI
    /// draws a new window synchronously, and the code window's first draw
    /// reads this `Ide`, which is still mutably borrowed here.
    pub(super) fn show_code_window(&mut self, cx: &mut Context<Self>) {
        self.reveal_cursor = true;
        let ide = cx.entity();
        let existing = self.code_window;
        let focus = self.editor_focus.clone();
        cx.defer(move |cx| {
            if let Some(handle) = existing {
                let alive = handle
                    .update(cx, |_, win, _| {
                        win.activate_window();
                        win.focus(&focus);
                    })
                    .is_ok();
                if alive {
                    return;
                }
            }
            let bounds = Bounds::centered(None, size(px(820.), px(600.)), cx);
            let weak = ide.downgrade();
            let on_close = weak.clone();
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some(SharedString::from("Code")),
                        // its header is the title bar, as in the main window
                        appears_transparent: true,
                        traffic_light_position: Some(super::toolbar::traffic_lights()),
                    }),
                    window_min_size: Some(size(px(560.), px(260.))),
                    ..Default::default()
                },
                move |win, cx| {
                    // Closing the window only hides the code; it stays in
                    // the IDE for the next View Code.
                    win.on_window_should_close(cx, move |_win, cx| {
                        if let Some(ide) = on_close.upgrade() {
                            ide.update(cx, |ide, cx| {
                                ide.code_window = None;
                                ide.code_menu = None;
                                cx.notify();
                            });
                        }
                        true
                    });
                    win.focus(&focus);
                    cx.new(|cx| CodeWindow::new(ide.clone(), cx))
                },
            );
            match opened {
                Ok(handle) => {
                    let any: AnyWindowHandle = handle.into();
                    if let Some(ide) = weak.upgrade() {
                        ide.update(cx, |ide, cx| {
                            ide.code_window = Some(any);
                            cx.notify();
                        });
                    }
                }
                Err(e) => {
                    if let Some(ide) = weak.upgrade() {
                        ide.update(cx, |ide, cx| {
                            ide.status = format!("could not open the code window: {}", e);
                            cx.notify();
                        });
                    }
                }
            }
        });
    }

    /// Close the code window (e.g. View Object).
    pub(super) fn hide_code_window(&mut self, cx: &mut Context<Self>) {
        self.code_menu = None;
        if let Some(handle) = self.code_window.take() {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, win, _| win.remove_window());
            });
        }
    }
}

/// Rendering the code window.
impl Ide {
    pub(super) fn render_code_window(
        &mut self,
        win: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (object, event) = self.code_selection();
        let events = self.code_events(&object);
        let objects = self.code_objects();
        let menu = self.code_menu;

        let header = div()
            .flex_none()
            .h(px(super::toolbar::TOOLBAR_H))
            // room for the traffic lights
            .pl(px(84.0))
            .pr_3()
            .flex()
            .items_center()
            .gap_2()
            .bg(rgb(TOOLBAR))
            .border_b_1()
            .border_color(rgb(BORDER))
            .on_mouse_down(
                gpui::MouseButton::Left,
                |ev: &MouseDownEvent, win: &mut Window, _cx| {
                    if ev.click_count == 2 {
                        win.titlebar_double_click();
                    }
                },
            )
            .child(self.select_box("code-object", CodeMenu::Object, object.clone(), cx))
            .child(self.select_box(
                "code-event",
                CodeMenu::Event,
                if event.is_empty() {
                    "(choose an event)".to_string()
                } else {
                    event.clone()
                },
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(rgb(DIM))
                    .text_xs()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(SharedString::from(self.status.clone())),
            );

        // The open list floats over the editor, under its select box.
        let popup = match menu {
            Some(CodeMenu::Object) => Some(
                self.select_list(
                    "code-object-list",
                    0.0,
                    objects
                        .iter()
                        .map(|o| (o.clone(), o.eq_ignore_ascii_case(&object), false))
                        .collect(),
                    |this, item| this.pick_object(&item),
                    cx,
                ),
            ),
            Some(CodeMenu::Event) => {
                let obj = object.clone();
                Some(
                    self.select_list(
                        "code-event-list",
                        SELECT_W + 8.0,
                        events
                            .iter()
                            .map(|(e, exists)| (e.clone(), *e == event, *exists && obj != GENERAL))
                            .collect(),
                        move |this, item| this.pick_event(&obj, &item),
                        cx,
                    ),
                )
            }
            None => None,
        };

        if self.reveal_cursor {
            self.reveal_cursor = false;
            self.scroll_cursor_into_view(win);
        }

        let mut root = div()
            .id("code-window")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(WINDOW_BG))
            .text_color(rgb(FG))
            .text_sm()
            .font_family(".AppleSystemUIFont")
            .on_action(cx.listener(|this, _: &Quit, win, cx| {
                this.quit(win, cx);
            }))
            .on_action(cx.listener(|this, _: &SaveProject, win, cx| {
                if this.workspace.file.is_some() {
                    if let Err(e) = this.save_project() {
                        this.status = format!("save failed: {}", e);
                    }
                    cx.notify();
                } else {
                    this.save_project_as(win, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SaveCode, _win, cx| {
                this.save_code();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditUndo, win, cx| {
                this.edit_action(win, cx, |ide, _| ide.undo());
            }))
            .on_action(cx.listener(|this, _: &EditRedo, win, cx| {
                this.edit_action(win, cx, |ide, _| ide.redo());
            }))
            .on_action(cx.listener(|this, _: &EditCut, win, cx| {
                this.edit_action(win, cx, |ide, cx| ide.cut(cx));
            }))
            .on_action(cx.listener(|this, _: &EditCopy, win, cx| {
                this.edit_action(win, cx, |ide, cx| {
                    ide.copy(cx);
                });
            }))
            .on_action(cx.listener(|this, _: &EditPaste, win, cx| {
                this.edit_action(win, cx, |ide, cx| ide.paste(cx));
            }))
            .on_action(cx.listener(|this, _: &EditFind, win, cx| {
                this.open_find(false, win);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditReplace, win, cx| {
                this.open_find(true, win);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditFindNext, _win, cx| {
                this.find_step(true);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditFindPrevious, _win, cx| {
                this.find_step(false);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditSelectAll, win, cx| {
                this.edit_action(win, cx, |ide, _| ide.select_all());
            }))
            .on_action(cx.listener(|this, _: &RunProgram, _win, cx| {
                this.run_program();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &StepProgram, _win, cx| {
                this.step_program();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &StopProgram, _win, cx| {
                this.stop_program(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ViewObject, _win, cx| {
                this.hide_code_window(cx);
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _win, cx| {
                this.update_scrollbar_drag(ev, cx);
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _ev: &gpui::MouseUpEvent, _win, _cx| {
                    this.scrollbar_drag = None;
                }),
            )
            .child(header)
            .children(self.find.as_ref().map(|_| self.render_find_bar(win, cx)))
            .child(div().flex_1().min_h_0().child(self.render_editor(cx)));
        if let Some(popup) = popup {
            root = root.child(popup);
        }
        root.into_any_element()
    }

    /// One of the header's select boxes, showing `value`; clicking it
    /// opens (or closes) its list.
    fn select_box(
        &self,
        id: &'static str,
        which: CodeMenu,
        value: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let open = self.code_menu == Some(which);
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .w(px(SELECT_W))
            .flex_none()
            .h_7()
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .rounded_md()
            .bg(rgb(WINDOW_BG))
            .border_1()
            .border_color(rgb(if open { ACCENT } else { BORDER }))
            .cursor_pointer()
            .hover(|s| s.border_color(rgb(ACCENT)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(SharedString::from(value)),
            )
            .child(div().text_color(rgb(DIM)).child("⌄"))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this: &mut Ide, _ev: &MouseDownEvent, _win, cx| {
                    cx.stop_propagation();
                    this.code_menu = if this.code_menu == Some(which) {
                        None
                    } else {
                        Some(which)
                    };
                    cx.notify();
                }),
            )
    }
}

/// The select boxes' lists and caret scrolling.
impl Ide {
    /// A select box's open list, `left` px from the header's left edge.
    /// Each item is `(label, selected, has_code)`; items with code are bold
    /// (VB's cue for "this event has a handler"). A click outside closes it.
    fn select_list(
        &self,
        id: &'static str,
        left: f32,
        items: Vec<(String, bool, bool)>,
        on_pick: impl Fn(&mut Ide, String) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div()
            .id(id)
            .debug_selector(move || id.to_string())
            .occlude()
            .absolute()
            .top(px(40.0))
            .left(px(8.0 + left))
            .w(px(SELECT_W))
            .max_h(px(360.0))
            .overflow_y_scroll()
            .py_1()
            .rounded_md()
            .bg(rgb(WINDOW_BG))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow_md()
            .on_mouse_down_out(
                cx.listener(|this: &mut Ide, _ev: &MouseDownEvent, _win, cx| {
                    this.code_menu = None;
                    cx.notify();
                }),
            );
        for (i, (label, selected, has_code)) in items.into_iter().enumerate() {
            let pick = on_pick.clone();
            let item = label.clone();
            let mut row = div()
                .id((SharedString::from(id), i))
                .debug_selector(move || format!("{}-{}", id, item))
                .h_6()
                .px_2()
                .flex()
                .items_center()
                .cursor_pointer()
                .whitespace_nowrap()
                .hover(|s| s.bg(rgb(ACCENT)).text_color(rgb(WINDOW_BG)))
                .child(SharedString::from(label.clone()))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |this: &mut Ide, _ev: &MouseDownEvent, win, cx| {
                        cx.stop_propagation();
                        pick(this, label.clone());
                        win.focus(&this.editor_focus);
                        cx.notify();
                    }),
                );
            if selected {
                row = row.bg(rgb(SEL_BG));
            }
            if has_code {
                row = row.font_weight(gpui::FontWeight::BOLD);
            }
            list = list.child(row);
        }
        deferred(list).with_priority(1).into_any_element()
    }

    /// Scroll the editor so the caret's row is visible, leaving a few rows
    /// of context above it.
    pub(super) fn scroll_cursor_into_view(&mut self, _win: &mut Window) {
        let handle = &self.editor_scroll;
        let viewport = f32::from(handle.bounds().size.height);
        let row_top = self.cursor.0 as f32 * ROW_H;
        let mut offset = handle.offset();
        let top = -f32::from(offset.y);
        if viewport <= 0.0 {
            // Not laid out yet (the window is opening): aim for the row
            // and let layout clamp it.
            offset.y = px(-(row_top - 3.0 * ROW_H).max(0.0));
        } else if row_top < top || row_top + ROW_H > top + viewport {
            offset.y = px(-(row_top - 3.0 * ROW_H).max(0.0));
        } else {
            return;
        }
        handle.set_offset(offset);
    }
}
