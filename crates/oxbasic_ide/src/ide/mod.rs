//! The OxidizedBASIC IDE (Phase 3): a GPUI application with
//!
//! - a syntax-highlighted code editor with a breakpoint gutter,
//! - a visual form designer (drag-to-move, add controls, save `.oxform`),
//! - a property inspector for the selected control,
//! - an output / immediate pane,
//! - debug controls (run, step-over, stop) with a variable watch pane.

pub mod debug_session;
pub mod highlight;

use debug_session::{DebugCommand, DebugEvent, DebugSession};
use oxbasic_runtime::PausedInfo;
use oxbasic_ui::{ControlKind, ControlSpec, FormSpec, Rect};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use gpui::{
    App, AppContext as _, Application, Bounds, Context, FocusHandle, KeyDownEvent,
    MouseDownEvent, MouseMoveEvent, Pixels, Point, SharedString, Window, WindowBounds,
    WindowOptions, canvas, div, prelude::*, px, rgb, size,
};

// Palette (dark, VS Code-ish)
const BG: u32 = 0x1E1E1E;
const PANEL: u32 = 0x252526;
const TOOLBAR: u32 = 0x333333;
const BORDER: u32 = 0x3C3C3C;
const FG: u32 = 0xD4D4D4;
const DIM: u32 = 0x858585;
const ACCENT: u32 = 0x0078D7;
const SEL_BG: u32 = 0x264F78;
const C_KEYWORD: u32 = 0x569CD6;
const C_STRING: u32 = 0xCE9178;
const C_NUMBER: u32 = 0xB5CEA8;
const C_COMMENT: u32 = 0x6A9955;
const C_IDENT: u32 = 0x9CDCFE;
const C_PUNCT: u32 = 0xD4D4D4;
const FORM_BG: u32 = 0xECE9D8;
const CTRL_BG: u32 = 0xE1E1E1;
const CTRL_BORDER: u32 = 0x8E8E8E;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// Launch the IDE, optionally loading a `.bas` source file and an
/// `.oxform` layout file.
pub fn run_ide(bas_path: Option<&str>, oxform_path: Option<&str>) {
    let bas_path = bas_path.map(|s| s.to_string());
    let oxform_path = oxform_path.map(|s| s.to_string());
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(SharedString::from("OxidizedBASIC IDE")),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |win, cx| {
                let bas = bas_path.clone();
                let oxform = oxform_path.clone();
                let entity = cx.new(|cx| Ide::new(bas.as_deref(), oxform.as_deref(), cx));
                // start with the editor focused so typing works immediately
                win.focus(&entity.read(cx).editor_focus);
                entity
            },
        )
        .expect("failed to open IDE window");
        cx.activate(true);
    });
}

pub struct Ide {
    // files
    bas_path: Option<String>,
    oxform_path: Option<String>,
    // code editor
    lines: Vec<String>,
    cursor: (usize, usize), // (row, char-col)
    editor_focus: FocusHandle,
    breakpoints: Arc<Mutex<HashSet<usize>>>,
    // form designer
    form: FormSpec,
    selected: Option<usize>,
    drag: Option<(usize, f32, f32)>, // control idx, grab offset
    canvas_origin: Arc<Mutex<Option<Point<Pixels>>>>,
    // property inspector buffers: (control idx, prop) -> live text
    prop_bufs: HashMap<(usize, &'static str), String>,
    inspector_focus: FocusHandle,
    // output & debug
    output: String,
    session: Option<DebugSession>,
    paused: Option<PausedInfo>,
    status: String,
    evt_rx: Arc<Mutex<Receiver<DebugEvent>>>,
    evt_tx: Sender<DebugEvent>,
}

impl Ide {
    pub fn new(
        bas_path: Option<&str>,
        oxform_path: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Ide {
        let (evt_tx, evt_rx) = channel::<DebugEvent>();
        let evt_rx = Arc::new(Mutex::new(evt_rx));

        let default_src = "\
' OxidizedBASIC IDE — Phase 3
' Set breakpoints in the gutter, then Run or Step.

Dim Total As Long

Sub Command1_Click()
    Total = Total + 1
    MsgBox \"clicked \" & CStr(Total) & \" times\"
End Sub

For i = 1 To 10
    Total = Total + i
Next
Print \"Total = \" & CStr(Total)
";

        let lines: Vec<String> = match bas_path.and_then(|p| std::fs::read_to_string(p).ok()) {
            Some(src) => src.lines().map(|l| l.to_string()).collect(),
            None => default_src.lines().map(|l| l.to_string()).collect(),
        };

        let form = match oxform_path
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|json| FormSpec::from_oxform(&json).ok())
        {
            Some(f) => f,
            None => default_form(),
        };

        let mut ide = Ide {
            bas_path: bas_path.map(|s| s.to_string()),
            oxform_path: oxform_path.map(|s| s.to_string()),
            lines,
            cursor: (0, 0),
            editor_focus: cx.focus_handle(),
            breakpoints: Arc::new(Mutex::new(HashSet::new())),
            form,
            selected: None,
            drag: None,
            canvas_origin: Arc::new(Mutex::new(None)),
            prop_bufs: HashMap::new(),
            inspector_focus: cx.focus_handle(),
            output: String::new(),
            session: None,
            paused: None,
            status: "Ready".into(),
            evt_rx,
            evt_tx,
        };

        // Start with the editor focused so typing works immediately, and
        // leave a hint in the output pane.
        ide.output.push_str(
            "oxbasic IDE\n\
             \u{2022} click in the editor and type to edit code\n\
             \u{2022} click a line number to toggle a breakpoint\n\
             \u{2022} Run executes the program (output appears here); Step pauses at each statement\n\
             \u{2022} in the designer, click a control to select it, drag to move, edit properties below\n\
             \u{2022} Save Form / Save Code write .oxform / .bas files\n",
        );

        // Poll debug events and pump them into the entity.
        let rx = ide.evt_rx.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(40))
                    .await;
                let mut events = Vec::new();
                {
                    let rx = rx.lock().unwrap();
                    while let Ok(ev) = rx.try_recv() {
                        events.push(ev);
                    }
                }
                for ev in events {
                    let _ = this.update(cx, |ide, cx| {
                        ide.on_debug_event(ev);
                        cx.notify();
                    });
                }
            }
        })
        .detach();

        ide
    }

    fn on_debug_event(&mut self, ev: DebugEvent) {
        match ev {
            DebugEvent::Paused(info) => {
                self.status = format!(
                    "Paused at line {} in {}",
                    info.line,
                    if info.proc.is_empty() { "(module)" } else { &info.proc }
                );
                self.paused = Some(info);
            }
            DebugEvent::Output(s) => {
                self.output.push_str(&s);
                self.status = "Running".into();
            }
            DebugEvent::Finished(msg) => {
                self.status = msg;
                self.paused = None;
                self.session = None;
            }
            DebugEvent::Failed(msg) => {
                self.status = msg;
                self.session = None;
            }
        }
    }

    fn source(&self) -> String {
        self.lines.join("\n")
    }
}


/// Toolbar actions and control additions.
impl Ide {
    fn run_program(&mut self) {
        if let Some(s) = &self.session {
            s.send(DebugCommand::Continue);
            return;
        }
        let session =
            DebugSession::start(self.source(), self.breakpoints.clone(), self.evt_tx.clone());
        session.send(DebugCommand::Run);
        self.session = Some(session);
        self.paused = None;
        self.status = "Running".into();
    }

    fn step_program(&mut self) {
        if let Some(s) = &self.session {
            s.send(DebugCommand::StepOver);
            return;
        }
        let session =
            DebugSession::start(self.source(), self.breakpoints.clone(), self.evt_tx.clone());
        session.send(DebugCommand::Step);
        self.session = Some(session);
    }

    fn stop_program(&mut self) {
        if let Some(s) = &self.session.take() {
            s.send(DebugCommand::Stop);
        }
        self.paused = None;
        self.status = "Stopped".into();
    }

    fn save_code(&mut self) {
        let path = self
            .bas_path
            .clone()
            .unwrap_or_else(|| "program.bas".to_string());
        match std::fs::write(&path, self.source()) {
            Ok(()) => self.status = format!("saved {}", path),
            Err(e) => self.status = format!("save failed: {}", e),
        }
    }

    fn save_form(&mut self) {
        let path = self
            .oxform_path
            .clone()
            .unwrap_or_else(|| "form1.oxform".to_string());
        match std::fs::write(&path, self.form.to_oxform()) {
            Ok(()) => self.status = format!("saved {}", path),
            Err(e) => self.status = format!("save failed: {}", e),
        }
    }

    fn toggle_breakpoint(&mut self, line: usize) {
        let mut bps = self.breakpoints.lock().unwrap();
        if !bps.insert(line) {
            bps.remove(&line);
        }
    }

    fn add_control(&mut self, kind: ControlKind) {
        let n = self.form.controls.iter().filter(|c| c.kind == kind).count() + 1;
        let prefix = match kind {
            ControlKind::CommandButton => "Command",
            ControlKind::Label => "Label",
            ControlKind::TextBox => "Text",
            ControlKind::Frame => "Frame",
            ControlKind::ListBox => "List",
            ControlKind::ComboBox => "Combo",
            ControlKind::PictureBox => "Picture",
            ControlKind::Timer => "Timer",
        };
        let name = format!("{}{}", prefix, n);
        let row = 16.0 + 34.0 * (self.form.controls.len() as f32 % 8.0);
        let spec = ControlSpec {
            name: name.clone(),
            kind,
            rect: if kind == ControlKind::Timer {
                Rect::new(0.0, 0.0, 0.0, 0.0)
            } else {
                Rect::new(16.0, row, 100.0, 28.0)
            },
            caption: if kind == ControlKind::CommandButton {
                name.clone()
            } else {
                String::new()
            },
            text: if kind == ControlKind::TextBox {
                Some(String::new())
            } else {
                None
            },
            items: vec![],
            enabled: true,
            visible: kind != ControlKind::Timer,
            interval_ms: if kind == ControlKind::Timer { 1000 } else { 0 },
            color: None,
        };
        self.form.controls.push(spec);
        self.selected = Some(self.form.controls.len() - 1);
    }
}

fn default_form() -> FormSpec {
    let mut form = FormSpec::new("Form1", "Form1", 360.0, 280.0);
    form.label("Label1", "Hello, OxidizedBASIC!", Rect::new(16.0, 16.0, 240.0, 20.0));
    form.button("Command1", "Click Me", Rect::new(16.0, 48.0, 96.0, 28.0));
    form.text_box("Text1", "", Rect::new(16.0, 88.0, 200.0, 24.0));
    form
}


/// Editor editing operations.
impl Ide {
    fn line_len(&self, row: usize) -> usize {
        self.lines.get(row).map(|l| l.chars().count()).unwrap_or(0)
    }

    fn insert_char(&mut self, c: char) {
        let (row, col) = self.cursor;
        let mut chars: Vec<char> = self.lines[row].chars().collect();
        chars.insert(col, c);
        self.lines[row] = chars.into_iter().collect();
        self.cursor.1 += 1;
    }

    fn backspace(&mut self) {
        let (row, col) = self.cursor;
        if col > 0 {
            let mut chars: Vec<char> = self.lines[row].chars().collect();
            chars.remove(col - 1);
            self.lines[row] = chars.into_iter().collect();
            self.cursor.1 -= 1;
        } else if row > 0 {
            let cur = self.lines.remove(row);
            let prev_len = self.lines[row - 1].chars().count();
            self.lines[row - 1].push_str(&cur);
            self.cursor = (row - 1, prev_len);
        }
    }

    fn newline(&mut self) {
        let (row, col) = self.cursor;
        let chars: Vec<char> = self.lines[row].chars().collect();
        let indent: String = self.lines[row].chars().take_while(|c| *c == ' ').collect();
        // auto-indent one level after block openers
        let extra = if self.lines[row].trim_end().ends_with("Then")
            || self.lines[row].trim_end().ends_with("Do")
        {
            "  "
        } else {
            ""
        };
        let rest: String = chars[col..].iter().collect();
        self.lines[row] = chars[..col].iter().collect();
        self.lines
            .insert(row + 1, format!("{}{}{}", indent, extra, rest));
        self.cursor = (row + 1, indent.chars().count() + extra.chars().count());
    }

    fn editor_key(&mut self, ev: &KeyDownEvent) {
        let key = ev.keystroke.key.as_str();
        match key {
            "enter" => self.newline(),
            "backspace" => self.backspace(),
            "delete" => {
                let (row, col) = self.cursor;
                if col < self.line_len(row) {
                    self.cursor.1 += 1;
                    self.backspace();
                } else if row + 1 < self.lines.len() {
                    let next = self.lines.remove(row + 1);
                    self.lines[row].push_str(&next);
                }
            }
            "left" => {
                if self.cursor.1 > 0 {
                    self.cursor.1 -= 1;
                } else if self.cursor.0 > 0 {
                    self.cursor.0 -= 1;
                    self.cursor.1 = self.line_len(self.cursor.0);
                }
            }
            "right" => {
                if self.cursor.1 < self.line_len(self.cursor.0) {
                    self.cursor.1 += 1;
                } else if self.cursor.0 + 1 < self.lines.len() {
                    self.cursor.0 += 1;
                    self.cursor.1 = 0;
                }
            }
            "up" => {
                if self.cursor.0 > 0 {
                    self.cursor.0 -= 1;
                    self.cursor.1 = self.cursor.1.min(self.line_len(self.cursor.0));
                }
            }
            "down" => {
                if self.cursor.0 + 1 < self.lines.len() {
                    self.cursor.0 += 1;
                    self.cursor.1 = self.cursor.1.min(self.line_len(self.cursor.0));
                }
            }
            "home" => self.cursor.1 = 0,
            "end" => self.cursor.1 = self.line_len(self.cursor.0),
            "tab" => {
                self.insert_char(' ');
                self.insert_char(' ');
            }
            "space" => self.insert_char(' '),
            _ => {
                if key.chars().count() == 1 {
                    let c = key.chars().next().unwrap();
                    let c = if ev.keystroke.modifiers.shift {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    };
                    self.insert_char(c);
                }
            }
        }
    }
}

/// Layout & rendering.
impl Render for Ide {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(FG))
            .text_sm()
            .font_family("Menlo")
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w_3_5()
                            .min_w_0()
                            .child(div().flex_1().min_h_0().child(self.render_editor(cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .border_l_1()
                            .border_color(rgb(BORDER))
                            .child(div().flex_1().min_h_0().child(self.render_designer(cx)))
                            .child(self.render_inspector(cx)),
                    ),
            )
            .child(self.render_output())
    }
}

impl Ide {
    fn tool_button(
        &self,
        label: &str,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Ide) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(SharedString::from(format!(
                "btn-{}-{}",
                label,
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            )))
            .px_3()
            .py_1()
            .mx_1()
            .rounded_sm()
            .bg(rgb(TOOLBAR))
            .border_1()
            .border_color(rgb(BORDER))
            .hover(|s| s.bg(rgb(ACCENT)))
            .text_color(rgb(FG))
            .child(label.to_string())
            .on_click(cx.listener(move |this: &mut Ide, _ev, _win, _cx| action(this)))
    }

    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.session.is_some();
        div()
            .flex()
            .items_center()
            .h_9()
            .px_2()
            .bg(rgb(TOOLBAR))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(if running {
                self.tool_button("Continue", cx, |this| this.run_program())
            } else {
                self.tool_button("Run", cx, |this| this.run_program())
            })
            .child(self.tool_button("Step", cx, |this| this.step_program()))
            .child(self.tool_button("Stop", cx, |this| this.stop_program()))
            .child(div().w_px().h_6().mx_2().bg(rgb(BORDER)))
            .child(self.tool_button("+ Button", cx, |this| {
                this.add_control(ControlKind::CommandButton)
            }))
            .child(self.tool_button("+ Label", cx, |this| {
                this.add_control(ControlKind::Label)
            }))
            .child(self.tool_button("+ TextBox", cx, |this| {
                this.add_control(ControlKind::TextBox)
            }))
            .child(self.tool_button("+ Timer", cx, |this| {
                this.add_control(ControlKind::Timer)
            }))
            .child(div().w_px().h_6().mx_2().bg(rgb(BORDER)))
            .child(self.tool_button("Save Code", cx, |this| this.save_code()))
            .child(self.tool_button("Save Form", cx, |this| this.save_form()))
            .child(
                div()
                    .flex_1()
                    .ml_3()
                    .text_color(rgb(DIM))
                    .child(SharedString::from(self.status.clone())),
            )
    }
}

/// The code editor pane.
impl Ide {
    fn render_editor(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let bps = self.breakpoints.lock().unwrap().clone();
        let paused_line = self.paused.as_ref().map(|p| p.line);
        let mut list = div()
            .id("editor-scroll")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(BG));

        for (idx, line) in self.lines.clone().into_iter().enumerate() {
            let lineno = idx + 1;
            let has_bp = bps.contains(&lineno);
            let is_paused = paused_line == Some(lineno);
            let is_cursor = self.cursor.0 == idx;

            let mut gutter = div()
                .id((SharedString::from("gutter"), idx))
                .w_10()
                .flex()
                .justify_end()
                .items_center()
                .pr_1()
                .text_color(rgb(if has_bp { 0xF14C4C } else { DIM }))
                .cursor_pointer()
                .child(if has_bp {
                    SharedString::from("\u{25cf} ")
                } else {
                    SharedString::from(format!("{} ", lineno))
                });
            gutter = gutter.on_click(cx.listener(
                move |this: &mut Ide, _ev: &gpui::ClickEvent, _win, _cx| {
                    this.toggle_breakpoint(lineno);
                },
            ));

            let mut code = div().flex_1().pl_2();
            if is_paused {
                code = code.bg(rgb(SEL_BG));
            }
            if is_cursor {
                let col = self.cursor.1;
                let chars: Vec<char> = line.chars().collect();
                let before: String = chars[..col.min(chars.len())].iter().collect();
                let at: String = chars
                    .get(col)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| " ".into());
                let after: String = chars[(col + 1).min(chars.len())..].iter().collect();
                code = code
                    .child(render_spans(&highlight::tokenize_line(&before)))
                    .child(
                        div()
                            .bg(rgb(0xDDDDDD))
                            .text_color(rgb(BG))
                            .child(SharedString::from(at)),
                    )
                    .child(render_spans(&highlight::tokenize_line(&after)));
            } else {
                code = code.child(render_spans(&highlight::tokenize_line(&line)));
            }

            list = list.child(div().flex().h_5().items_center().child(gutter).child(code));
        }

        div()
            .id("editor")
            .size_full()
            .track_focus(&self.editor_focus)
            .on_key_down(cx.listener(|this: &mut Ide, ev: &KeyDownEvent, _win, cx| {
                this.editor_key(ev);
                cx.notify();
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut Ide, _ev: &MouseDownEvent, win, _cx| {
                    // click-to-type: focus the editor
                    win.focus(&this.editor_focus);
                }),
            )
            .child(list)
    }
}

fn render_spans(toks: &[(String, highlight::Tok)]) -> gpui::Div {
    let mut row = div().flex();
    for (text, kind) in toks {
        let color = match kind {
            highlight::Tok::Keyword => C_KEYWORD,
            highlight::Tok::Str => C_STRING,
            highlight::Tok::Num => C_NUMBER,
            highlight::Tok::Comment => C_COMMENT,
            highlight::Tok::Ident => C_IDENT,
            highlight::Tok::Punct => C_PUNCT,
        };
        row = row.child(
            div()
                .text_color(rgb(color))
                .child(SharedString::from(text.clone())),
        );
    }
    row
}


/// The visual form designer: drag controls to move them; the property
/// inspector below edits the selected control; Save Form writes `.oxform`.
impl Ide {
    fn render_designer(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let form = self.form.clone();
        let selected = self.selected;

        // capture the form area origin for mouse coordinate conversion
        let origin = self.canvas_origin.clone();
        let bounds_capture = canvas(
            move |bounds: Bounds<Pixels>, _win: &mut Window, _cx: &mut App| bounds,
            move |bounds: Bounds<Pixels>, _state, _win: &mut Window, _cx: &mut App| {
                *origin.lock().unwrap() = Some(bounds.origin);
            },
        )
        .absolute()
        .size_full();

        let mut form_area = div()
            .relative()
            .w(px(form.width))
            .h(px(form.height))
            .bg(rgb(FORM_BG))
            .border_1()
            .border_color(rgb(CTRL_BORDER))
            .child(bounds_capture)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut Ide, _ev: &MouseDownEvent, _win, _cx| {
                    this.selected = None; // background click deselects
                }),
            );

        for (idx, c) in form.controls.iter().enumerate() {
            if !c.visible {
                continue;
            }
            let is_sel = selected == Some(idx);
            let mut el = div()
                .id((SharedString::from("ctrl"), idx))
                .absolute()
                .left(px(c.rect.left))
                .top(px(c.rect.top))
                .w(px(c.rect.width.max(8.0)))
                .h(px(c.rect.height.max(8.0)))
                .flex()
                .items_center()
                .pl_1()
                .bg(rgb(match c.kind {
                    ControlKind::TextBox | ControlKind::ListBox => 0xFFFFFF,
                    ControlKind::PictureBox => c.color.unwrap_or(0xC8C8C8),
                    _ => CTRL_BG,
                }))
                .border_1()
                .border_color(rgb(if is_sel { ACCENT } else { CTRL_BORDER }))
                .child(SharedString::from(if c.kind == ControlKind::CommandButton {
                    c.caption.clone()
                } else {
                    c.text().to_string()
                }));
            if is_sel {
                el = el.border_2();
            }
            el = el.on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(
                    move |this: &mut Ide, ev: &MouseDownEvent, _win, _cx| {
                        this.selected = Some(idx);
                        let origin = this.canvas_origin.lock().unwrap();
                        if let Some(o) = *origin {
                            let px = f32::from(ev.position.x) - f32::from(o.x);
                            let py = f32::from(ev.position.y) - f32::from(o.y);
                            let off_x = px - this.form.controls[idx].rect.left;
                            let off_y = py - this.form.controls[idx].rect.top;
                            this.drag = Some((idx, off_x, off_y));
                        }
                    },
                ),
            );
            form_area = form_area.child(el);
        }

        // the designer region tracks mouse movement for dragging
        div()
            .id("designer")
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(rgb(PANEL))
            .flex()
            .items_start()
            .justify_start()
            .p_4()
            .child(form_area)
            .on_mouse_move(cx.listener(|this: &mut Ide, ev: &MouseMoveEvent, _win, cx| {
                if let Some((idx, off_x, off_y)) = this.drag {
                    if !ev.dragging() {
                        this.drag = None;
                        return;
                    }
                    let origin = this.canvas_origin.lock().unwrap();
                    if let Some(o) = *origin {
                        let c = &mut this.form.controls[idx];
                        let px = f32::from(ev.position.x) - f32::from(o.x);
                        let py = f32::from(ev.position.y) - f32::from(o.y);
                        c.rect.left = (px - off_x).max(0.0).round();
                        c.rect.top = (py - off_y).max(0.0).round();
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut Ide, _ev: &gpui::MouseUpEvent, _win, _cx| {
                    this.drag = None;
                }),
            )
    }
}


/// The property inspector for the selected control.
impl Ide {
    fn prop_row(
        &mut self,
        idx: usize,
        key: &'static str,
        label: &str,
        value: String,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let buf_key = (idx, key);
        let text = self.prop_bufs.entry(buf_key).or_insert(value).clone();
        div()
            .id((SharedString::from(key), idx))
            .flex()
            .items_center()
            .gap_2()
            .child(div().w_16().text_color(rgb(DIM)).child(label.to_string()))
            .child(
                div()
                    .flex_1()
                    .px_1()
                    .py_0p5()
                    .bg(rgb(BG))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .track_focus(&self.inspector_focus)
                    .child(SharedString::from(text))
                    .on_key_down(cx.listener(
                        move |this: &mut Ide, ev: &KeyDownEvent, _win, cx| {
                            let key_name = ev.keystroke.key.as_str();
                            let k = (idx, key);
                            {
                                let buf = this.prop_bufs.entry(k).or_default();
                                match key_name {
                                    "backspace" => {
                                        buf.pop();
                                    }
                                    "space" => buf.push(' '),
                                    _ => {
                                        if key_name.chars().count() == 1 {
                                            let ch = key_name.chars().next().unwrap();
                                            buf.push(if ev.keystroke.modifiers.shift {
                                                ch.to_ascii_uppercase()
                                            } else {
                                                ch
                                            });
                                        }
                                    }
                                }
                            }
                            // live-apply to the control
                            let val = this.prop_bufs.get(&k).cloned().unwrap_or_default();
                            if let Some(c) = this.form.controls.get_mut(idx) {
                                match key {
                                    "caption" => c.caption = val.clone(),
                                    "text" => c.text = Some(val.clone()),
                                    "items" => {
                                        c.items = val
                                            .split(',')
                                            .map(|s| s.trim().to_string())
                                            .filter(|s| !s.is_empty())
                                            .collect()
                                    }
                                    "left" => {
                                        if let Ok(v) = val.parse::<f32>() {
                                            c.rect.left = v
                                        }
                                    }
                                    "top" => {
                                        if let Ok(v) = val.parse::<f32>() {
                                            c.rect.top = v
                                        }
                                    }
                                    "width" => {
                                        if let Ok(v) = val.parse::<f32>() {
                                            c.rect.width = v
                                        }
                                    }
                                    "height" => {
                                        if let Ok(v) = val.parse::<f32>() {
                                            c.rect.height = v
                                        }
                                    }
                                    "interval" => {
                                        c.interval_ms = val.parse().unwrap_or(c.interval_ms)
                                    }
                                    _ => {}
                                }
                            }
                            cx.notify();
                        },
                    )),
            )
    }
}


/// Inspector panel and output/watch panes.
impl Ide {
    fn render_inspector(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .h_64()
            .border_t_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PANEL))
            .p_2()
            .flex()
            .flex_col()
            .gap_1();

        match self.selected {
            None => {
                panel = panel.child(
                    div()
                        .text_color(rgb(DIM))
                        .child("Property Inspector — select a control"),
                );
            }
            Some(idx) => {
                let c = self.form.controls[idx].clone();
                panel = panel.child(
                    div()
                        .text_color(rgb(DIM))
                        .pb_1()
                        .child(SharedString::from(format!(
                            "{} — {}",
                            c.name,
                            c.kind.basic_name()
                        ))),
                );
                panel = panel.child(
                    self.prop_row(idx, "caption", "Caption", c.caption.clone(), cx),
                );
                if c.kind == ControlKind::TextBox {
                    panel = panel.child(
                        self.prop_row(idx, "text", "Text", c.text.clone().unwrap_or_default(), cx),
                    );
                }
                if matches!(c.kind, ControlKind::ListBox | ControlKind::ComboBox) {
                    panel = panel
                        .child(self.prop_row(idx, "items", "Items", c.items.join(", "), cx));
                }
                panel = panel.child(
                    self.prop_row(idx, "left", "Left", format!("{}", c.rect.left), cx),
                );
                panel = panel.child(
                    self.prop_row(idx, "top", "Top", format!("{}", c.rect.top), cx),
                );
                panel = panel.child(
                    self.prop_row(idx, "width", "Width", format!("{}", c.rect.width), cx),
                );
                panel = panel.child(
                    self.prop_row(idx, "height", "Height", format!("{}", c.rect.height), cx),
                );
                if c.kind == ControlKind::Timer {
                    panel = panel.child(self.prop_row(
                        idx,
                        "interval",
                        "Interval",
                        format!("{}", c.interval_ms),
                        cx,
                    ));
                }
            }
        }
        panel
    }

    fn render_output(&mut self) -> impl IntoElement {
        let mut panel = div()
            .h_40()
            .border_t_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PANEL))
            .p_2()
            .flex()
            .gap_2()
            .overflow_hidden();
        // output / immediate window
        panel = panel.child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().text_color(rgb(DIM)).pb_1().child("Output"))
                .child(
                    div()
                        .id("output-scroll")
                        .flex_1()
                        .overflow_y_scroll()
                        .child(SharedString::from(self.output.clone())),
                ),
        );
        // watch pane
        let mut watch = div()
            .w_2_5()
            .flex()
            .flex_col()
            .child(div().text_color(rgb(DIM)).pb_1().child("Watch"));
        if let Some(p) = &self.paused {
            watch = watch.child(
                div()
                    .text_color(rgb(0x4EC9B0))
                    .pb_1()
                    .child(SharedString::from(format!(
                        "line {} — {}",
                        p.line,
                        if p.proc.is_empty() { "(module)" } else { &p.proc }
                    ))),
            );
            for (name, value) in p.locals.iter().chain(p.globals.iter()) {
                watch = watch.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            div()
                                .text_color(rgb(C_IDENT))
                                .child(SharedString::from(name.clone())),
                        )
                        .child(
                            div()
                                .text_color(rgb(C_STRING))
                                .child(SharedString::from(value.clone())),
                        ),
                );
            }
        } else {
            watch = watch.child(
                div()
                    .text_color(rgb(DIM))
                    .child("not paused — Run or Step with breakpoints"),
            );
        }
        panel.child(watch)
    }
}

