//! The OxidizedBASIC IDE (Phase 3): a GPUI application with
//!
//! - a syntax-highlighted code editor with a breakpoint gutter,
//! - a visual form designer (drag-to-move, add controls, save `.oxform`),
//! - a property inspector for the selected control,
//! - an output / immediate pane,
//! - debug controls (run, step-over, stop) with a variable watch pane.

pub mod code_model;
pub mod code_window;
pub mod debug_session;
pub mod highlight;

use debug_session::{DebugCommand, DebugEvent, DebugSession};
#[cfg(test)]
use gpui::point;
use oxbasic_runtime::PausedInfo;
use oxbasic_ui::gpui_controls::{
    edit_text, paint_control, typed_text as ks_text, ControlLook, ControlState, Part, FORM_BACK,
};
use oxbasic_ui::{
    default_event, event_params, event_signatures, events_for_control, ControlKind, ControlSpec,
    FormSpec, Rect,
};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use gpui::{
    deferred, div, prelude::*, px, rgb, size, AnyElement, AnyWindowHandle, App, Application,
    Bounds, Context, Entity, FocusHandle, KeyDownEvent, MouseDownEvent, MouseMoveEvent,
    ScrollHandle, SharedString, Subscription, Window, WindowBounds, WindowHandle, WindowOptions,
};

// macOS-inspired system surfaces. The workspace stays familiar to VB users,
// while the visual language belongs to the host platform.
const BG: u32 = 0xF5F5F7;
const PANEL: u32 = 0xFFFFFF;
const TOOLBAR: u32 = 0xF5F5F7;
const BORDER: u32 = 0xD2D2D7;
const FG: u32 = 0x1D1D1F;
const DIM: u32 = 0x6E6E73;
const ACCENT: u32 = 0x007AFF;
const SEL_BG: u32 = 0xDCEEFF;
const C_KEYWORD: u32 = 0x9B2393;
const C_STRING: u32 = 0xC41A16;
const C_NUMBER: u32 = 0x1C00CF;
const C_COMMENT: u32 = 0x5D6C79;
const C_IDENT: u32 = 0x1D1D1F;
const C_PUNCT: u32 = 0x1D1D1F;
const FORM_BG: u32 = FORM_BACK;
const CTRL_BORDER: u32 = 0xC7C7CC;
const WINDOW_BG: u32 = 0xFFFFFF;
const TITLE_BG: u32 = 0xF5F5F7;
const ERROR: u32 = 0xD70015;

/// Inspector rows for the form itself use this in place of a control index.
const FORM_IDX: usize = usize::MAX;
/// The code window's object list entry for module-level code.
const GENERAL: &str = "(General)";
/// The code window's procedure list entry for the module-level code.
const DECLARATIONS: &str = "(Declarations)";
/// Width of the code window's object / procedure select boxes.
const SELECT_W: f32 = 240.0;
/// Height of one editor row (`h_5`).
const ROW_H: f32 = 20.0;
/// Editor gutter width plus the code's left padding.
const CODE_X: f32 = 56.0;

/// Which of the code window's two select boxes has its list open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeMenu {
    Object,
    Event,
}

struct ScrollbarDrag {
    pane: &'static str,
    vertical: bool,
    pointer_start: f32,
    offset_start: f32,
    max_offset: f32,
    thumb_travel: f32,
}

gpui::actions!(
    ide,
    [
        Quit,
        SaveCode,
        SaveForm,
        RunProgram,
        StepProgram,
        StopProgram,
        ViewCode,
        ViewObject
    ]
);

/// Launch the IDE, optionally loading a `.bas` source file and an
/// `.oxform` layout file.
pub fn run_ide(bas_path: Option<&str>, oxform_path: Option<&str>) {
    let bas_path = bas_path.map(|s| s.to_string());
    let oxform_path = oxform_path.map(|s| s.to_string());
    Application::new().run(move |cx: &mut App| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-q", Quit, None),
            gpui::KeyBinding::new("cmd-s", SaveCode, None),
            gpui::KeyBinding::new("cmd-shift-s", SaveForm, None),
            gpui::KeyBinding::new("cmd-r", RunProgram, None),
            gpui::KeyBinding::new("cmd-shift-r", StopProgram, None),
            gpui::KeyBinding::new("f7", ViewCode, None),
            gpui::KeyBinding::new("shift-f7", ViewObject, None),
        ]);
        cx.set_menus(vec![
            gpui::Menu {
                name: "OxidizedBASIC".into(),
                items: vec![gpui::MenuItem::action("Quit OxidizedBASIC", Quit)],
            },
            gpui::Menu {
                name: "File".into(),
                items: vec![
                    gpui::MenuItem::action("Save Code", SaveCode),
                    gpui::MenuItem::action("Save Form", SaveForm),
                ],
            },
            gpui::Menu {
                name: "View".into(),
                items: vec![
                    gpui::MenuItem::action("Code", ViewCode),
                    gpui::MenuItem::action("Object", ViewObject),
                ],
            },
            gpui::Menu {
                name: "Run".into(),
                items: vec![
                    gpui::MenuItem::action("Run / Continue", RunProgram),
                    gpui::MenuItem::action("Step Over", StepProgram),
                    gpui::MenuItem::action("Stop", StopProgram),
                ],
            },
        ]);
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
                win.focus(&entity.read(cx).designer_focus);
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
    /// Focus for the designer, so window-level shortcuts (F7) reach the
    /// IDE while no text field is being edited.
    designer_focus: FocusHandle,
    editor_scroll: ScrollHandle,
    designer_scroll: ScrollHandle,
    output_scroll: ScrollHandle,
    scrollbar_drag: Option<ScrollbarDrag>,
    scrollbars_initialized: bool,
    breakpoints: Arc<Mutex<HashSet<usize>>>,
    // form designer
    form: FormSpec,
    selected: Option<usize>,
    drag: Option<DesignDrag>, // moving or resizing a control, with the last pointer position
    // property inspector buffers: (control idx or FORM_IDX, prop) -> live text
    prop_bufs: HashMap<(usize, &'static str), String>,
    /// one focus handle per inspector field, so keystrokes always edit the
    /// row that was clicked
    prop_focus: HashMap<(usize, &'static str), FocusHandle>,
    /// A rejected `Name` edit: the control (or FORM_IDX) and why.
    rename_error: Option<(usize, String)>,
    /// The code window, when open. Code is shown only on request (View
    /// Code, double-clicking a control), in its own window.
    code_window: Option<AnyWindowHandle>,
    /// The code window's open select box, if any.
    code_menu: Option<CodeMenu>,
    /// The code window's object list choice when it differs from what
    /// the caret is in (e.g. a control was picked but no event yet).
    code_object: Option<String>,
    /// Scroll the caret into view on the next code-window frame.
    reveal_cursor: bool,
    // output & debug
    output: String,
    session: Option<DebugSession>,
    paused: Option<PausedInfo>,
    /// The program finished its module-level code and is serving events.
    live: bool,
    /// The live form's own OS window — in classic VB the form *is* the
    /// application window, not a pane inside the IDE.
    form_window: Option<WindowHandle<FormRunner>>,
    status: String,
    evt_rx: Arc<Mutex<Receiver<DebugEvent>>>,
    evt_tx: Sender<DebugEvent>,
}

impl Ide {
    pub fn new(bas_path: Option<&str>, oxform_path: Option<&str>, cx: &mut Context<Self>) -> Ide {
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
            designer_focus: cx.focus_handle(),
            editor_scroll: ScrollHandle::new(),
            designer_scroll: ScrollHandle::new(),
            output_scroll: ScrollHandle::new(),
            scrollbar_drag: None,
            scrollbars_initialized: false,
            breakpoints: Arc::new(Mutex::new(HashSet::new())),
            form,
            selected: None,
            drag: None,
            prop_bufs: HashMap::new(),
            prop_focus: HashMap::new(),
            rename_error: None,
            code_window: None,
            code_menu: None,
            code_object: None,
            reveal_cursor: false,
            output: String::new(),
            session: None,
            paused: None,
            live: false,
            form_window: None,
            status: "Ready".into(),
            evt_rx,
            evt_tx,
        };

        // Start with the editor focused so typing works immediately, and
        // leave a hint in the output pane.
        ide.output.push_str(
            "oxbasic IDE\n\
             \u{2022} double-click a control (or the form) to edit its event code\n\
             \u{2022} View Code (F7) opens the code window; pick an object and event in its header\n\
             \u{2022} select a control to edit its properties, including (Name), in Properties\n\
             \u{2022} drag controls to move them; add more from the Toolbox\n\
             \u{2022} Run opens the form in its own window; Step pauses at each statement\n",
        );

        // Test hook: OXBASIC_IDE_AUTORUN=1 runs the program automatically
        // shortly after launch (used by scripts/ide_smoke.sh to verify the
        // Run pipeline end-to-end with a screenshot).
        if std::env::var("OXBASIC_IDE_AUTORUN").ok().as_deref() == Some("1") {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(800))
                    .await;
                let _ = this.update(cx, |ide, cx| {
                    ide.run_program();
                    cx.notify();
                });
            })
            .detach();
        }

        // Poll debug events and pump them into the entity.
        let rx = ide.evt_rx.clone();
        cx.spawn(async move |this, cx| loop {
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
                    ide.on_debug_event(ev, cx);
                    cx.notify();
                });
            }
        })
        .detach();

        ide
    }

    fn on_debug_event(&mut self, ev: DebugEvent, cx: &mut Context<Self>) {
        match ev {
            DebugEvent::Paused(info) => {
                self.status = format!(
                    "Paused at line {} in {}",
                    info.line,
                    if info.proc.is_empty() {
                        "(module)"
                    } else {
                        &info.proc
                    }
                );
                self.paused = Some(info);
                // Show where execution stopped: the code is only on
                // screen on request, and a breakpoint hit is a request.
                if let Some(p) = &self.paused {
                    let row = p
                        .line
                        .saturating_sub(1)
                        .min(self.lines.len().saturating_sub(1));
                    self.cursor = (row, 0);
                    self.code_object = None;
                }
                self.show_code_window(cx);
            }
            DebugEvent::Output(s) => {
                self.output.push_str(&s);
                if !self.live {
                    self.status = "Running".into();
                }
            }
            DebugEvent::Live => {
                self.live = true;
                self.paused = None;
                self.status =
                    "Live — the form runs in its own window; Stop (or closing it) ends the program"
                        .into();
                self.open_form_window(cx);
            }
            DebugEvent::Finished(msg) => {
                self.output.push_str(&format!("[{}]\n", msg));
                self.status = msg;
                self.paused = None;
                self.session = None;
                self.live = false;
                self.close_form_window(cx);
            }
            DebugEvent::Failed(msg) => {
                self.output.push_str(&format!("[error] {}\n", msg));
                self.status = msg;
                self.session = None;
                self.live = false;
                self.close_form_window(cx);
            }
        }
    }

    fn source(&self) -> String {
        self.lines.join("\n")
    }

    /// Open the running program's form in its own OS window — in classic
    /// VB the form *is* the application window the generated app shows,
    /// not a pane inside the IDE.
    fn open_form_window(&mut self, cx: &mut Context<Self>) {
        if self.form_window.is_some() {
            return;
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        let form = self.form.clone();
        let ide = cx.entity().downgrade();
        let bounds = Bounds::centered(
            None,
            size(px(form.width + 16.0), px(form.height + 16.0)),
            cx,
        );
        let title = SharedString::from(form.caption.clone());
        match cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(title),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |win, cx| {
                let runner = cx.new(|cx| FormRunner::new(form.clone(), session.clone(), cx));
                // Closing the form window stops the program, like closing
                // a VB app's main form ends the app.
                let weak = ide.clone();
                win.on_window_should_close(cx, move |_win, cx| {
                    if let Some(ide) = weak.upgrade() {
                        ide.update(cx, |ide, cx| {
                            if ide.live {
                                ide.stop_program(cx);
                            }
                        });
                    }
                    true
                });
                runner
            },
        ) {
            Ok(handle) => self.form_window = Some(handle),
            Err(e) => self
                .output
                .push_str(&format!("[form window failed: {}]\n", e)),
        }
    }

    /// Take down the live form's OS window (program finished, failed, or
    /// was stopped).
    fn close_form_window(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.form_window.take() {
            let _ = handle.update(cx, |_, win, _| win.remove_window());
        }
    }
}

/// Toolbar actions and control additions.
impl Ide {
    fn run_program(&mut self) {
        if let Some(s) = &self.session {
            s.send(DebugCommand::Continue);
            return;
        }
        self.output.clear();
        self.output.push_str("--- run ---\n");
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
        self.output.clear();
        self.output.push_str("--- step ---\n");
        let session =
            DebugSession::start(self.source(), self.breakpoints.clone(), self.evt_tx.clone());
        session.send(DebugCommand::Step);
        self.session = Some(session);
        self.status = "Stepping".into();
    }

    fn stop_program(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = &self.session.take() {
            s.send(DebugCommand::Stop);
            self.output.push_str("[stopped]\n");
        }
        self.paused = None;
        self.live = false;
        self.close_form_window(cx);
        self.status = "Stopped".into();
    }

    /// View > Code (F7): the code window at the selected control's default
    /// event (or the form's when nothing is selected), like VB's View Code.
    fn view_code(&mut self, cx: &mut Context<Self>) {
        self.commit_pending_rename();
        match self.selected {
            Some(idx) => self.open_default_event(Some(idx), cx),
            None => self.show_code_window(cx),
        }
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
        // The first free `<Prefix><n>` — renamed controls can leave gaps or
        // already hold the next number.
        let name = (1..)
            .map(|n| format!("{}{}", prefix, n))
            .find(|n| {
                !self
                    .form
                    .controls
                    .iter()
                    .any(|c| c.name.eq_ignore_ascii_case(n))
                    && !self.form.name.eq_ignore_ascii_case(n)
            })
            .unwrap();
        // Drop the control at the first spot in the form's left column that
        // doesn't overlap an existing control, so repeated additions never
        // stack invisibly on top of each other.
        let (w, h) = kind.default_size();
        let mut top = 16.0_f32;
        loop {
            let hit = self
                .form
                .controls
                .iter()
                .filter(|c| c.kind != ControlKind::Timer)
                .filter(|c| {
                    c.rect.left < 16.0 + w
                        && c.rect.left + c.rect.width > 16.0
                        && c.rect.top < top + h
                        && c.rect.top + c.rect.height > top
                })
                .map(|c| c.rect.top + c.rect.height)
                .fold(None, |m: Option<f32>, b| Some(m.map_or(b, |m| m.max(b))));
            match hit {
                Some(bottom) if top < 2000.0 => top = bottom + 8.0,
                _ => break,
            }
        }
        // As in VB6, a new Label, CommandButton or Frame is captioned with
        // its name, and a new TextBox or ComboBox shows its name as text.
        let spec = ControlSpec {
            name: name.clone(),
            kind,
            rect: if kind == ControlKind::Timer {
                Rect::new(0.0, 0.0, 0.0, 0.0)
            } else {
                Rect::new(16.0, top, w, h)
            },
            caption: if kind.has_caption() {
                name.clone()
            } else {
                String::new()
            },
            text: if kind.has_text() {
                Some(name.clone())
            } else {
                None
            },
            items: vec![],
            enabled: true,
            visible: kind != ControlKind::Timer,
            interval_ms: if kind == ControlKind::Timer { 1000 } else { 0 },
            color: None,
            style: 0,
        };
        self.form.controls.push(spec);
        self.select(Some(self.form.controls.len() - 1));
    }
}

#[cfg(test)]
mod toolbox_tests;
#[cfg(test)]
mod designer_tests {
    use super::*;

    #[gpui::test]
    fn stopped_control_click_selects_inspector_and_background_deselects(
        cx: &mut gpui::TestAppContext,
    ) {
        let (ide, cx) = cx.add_window_view(|_, cx| Ide::new(None, None, cx));
        cx.simulate_resize(size(px(1400.0), px(900.0)));
        ide.update(cx, |ide, cx| {
            ide.stop_program(cx);
            cx.notify();
        });
        cx.run_until_parked();

        let control = cx
            .debug_bounds("designer-control-1")
            .expect("button bounds");
        cx.simulate_click(control.center(), gpui::Modifiers::none());
        ide.update(cx, |ide, cx| {
            assert_eq!(ide.selected, Some(1));
            assert_eq!(ide.form.controls[1].name, "Command1");
            // Property rows are constructed only for the selected control.
            assert!(ide.prop_bufs.contains_key(&(1, "caption")));
            cx.notify();
        });
        cx.run_until_parked();
        ide.update(cx, |ide, _| assert_eq!(ide.selected, Some(1)));

        let form = cx.debug_bounds("designer-form").expect("form bounds");
        let background = form.origin + gpui::point(px(8.0), px(8.0));
        cx.simulate_click(background, gpui::Modifiers::none());
        ide.update(cx, |ide, _| assert_eq!(ide.selected, None));
    }

    #[gpui::test]
    fn live_program_opens_and_closes_its_own_form_window(cx: &mut gpui::TestAppContext) {
        let (ide, cx) = cx.add_window_view(|_, cx| Ide::new(None, None, cx));
        cx.simulate_resize(size(px(1400.0), px(900.0)));

        ide.update(cx, |ide, cx| {
            // A real (tiny) session, so the form window has a program to
            // send its click events to.
            let (tx, _rx) = channel();
            let bps = Arc::new(Mutex::new(HashSet::new()));
            ide.session = Some(DebugSession::start("Print \"\"".into(), bps, tx));
            ide.on_debug_event(DebugEvent::Live, cx);
            cx.notify();
        });
        cx.run_until_parked();

        ide.update(cx, |ide, _| {
            assert!(ide.live);
            assert!(
                ide.form_window.is_some(),
                "going live must open the form in its own window"
            );
        });

        // The program ends -> its form window is taken down with it.
        ide.update(cx, |ide, cx| {
            ide.on_debug_event(DebugEvent::Finished("done".into()), cx);
            cx.notify();
        });
        cx.run_until_parked();
        ide.update(cx, |ide, _| assert!(ide.form_window.is_none()));
    }

    /// The code window opened by the IDE, as a test context of its own.
    fn code_window_cx(
        ide: &Entity<Ide>,
        cx: &mut gpui::VisualTestContext,
    ) -> gpui::VisualTestContext {
        cx.run_until_parked();
        let handle = ide
            .read_with(cx, |ide, _| ide.code_window)
            .expect("the code window must be open");
        let code = gpui::VisualTestContext::from_window(handle, cx);
        code.simulate_resize(size(px(900.0), px(640.0)));
        code.run_until_parked();
        code
    }

    fn double_click(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>) {
        cx.simulate_event(gpui::MouseDownEvent {
            button: gpui::MouseButton::Left,
            position: at,
            modifiers: gpui::Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(gpui::MouseUpEvent {
            button: gpui::MouseButton::Left,
            position: at,
            modifiers: gpui::Modifiers::none(),
            click_count: 2,
        });
    }

    #[gpui::test]
    fn code_is_hidden_until_requested(cx: &mut gpui::TestAppContext) {
        let (ide, cx) = cx.add_window_view(|_, cx| Ide::new(None, None, cx));
        cx.simulate_resize(size(px(1400.0), px(900.0)));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("editor-viewport").is_none(),
            "the designer window must not show the code listing"
        );
        ide.read_with(cx, |ide, _| assert!(ide.code_window.is_none()));

        // View Code shows it — in its own window.
        let view_code = cx.debug_bounds("toolbar-view-code").expect("View Code");
        cx.simulate_click(view_code.center(), gpui::Modifiers::none());
        let mut code = code_window_cx(&ide, cx);
        assert!(code.debug_bounds("editor-viewport").is_some());
        assert!(code.debug_bounds("code-object").is_some());
        assert!(code.debug_bounds("code-event").is_some());
        assert!(cx.debug_bounds("editor-viewport").is_none());
    }

    #[gpui::test]
    fn double_click_opens_the_default_event_with_a_placeholder(cx: &mut gpui::TestAppContext) {
        let (ide, cx) = cx.add_window_view(|_, cx| Ide::new(None, None, cx));
        cx.simulate_resize(size(px(1400.0), px(900.0)));
        ide.update(cx, |ide, cx| {
            ide.lines = vec!["Dim Total As Long".into()];
            cx.notify();
        });
        cx.run_until_parked();

        // Text1 (index 2) has no code yet: double-click adds Text1_Change.
        let text1 = cx.debug_bounds("designer-control-2").expect("Text1 bounds");
        double_click(cx, text1.center());
        let code = code_window_cx(&ide, cx);
        ide.read_with(&code, |ide, _| {
            let expected: Vec<String> = [
                "Dim Total As Long",
                "",
                "Private Sub Text1_Change()",
                "    ",
                "End Sub",
            ]
            .map(String::from)
            .to_vec();
            assert_eq!(ide.lines, expected);
            assert_eq!(ide.cursor, (3, 4), "caret in the placeholder's body");
            assert_eq!(ide.selected, Some(2));
            assert_eq!(
                ide.code_selection(),
                ("Text1".to_string(), "Change".to_string()),
                "the select boxes default to the opened control and event"
            );
        });

        // Opening it again doesn't duplicate the handler.
        ide.update(cx, |ide, cx| ide.open_default_event(Some(2), cx));
        cx.run_until_parked();
        ide.read_with(cx, |ide, _| {
            let n = ide
                .lines
                .iter()
                .filter(|l| l.contains("Text1_Change"))
                .count();
            assert_eq!(n, 1);
        });

        // Double-clicking the form background opens Form_Load.
        let form = cx.debug_bounds("designer-form").expect("form bounds");
        double_click(cx, form.origin + point(px(300.0), px(200.0)));
        cx.run_until_parked();
        ide.read_with(cx, |ide, _| {
            assert!(ide.lines.iter().any(|l| l == "Private Sub Form_Load()"));
            assert_eq!(ide.selected, None);
        });
    }

    #[gpui::test]
    fn header_select_boxes_list_objects_and_events(cx: &mut gpui::TestAppContext) {
        let (ide, cx) = cx.add_window_view(|_, cx| Ide::new(None, None, cx));
        cx.simulate_resize(size(px(1400.0), px(900.0)));
        ide.update(cx, |ide, cx| {
            ide.lines = vec![
                "Sub Command1_Click()".into(),
                "    MsgBox \"hi\"".into(),
                "End Sub".into(),
            ];
            ide.open_event("Command1", Some("Click"), cx);
        });
        let mut code = code_window_cx(&ide, cx);
        ide.read_with(&code, |ide, _| {
            assert_eq!(
                ide.cursor,
                (1, 4),
                "an existing handler is opened, not duplicated"
            );
            assert_eq!(ide.lines.len(), 3);
            let objects = ide.code_objects();
            assert_eq!(
                objects,
                vec!["(General)", "Form", "Label1", "Command1", "Text1"]
            );
            let events = ide.code_events("Command1");
            assert!(
                events.contains(&("Click".to_string(), true)),
                "Click has code"
            );
            assert!(events.contains(&("MouseDown".to_string(), false)));
            assert!(
                !events.iter().any(|(e, _)| e == "Change"),
                "buttons don't raise Change"
            );
        });

        // Open the event list beside the object list and pick MouseDown.
        let object_box = code.debug_bounds("code-object").expect("object select");
        let event_box = code.debug_bounds("code-event").expect("event select");
        assert!(
            event_box.origin.x > object_box.origin.x,
            "events sit right of objects"
        );
        code.simulate_click(event_box.center(), gpui::Modifiers::none());
        let item = code
            .debug_bounds("code-event-list-MouseDown")
            .expect("MouseDown in the event list");
        code.simulate_click(item.center(), gpui::Modifiers::none());
        ide.read_with(&code, |ide, _| {
            let decl = "Private Sub Command1_MouseDown(Button As Integer, Shift As Integer, \
                        X As Single, Y As Single)";
            assert!(ide.lines.iter().any(|l| l == decl), "{:?}", ide.lines);
            assert_eq!(ide.code_selection().1, "MouseDown");
            assert_eq!(ide.code_menu, None, "picking closes the list");
        });

        // Pick another object: Form, which has no code -> Form_Load.
        code.simulate_click(object_box.center(), gpui::Modifiers::none());
        let form = code
            .debug_bounds("code-object-list-Form")
            .expect("Form in the object list");
        code.simulate_click(form.center(), gpui::Modifiers::none());
        ide.read_with(&code, |ide, _| {
            assert!(ide.lines.iter().any(|l| l == "Private Sub Form_Load()"));
            assert_eq!(
                ide.code_selection(),
                ("Form".to_string(), "Load".to_string())
            );
        });

        // Moving the caret back into Command1_Click updates both lists.
        ide.update(&mut code, |ide, _| ide.cursor = (1, 0));
        ide.read_with(&code, |ide, _| {
            assert_eq!(
                ide.code_selection(),
                ("Command1".to_string(), "Click".to_string())
            )
        });
    }

    #[gpui::test]
    fn renaming_a_control_renames_its_code(cx: &mut gpui::TestAppContext) {
        let (ide, cx) = cx.add_window_view(|_, cx| Ide::new(None, None, cx));
        cx.simulate_resize(size(px(1400.0), px(900.0)));
        ide.update(cx, |ide, cx| {
            ide.lines = vec![
                "Sub Command1_Click()".into(),
                "    Command1.Caption = \"Command1\"".into(),
                "End Sub".into(),
            ];
            cx.notify();
        });
        cx.run_until_parked();

        let button = cx.debug_bounds("designer-control-1").expect("Command1");
        cx.simulate_click(button.center(), gpui::Modifiers::none());
        let name = cx.debug_bounds("prop-name").expect("(Name) field");
        cx.simulate_click(name.center(), gpui::Modifiers::none());
        // Clear "Command1" and type a new name.
        cx.simulate_keystrokes(&["backspace"; 8].join(" "));
        cx.simulate_input("OkButton");
        ide.read_with(cx, |ide, _| {
            assert_eq!(
                ide.form.controls[1].name, "Command1",
                "applied only on Enter"
            );
        });
        cx.simulate_keystrokes("enter");
        ide.read_with(cx, |ide, _| {
            assert_eq!(ide.form.controls[1].name, "OkButton");
            let expected: Vec<String> = [
                "Sub OkButton_Click()",
                "    OkButton.Caption = \"Command1\"",
                "End Sub",
            ]
            .map(String::from)
            .to_vec();
            assert_eq!(ide.lines, expected);
            assert!(ide.rename_error.is_none());
        });
    }

    #[gpui::test]
    fn invalid_or_duplicate_names_are_rejected(cx: &mut gpui::TestAppContext) {
        let (ide, cx) = cx.add_window_view(|_, cx| Ide::new(None, None, cx));
        ide.update(cx, |ide, _| {
            for (bad, why) in [
                ("Text1", "already used"),
                ("text1", "already used"),
                ("1abc", "start with a letter"),
                ("my name", "letters, digits"),
                ("Dim", "reserved word"),
                ("", "required"),
            ] {
                let err = ide.rename_error(1, bad).unwrap_or_default();
                assert!(err.contains(why), "{:?}: {:?}", bad, err);
            }
            assert_eq!(ide.rename_error(1, "Command1"), None, "own name is fine");
            assert_eq!(ide.rename_error(1, "cmdOK_2"), None);

            // A rejected rename leaves the control untouched...
            ide.prop_bufs.insert((1, "name"), "Text1".into());
            assert!(!ide.commit_rename(1));
            assert_eq!(ide.form.controls[1].name, "Command1");
            assert!(ide.rename_error.is_some());
            // ...and selecting another control drops it.
            ide.select(Some(0));
            assert!(ide.rename_error.is_none());
            assert_eq!(ide.form.controls[1].name, "Command1");
        });
    }

    #[gpui::test]
    fn added_controls_get_unique_names_after_a_rename(cx: &mut gpui::TestAppContext) {
        let (ide, cx) = cx.add_window_view(|_, cx| Ide::new(None, None, cx));
        ide.update(cx, |ide, _| {
            // default form: Label1, Command1, Text1
            ide.form.controls[1].name = "Command2".into();
            ide.add_control(ControlKind::CommandButton);
            let names: Vec<_> = ide.form.controls.iter().map(|c| c.name.clone()).collect();
            assert_eq!(names, vec!["Label1", "Command2", "Text1", "Command1"]);
            ide.add_control(ControlKind::CommandButton);
            assert_eq!(ide.form.controls.last().unwrap().name, "Command3");
        });
    }

    #[gpui::test]
    fn wide_code_line_overflows_the_editor_viewport(cx: &mut gpui::TestAppContext) {
        let (ide, cx) = cx.add_window_view(|_, cx| Ide::new(None, None, cx));
        cx.simulate_resize(size(px(900.0), px(700.0)));
        ide.update(cx, |ide, cx| {
            ide.lines = vec![format!("Print \\\"{}\\\"", "x".repeat(300)), "short".into()];
            ide.show_code_window(cx);
        });
        let mut code = code_window_cx(&ide, cx);
        // a second frame, so the scroll handle knows its content size
        ide.update(&mut code, |_, cx| cx.notify());
        code.run_until_parked();
        let cx = &mut code;

        let viewport = cx
            .debug_bounds("editor-viewport")
            .expect("editor viewport bounds");
        let content = cx
            .debug_bounds("editor-content")
            .expect("editor content bounds");
        assert!(
            content.size.width > viewport.size.width,
            "wide source must create horizontal overflow: content={:?}, viewport={:?}",
            content,
            viewport
        );
        let long_row = cx.debug_bounds("editor-row-0").expect("long line bounds");
        let short_row = cx.debug_bounds("editor-row-1").expect("short line bounds");
        assert_eq!(
            long_row.size.width, short_row.size.width,
            "editor line gutters and row widths must align"
        );
        assert!(
            cx.debug_bounds("scrollbar-editor-horizontal").is_some(),
            "the editor must show a horizontal scrollbar when code overflows"
        );
    }
}

fn default_form() -> FormSpec {
    let mut form = FormSpec::new("Form1", "Form1", 360.0, 280.0);
    form.label(
        "Label1",
        "Hello, OxidizedBASIC!",
        Rect::new(16.0, 16.0, 240.0, 20.0),
    );
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
        if ev.keystroke.modifiers.platform || ev.keystroke.modifiers.control {
            return;
        }
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
                if let Some(s) = typed_text(ev) {
                    for c in s.chars() {
                        self.insert_char(c);
                    }
                }
            }
        }
    }

    /// Put the caret at the character under `pos` (window coordinates),
    /// measuring the line in the editor's monospaced font.
    fn place_cursor_at(&mut self, pos: gpui::Point<gpui::Pixels>, win: &mut Window) {
        let bounds = self.editor_scroll.bounds();
        let offset = self.editor_scroll.offset();
        let y = f32::from(pos.y - bounds.origin.y - offset.y);
        let x = f32::from(pos.x - bounds.origin.x - offset.x) - CODE_X;
        if y < 0.0 || self.lines.is_empty() {
            return;
        }
        let row = ((y / ROW_H) as usize).min(self.lines.len() - 1);
        let ts = win.text_system();
        let font_id = ts.resolve_font(&gpui::font("Menlo"));
        let size = win.text_style().font_size.to_pixels(win.rem_size());
        let mut acc = 0.0;
        let mut col = 0;
        for ch in self.lines[row].chars() {
            let w = ts
                .advance(font_id, size, ch)
                .map(|s| f32::from(s.width))
                .unwrap_or(8.0);
            if acc + w / 2.0 > x {
                break;
            }
            acc += w;
            col += 1;
        }
        self.cursor = (row, col);
    }
}

/// Layout & rendering.
impl Render for Ide {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.scrollbars_initialized {
            self.scrollbars_initialized = true;
            cx.on_next_frame(window, |_this, _window, cx| cx.notify());
        }
        div()
            .id("ide-workspace")
            .on_action(cx.listener(|this, _: &SaveCode, _win, cx| {
                this.save_code();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SaveForm, _win, cx| {
                this.save_form();
                cx.notify();
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
            .on_action(cx.listener(|this, _: &ViewCode, _win, cx| {
                this.view_code(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ViewObject, _win, cx| {
                this.hide_code_window(cx);
                cx.notify();
            }))
            .track_focus(&self.designer_focus)
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _win, cx| {
                this.update_scrollbar_drag(ev, cx);
                this.update_designer_drag(ev, cx);
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _ev: &gpui::MouseUpEvent, _win, _cx| {
                    this.scrollbar_drag = None;
                    this.drag = None;
                }),
            )
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(FG))
            .text_sm()
            .font_family(".AppleSystemUIFont")
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .p_1()
                    .gap_1()
                    .child(self.render_toolbox(cx))
                    .child(
                        // The form designer is the document; the code is
                        // only shown when requested, in its own window.
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(PANEL))
                            .child(self.document_title(&format!(
                                "{} · Designer — double-click a control to edit its code",
                                self.form.name
                            )))
                            .child(div().flex_1().min_h_0().child(self.render_designer(cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w_72()
                            .min_w_0()
                            .gap_2()
                            .child(self.render_project_explorer(cx))
                            .child(self.render_inspector(window, cx)),
                    ),
            )
            .child(self.render_output(cx))
            .child(
                div()
                    .h_5()
                    .px_2()
                    .flex()
                    .items_center()
                    .bg(rgb(PANEL))
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .text_color(rgb(DIM))
                    .child(SharedString::from(format!("Ready    {}", self.status))),
            )
    }
}

impl Ide {
    fn dock_title(&self, title: &str) -> gpui::Div {
        div()
            .h_7()
            .px_3()
            .flex()
            .items_center()
            .bg(rgb(TITLE_BG))
            .text_color(rgb(DIM))
            .font_weight(gpui::FontWeight::MEDIUM)
            .child(title.to_string())
    }

    fn document_title(&self, title: &str) -> gpui::Div {
        div()
            .h_8()
            .px_3()
            .flex()
            .items_center()
            .bg(rgb(PANEL))
            .text_color(rgb(FG))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(title.to_string())
    }

    fn render_project_explorer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let project = self
            .bas_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("Project1");
        let code_file = self
            .bas_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Module1.bas");
        let form_file = self
            .oxform_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Form1.oxform");
        div()
            .h_40()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(WINDOW_BG))
            .flex()
            .flex_col()
            .child(self.dock_title("Project"))
            .child(
                div()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(SharedString::from(format!("▾  {}", project)))
                    .child(
                        div()
                            .id("project-form")
                            .px_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(SEL_BG)))
                            .child(SharedString::from(format!("    ◻  {}", form_file)))
                            .on_click(cx.listener(|this: &mut Ide, _ev, _win, cx| {
                                this.hide_code_window(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("project-code")
                            .px_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(SEL_BG)))
                            .child(SharedString::from(format!(
                                "    ◧  {}  (double-click for code)",
                                code_file
                            )))
                            .on_click(cx.listener(
                                |this: &mut Ide, ev: &gpui::ClickEvent, _win, cx| {
                                    if ev.click_count() >= 2 {
                                        this.show_code_window(cx);
                                        cx.notify();
                                    }
                                },
                            )),
                    ),
            )
    }

    fn toolbox_button(
        &self,
        label: &str,
        kind: ControlKind,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(SharedString::from(format!("tool-{}", label)))
            .debug_selector({
                let kind_name = kind.basic_name();
                move || format!("tool-{}", kind_name)
            })
            .h_8()
            .w_full()
            .mx_1()
            .my_0p5()
            .px_2()
            .flex()
            .items_center()
            .rounded_md()
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(BORDER))
            .hover(|s| s.bg(rgb(0xE8E8ED)))
            .cursor_pointer()
            .child(label.to_string())
            .on_click(cx.listener(move |this: &mut Ide, _ev, _win, cx| {
                this.add_control(kind);
                cx.notify();
            }))
    }

    fn render_toolbox(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_48()
            .min_w_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PANEL))
            .flex()
            .flex_col()
            .child(self.dock_title("Toolbox"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .content_start()
                    .p_1()
                    .child(self.toolbox_button("Aa Label", ControlKind::Label, cx))
                    .child(self.toolbox_button("ab TextBox", ControlKind::TextBox, cx))
                    .child(self.toolbox_button("OK CommandButton", ControlKind::CommandButton, cx))
                    .child(self.toolbox_button("☐ Frame", ControlKind::Frame, cx))
                    .child(self.toolbox_button("≡ ListBox", ControlKind::ListBox, cx))
                    .child(self.toolbox_button("⌄ ComboBox", ControlKind::ComboBox, cx))
                    .child(self.toolbox_button("▧ PictureBox", ControlKind::PictureBox, cx))
                    .child(self.toolbox_button("◷ Timer", ControlKind::Timer, cx)),
            )
    }

    /// A toolbar action uses the same typed GPUI action as its macOS menu
    /// item. Keeping one route prevents menu and in-window commands from
    /// drifting apart.
    fn action_button(
        &self,
        id: &'static str,
        label: &str,
        action: impl Fn(&mut Ide, &mut Context<Ide>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .px_3()
            .py_1()
            .mx_1()
            .rounded_md()
            .bg(rgb(TOOLBAR))
            .hover(|s| s.bg(rgb(0xE8E8ED)))
            .active(|s| s.bg(rgb(SEL_BG)))
            .cursor_pointer()
            .text_color(rgb(FG))
            .child(label.to_string())
            .on_click(cx.listener(move |this: &mut Ide, _ev, _window, cx| {
                action(this, cx);
                cx.notify();
            }))
    }

    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.session.is_some();
        div()
            .flex()
            .items_center()
            .h_11()
            .px_2()
            .bg(rgb(TOOLBAR))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(self.action_button(
                "toolbar-save-code",
                "Save Code",
                |this, _cx| this.save_code(),
                cx,
            ))
            .child(self.action_button(
                "toolbar-save-form",
                "Save Form",
                |this, _cx| this.save_form(),
                cx,
            ))
            .child(div().w_px().h_7().mx_1().bg(rgb(BORDER)))
            .child(self.action_button(
                "toolbar-view-code",
                "View Code",
                |this, cx| this.view_code(cx),
                cx,
            ))
            .child(self.action_button(
                "toolbar-view-object",
                "View Object",
                |this, cx| this.hide_code_window(cx),
                cx,
            ))
            .child(div().w_px().h_7().mx_1().bg(rgb(BORDER)))
            .child(if self.paused.is_some() {
                self.action_button(
                    "toolbar-run",
                    "▶ Continue",
                    |this, _cx| this.run_program(),
                    cx,
                )
            } else if running {
                self.action_button("toolbar-live", "● Live", |_this, _cx| {}, cx)
            } else {
                self.action_button("toolbar-run", "▶ Run", |this, _cx| this.run_program(), cx)
            })
            .child(self.action_button("toolbar-step", "Step", |this, _cx| this.step_program(), cx))
            .child(self.action_button(
                "toolbar-stop",
                "■ Stop",
                |this, cx| this.stop_program(cx),
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .ml_2()
                    .text_color(rgb(DIM))
                    .child(SharedString::from(self.status.clone())),
            )
    }
}

/// The code editor pane.
impl Ide {
    fn scrollbar_overlays(
        &self,
        pane: &'static str,
        handle: &ScrollHandle,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let viewport = handle.bounds().size;
        let max = handle.max_offset();
        let offset = handle.offset();
        let overlay = div().absolute().size_full().child(if max.height > px(0.0) {
            let track = (f32::from(viewport.height) - 20.0).max(24.0);
            let thumb = (track * f32::from(viewport.height)
                / (f32::from(viewport.height) + f32::from(max.height)))
            .clamp(24.0, track);
            let travel = track - thumb;
            let top = 2.0 + (-f32::from(offset.y) / f32::from(max.height)) * travel;
            let scroll = handle.clone();
            div()
                .debug_selector(|| format!("scrollbar-{}-vertical", pane))
                .absolute()
                .left(px((f32::from(viewport.width) - 10.0).max(0.0)))
                .top(px(2.0))
                .w(px(8.0))
                .h(px(track))
                .rounded_full()
                .bg(rgb(0xE5E5EA))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(top - 2.0))
                        .w(px(8.0))
                        .h(px(thumb))
                        .rounded_full()
                        .bg(rgb(0x8E8E93))
                        .hover(|s| s.bg(rgb(0x636366)))
                        .cursor_pointer()
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this: &mut Ide, ev: &MouseDownEvent, _win, cx| {
                                cx.stop_propagation();
                                this.scrollbar_drag = Some(ScrollbarDrag {
                                    pane,
                                    vertical: true,
                                    pointer_start: f32::from(ev.position.y),
                                    offset_start: f32::from(scroll.offset().y),
                                    max_offset: f32::from(scroll.max_offset().height),
                                    thumb_travel: travel,
                                });
                            }),
                        ),
                )
        } else {
            div()
        });

        overlay.child(if max.width > px(0.0) {
            let track = (f32::from(viewport.width) - 20.0).max(24.0);
            let thumb = (track * f32::from(viewport.width)
                / (f32::from(viewport.width) + f32::from(max.width)))
            .clamp(24.0, track);
            let travel = track - thumb;
            let left = 2.0 + (-f32::from(offset.x) / f32::from(max.width)) * travel;
            let scroll = handle.clone();
            div()
                .debug_selector(|| format!("scrollbar-{}-horizontal", pane))
                .absolute()
                .left(px(2.0))
                .top(px((f32::from(viewport.height) - 10.0).max(0.0)))
                .w(px(track))
                .h(px(8.0))
                .rounded_full()
                .bg(rgb(0xE5E5EA))
                .child(
                    div()
                        .absolute()
                        .left(px(left - 2.0))
                        .top_0()
                        .w(px(thumb))
                        .h(px(8.0))
                        .rounded_full()
                        .bg(rgb(0x8E8E93))
                        .hover(|s| s.bg(rgb(0x636366)))
                        .cursor_pointer()
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this: &mut Ide, ev: &MouseDownEvent, _win, cx| {
                                cx.stop_propagation();
                                this.scrollbar_drag = Some(ScrollbarDrag {
                                    pane,
                                    vertical: false,
                                    pointer_start: f32::from(ev.position.x),
                                    offset_start: f32::from(scroll.offset().x),
                                    max_offset: f32::from(scroll.max_offset().width),
                                    thumb_travel: travel,
                                });
                            }),
                        ),
                )
        } else {
            div()
        })
    }

    fn update_scrollbar_drag(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(drag) = &self.scrollbar_drag else {
            return;
        };
        if !ev.dragging() {
            return;
        }
        let pointer = if drag.vertical {
            f32::from(ev.position.y)
        } else {
            f32::from(ev.position.x)
        };
        let offset = drag.offset_start
            - (pointer - drag.pointer_start) * drag.max_offset / drag.thumb_travel.max(1.0);
        let handle = match drag.pane {
            "editor" => self.editor_scroll.clone(),
            "designer" => self.designer_scroll.clone(),
            _ => self.output_scroll.clone(),
        };
        let mut position = handle.offset();
        if drag.vertical {
            position.y = px(offset.clamp(-drag.max_offset, 0.0));
        } else {
            position.x = px(offset.clamp(-drag.max_offset, 0.0));
        }
        handle.set_offset(position);
        cx.notify();
    }

    fn render_editor(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let bps = self.breakpoints.lock().unwrap().clone();
        let paused_line = self.paused.as_ref().map(|p| p.line);
        // Retain the width of the longest source line so horizontal scrolling
        // can expose it instead of flex layout clipping or wrapping it.
        let content_width = self
            .lines
            .iter()
            .map(|line| 64.0 + line.chars().count() as f32 * 8.0)
            .fold(240.0_f32, f32::max);
        let mut list = div()
            .id("editor-scroll")
            .debug_selector(|| "editor-content".into())
            .flex()
            .flex_col()
            .flex_none()
            .w(px(content_width))
            .bg(rgb(WINDOW_BG))
            .font_family("Menlo");

        for (idx, line) in self.lines.clone().into_iter().enumerate() {
            let lineno = idx + 1;
            let has_bp = bps.contains(&lineno);
            let is_paused = paused_line == Some(lineno);
            let is_cursor = self.cursor.0 == idx;

            let mut gutter = div()
                .id((SharedString::from("gutter"), idx))
                .w(px(48.0))
                .flex_none()
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

            let mut code = div().flex().flex_none().pl_2().whitespace_nowrap();
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

            list = list.child(
                div()
                    .debug_selector(|| format!("editor-row-{}", idx))
                    .flex()
                    .flex_none()
                    .w(px(content_width))
                    .h_5()
                    .items_center()
                    .whitespace_nowrap()
                    .child(gutter)
                    .child(code),
            );
        }

        div()
            .id("editor")
            .debug_selector(|| "editor-viewport".into())
            .size_full()
            .overflow_scroll()
            .scrollbar_width(px(8.0))
            .track_scroll(&self.editor_scroll)
            .track_focus(&self.editor_focus)
            .on_key_down(cx.listener(|this: &mut Ide, ev: &KeyDownEvent, _win, cx| {
                this.editor_key(ev);
                this.code_object = None;
                cx.notify();
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut Ide, ev: &MouseDownEvent, win, cx| {
                    // click-to-type: focus the editor and put the caret
                    // under the pointer
                    win.focus(&this.editor_focus);
                    this.code_menu = None;
                    this.code_object = None;
                    this.place_cursor_at(ev.position, win);
                    cx.notify();
                }),
            )
            .child(list)
            .child(self.scrollbar_overlays("editor", &self.editor_scroll, cx))
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

        // Do not put an overlay over the form: controls must receive their
        // own mouse events.
        let mut form_area = div()
            .debug_selector(|| "designer-form".into())
            .relative()
            .w(px(form.width))
            .h(px(form.height))
            .bg(rgb(FORM_BG))
            .text_color(rgb(0x1A1A1A))
            .border_1()
            .border_color(rgb(if selected.is_none() {
                ACCENT
            } else {
                CTRL_BORDER
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut Ide, ev: &MouseDownEvent, win, cx| {
                    // background click selects the form; double-click opens
                    // Form_Load, as in VB
                    this.select(None);
                    win.focus(&this.designer_focus);
                    if ev.click_count >= 2 {
                        this.open_default_event(None, cx);
                    }
                    cx.notify();
                }),
            );

        // Invisible-at-runtime controls (Timers) appear in the designer as
        // small icons so they can be selected and double-clicked.
        let mut icon_slot = 0.0;
        for idx in oxbasic_ui::gpui_controls::z_order(&form) {
            let mut c = form.controls[idx].clone();
            let is_timer = c.kind == ControlKind::Timer;
            if !c.visible && !is_timer {
                continue;
            }
            if is_timer {
                // a timer with no position parks in the form's corner
                if c.rect.width <= 0.0 {
                    c.rect.left = form.width - 36.0 - icon_slot * 32.0;
                    c.rect.top = form.height - 36.0;
                    icon_slot += 1.0;
                }
                c.rect.left = c.rect.left.max(0.0);
                c.rect.top = c.rect.top.max(0.0);
                c.rect.width = 28.0;
                c.rect.height = 28.0;
            }
            let look = ControlLook::design(&c);
            // The designer wires only the control as a whole: the parts
            // inside it (a combo's arrow, a list's rows) are not live here.
            let mut hook = |part: Part,
                            el: gpui::Stateful<gpui::Div>,
                            cx: &mut Context<Ide>|
             -> gpui::Stateful<gpui::Div> {
                if part != Part::Body {
                    return el;
                }
                el.debug_selector(move || format!("designer-control-{}", idx))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this: &mut Ide, ev: &MouseDownEvent, win, cx| {
                            // A control owns its pointer-down event; otherwise
                            // it bubbles to the form and selects the form.
                            cx.stop_propagation();
                            this.select(Some(idx));
                            win.focus(&this.designer_focus);
                            if ev.click_count >= 2 {
                                // double-click: edit the control's default event
                                this.drag = None;
                                this.open_default_event(Some(idx), cx);
                            } else if !is_timer || this.form.controls[idx].rect.width > 0.0 {
                                this.drag = Some(DesignDrag::Move(
                                    idx,
                                    f32::from(ev.position.x),
                                    f32::from(ev.position.y),
                                ));
                            }
                            cx.notify();
                        }),
                    )
            };
            form_area = form_area.child(paint_control(&c, &look, &mut hook, cx));
            if selected == Some(idx) {
                // selection outline, drawn over the control (no handlers, so
                // clicks pass through to the control itself)
                form_area = form_area.child(
                    div()
                        .absolute()
                        .left(px(c.rect.left - 2.0))
                        .top(px(c.rect.top - 2.0))
                        .w(px(c.rect.width.max(4.0) + 4.0))
                        .h(px(c.rect.height.max(4.0) + 4.0))
                        .border_1()
                        .border_color(rgb(ACCENT))
                        .border_dashed(),
                );
                // ...and VB's eight resize handles on it. A Timer has none:
                // its designer icon is fixed-size.
                if !is_timer {
                    for h in ResizeHandle::ALL {
                        let (hx, hy) = h.center(&c.rect);
                        let sel_name = format!("designer-handle-{}-{}-{}", idx, h.dx, h.dy);
                        form_area = form_area.child(
                            div()
                                .absolute()
                                .debug_selector(move || sel_name.clone())
                                .left(px(hx - RESIZE_HANDLE / 2.0))
                                .top(px(hy - RESIZE_HANDLE / 2.0))
                                .w(px(RESIZE_HANDLE))
                                .h(px(RESIZE_HANDLE))
                                .bg(rgb(PANEL))
                                .border_1()
                                .border_color(rgb(ACCENT))
                                .rounded_sm()
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(
                                        move |this: &mut Ide, ev: &MouseDownEvent, win, cx| {
                                            // the handle owns its pointer-down
                                            // event; a drag from it resizes
                                            cx.stop_propagation();
                                            this.select(Some(idx));
                                            win.focus(&this.designer_focus);
                                            this.drag = Some(DesignDrag::Resize(
                                                idx,
                                                h,
                                                f32::from(ev.position.x),
                                                f32::from(ev.position.y),
                                            ));
                                            cx.notify();
                                        },
                                    ),
                                ),
                        );
                    }
                }
            }
        }

        // the form itself, when it (not a control) is selected, gets VB's
        // sizing handles on its bottom, right and bottom-right edges
        if selected.is_none() {
            let form_rect = Rect {
                left: 0.0,
                top: 0.0,
                width: form.width,
                height: form.height,
            };
            for h in [
                ResizeHandle { dx: 1, dy: 0 },
                ResizeHandle { dx: 0, dy: 1 },
                ResizeHandle { dx: 1, dy: 1 },
            ] {
                let (hx, hy) = h.center(&form_rect);
                let sel_name = format!("designer-handle-form-{}-{}", h.dx, h.dy);
                form_area = form_area.child(
                    div()
                        .absolute()
                        .debug_selector(move || sel_name.clone())
                        .left(px(hx - RESIZE_HANDLE / 2.0))
                        .top(px(hy - RESIZE_HANDLE / 2.0))
                        .w(px(RESIZE_HANDLE))
                        .h(px(RESIZE_HANDLE))
                        .bg(rgb(PANEL))
                        .border_1()
                        .border_color(rgb(ACCENT))
                        .rounded_sm()
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this: &mut Ide, ev: &MouseDownEvent, win, cx| {
                                cx.stop_propagation();
                                this.select(None);
                                win.focus(&this.designer_focus);
                                this.drag = Some(DesignDrag::Resize(
                                    FORM_IDX,
                                    h,
                                    f32::from(ev.position.x),
                                    f32::from(ev.position.y),
                                ));
                                cx.notify();
                            }),
                        ),
                );
            }
        }

        // the designer region tracks mouse movement for dragging
        div()
            .id("designer")
            .flex_1()
            .min_h_0()
            .overflow_scroll()
            .scrollbar_width(px(8.0))
            .track_scroll(&self.designer_scroll)
            .bg(rgb(PANEL))
            .flex()
            .items_start()
            .justify_start()
            .p_4()
            .child(form_area)
            .child(self.scrollbar_overlays("designer", &self.designer_scroll, cx))
    }
    /// Track a designer drag — moving or resizing a control, or resizing the
    /// form — while the button is held. On the workspace root (not just the
    /// designer pane), so a drag keeps working when the pointer crosses into
    /// another pane.
    fn update_designer_drag(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !ev.dragging() {
            self.drag = None;
            return;
        }
        let x = f32::from(ev.position.x);
        let y = f32::from(ev.position.y);
        let drag = match self.drag {
            Some(DesignDrag::Move(idx, last_x, last_y)) => {
                let c = &mut self.form.controls[idx];
                c.rect.left = (c.rect.left + x - last_x).max(0.0).round();
                c.rect.top = (c.rect.top + y - last_y).max(0.0).round();
                Some(DesignDrag::Move(idx, x, y))
            }
            Some(DesignDrag::Resize(idx, h, last_x, last_y)) => {
                let (dx, dy) = (h.dx as f32, h.dy as f32);
                if idx == FORM_IDX {
                    // the form's own handles grow its bottom and right edges
                    if dx > 0.0 {
                        self.form.width = (self.form.width + x - last_x).max(MIN_FORM).round();
                    }
                    if dy > 0.0 {
                        self.form.height = (self.form.height + y - last_y).max(MIN_FORM).round();
                    }
                    Some(DesignDrag::Resize(idx, h, x, y))
                } else {
                    let c = &mut self.form.controls[idx];
                    // the right/bottom handles grow the size; the left/top
                    // ones move that edge, keeping the opposite one (and the
                    // form's border) fixed
                    if dx > 0.0 {
                        c.rect.width = (c.rect.width + x - last_x).max(MIN_CONTROL);
                    } else if dx < 0.0 {
                        let mut d = (x - last_x).min(c.rect.width - MIN_CONTROL);
                        if c.rect.left + d < 0.0 {
                            d = -c.rect.left;
                        }
                        c.rect.left += d;
                        c.rect.width -= d;
                    }
                    if dy > 0.0 {
                        c.rect.height = (c.rect.height + y - last_y).max(MIN_CONTROL);
                    } else if dy < 0.0 {
                        let mut d = (y - last_y).min(c.rect.height - MIN_CONTROL);
                        if c.rect.top + d < 0.0 {
                            d = -c.rect.top;
                        }
                        c.rect.top += d;
                        c.rect.height -= d;
                    }
                    c.rect.left = c.rect.left.round();
                    c.rect.top = c.rect.top.round();
                    c.rect.width = c.rect.width.round();
                    c.rect.height = c.rect.height.round();
                    Some(DesignDrag::Resize(idx, h, x, y))
                }
            }
            None => None,
        };
        let changed = drag.is_some();
        self.drag = drag;
        if changed {
            cx.notify();
        }
    }
}

/// What a pointer drag over the designer is doing to a control.
#[derive(Clone, Copy)]
enum DesignDrag {
    /// Moving the control: its index and the last pointer position.
    Move(usize, f32, f32),
    /// Resizing it with a handle: index, handle, last pointer position.
    Resize(usize, ResizeHandle, f32, f32),
}

/// One of the eight handles of a selected control's outline: offsets (-1, 0
/// or 1) from the control's centre along each axis. `(0, 0)` — the centre —
/// is the control's body, which drags to move it.
#[derive(Clone, Copy)]
struct ResizeHandle {
    dx: i8,
    dy: i8,
}

impl ResizeHandle {
    /// VB's eight handles: the four corners and the side midpoints.
    const ALL: [ResizeHandle; 8] = [
        ResizeHandle { dx: -1, dy: -1 },
        ResizeHandle { dx: 0, dy: -1 },
        ResizeHandle { dx: 1, dy: -1 },
        ResizeHandle { dx: -1, dy: 0 },
        ResizeHandle { dx: 1, dy: 0 },
        ResizeHandle { dx: -1, dy: 1 },
        ResizeHandle { dx: 0, dy: 1 },
        ResizeHandle { dx: 1, dy: 1 },
    ];

    /// Where the handle's square is centred on the control's outline.
    fn center(&self, r: &Rect) -> (f32, f32) {
        let x = r.left + r.width * (self.dx as f32 + 1.0) / 2.0;
        let y = r.top + r.height * (self.dy as f32 + 1.0) / 2.0;
        (x, y)
    }
}

/// A side of the square a resize handle is drawn as, in px.
const RESIZE_HANDLE: f32 = 6.5;
/// The smallest width/height dragging a handle can leave a control with.
const MIN_CONTROL: f32 = 8.0;
/// The smallest width/height the form's own handles can leave it with.
const MIN_FORM: f32 = 60.0;

/// A live program's form, running in its own OS window. In classic VB the
/// form defines the application's window: its controls look and behave as
/// in VB6 (text boxes take typing, list and combo boxes select, a combo's
/// ▾ drops its list) and each user action fires the matching
/// `<ControlName>_<Event>` sub in the running program via the debug
/// session.
struct FormRunner {
    form: FormSpec,
    session: DebugSession,
    /// Run-time state (text, selection, open list) per control.
    state: Vec<ControlState>,
    /// Keyboard focus handles for the controls that take typing.
    focus: HashMap<usize, FocusHandle>,
}

impl FormRunner {
    fn new(form: FormSpec, session: DebugSession, cx: &mut Context<Self>) -> FormRunner {
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
        FormRunner {
            form,
            session,
            state,
            focus,
        }
    }

    /// Fire `<control>_<event>` in the running program (a missing handler
    /// is a no-op there).
    fn fire(&self, control: &str, event: &str) {
        self.session
            .send(DebugCommand::Call(format!("{}_{}", control, event)));
    }

    fn close_lists(&mut self, keep: Option<usize>) {
        for (i, s) in self.state.iter_mut().enumerate() {
            if Some(i) != keep {
                s.dropped = false;
            }
        }
    }

    /// Handle a pointer-down on `part` of control `idx`.
    fn press(&mut self, idx: usize, part: Part) {
        let spec = self.form.controls[idx].clone();
        match part {
            Part::Arrow | Part::Field if spec.kind == ControlKind::ComboBox => {
                // The ▾ toggles the list; so does the field of a Dropdown
                // List (Style 2), which can't be typed into.
                if part == Part::Arrow || spec.style == 2 {
                    let open = !self.state[idx].dropped;
                    self.close_lists(Some(idx));
                    self.state[idx].dropped = open;
                    if open {
                        self.fire(&spec.name, "DropDown");
                    }
                } else {
                    self.close_lists(None);
                }
            }
            Part::ListItem(i) | Part::DropItem(i) => {
                let was = self.state[idx].text.clone();
                self.state[idx].pick(&spec, i);
                self.close_lists(None);
                if spec.kind == ControlKind::ComboBox && self.state[idx].text != was {
                    self.fire(&spec.name, "Change");
                }
                self.fire(&spec.name, "Click");
            }
            _ => {
                self.close_lists(None);
                self.fire(&spec.name, "Click");
            }
        }
    }

    /// Handle a keystroke in control `idx`. Returns whether it changed.
    fn key(&mut self, idx: usize, ks: &gpui::Keystroke) -> bool {
        let spec = &self.form.controls[idx];
        let editable = match spec.kind {
            ControlKind::TextBox => true,
            ControlKind::ComboBox => spec.style != 2,
            _ => false,
        };
        if !editable || !edit_text(&mut self.state[idx].text, ks) {
            return false;
        }
        self.state[idx].list_index = None;
        let name = spec.name.clone();
        self.fire(&name, "Change");
        true
    }
}

impl Render for FormRunner {
    fn render(&mut self, win: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut area = div()
            .id("form-runner")
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(FORM_BACK))
            .text_color(rgb(FG))
            // a click on the form itself closes any open combo list
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut FormRunner, _ev: &MouseDownEvent, _win, cx| {
                    this.close_lists(None);
                    cx.notify();
                }),
            );
        let form = self.form.clone();
        for idx in oxbasic_ui::gpui_controls::z_order(&form) {
            let c = &form.controls[idx];
            if !c.visible || c.kind == ControlKind::Timer {
                continue;
            }
            let focus = self.focus.get(&idx).cloned();
            let look = ControlLook {
                design: false,
                text: self.state[idx].text.clone(),
                list_index: self.state[idx].list_index,
                dropped: self.state[idx].dropped,
                focused: focus.as_ref().is_some_and(|f| f.contains_focused(win, cx)),
            };
            let mut hook = |part: Part,
                            el: gpui::Stateful<gpui::Div>,
                            cx: &mut Context<FormRunner>|
             -> gpui::Stateful<gpui::Div> {
                let name = c.name.clone();
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
                let focus = focus.clone();
                let el = el.on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(
                        move |this: &mut FormRunner, _ev: &MouseDownEvent, win, cx| {
                            cx.stop_propagation();
                            if let Some(f) = &focus {
                                win.focus(f);
                            }
                            this.press(idx, part);
                            cx.notify();
                        },
                    ),
                );
                if part == Part::Body {
                    el.on_key_down(cx.listener(
                        move |this: &mut FormRunner, ev: &KeyDownEvent, _win, cx| {
                            if this.key(idx, &ev.keystroke) {
                                cx.notify();
                            }
                        },
                    ))
                } else {
                    el
                }
            };
            area = area.child(paint_control(c, &look, &mut hook, cx));
        }
        area
    }
}

/// The property inspector: the selected control's properties, or the
/// form's when nothing is selected. Text properties apply as you type;
/// `Name` is validated and applied on Enter / leaving the field, because a
/// rename also rewrites the control's event handlers in the code.
impl Ide {
    /// The current value of inspector property `key` for `idx` (a control
    /// index, or [`FORM_IDX`]).
    fn prop_value(&self, idx: usize, key: &str) -> String {
        if idx == FORM_IDX {
            let f = &self.form;
            return match key {
                "name" => f.name.clone(),
                "caption" => f.caption.clone(),
                "width" => format!("{}", f.width),
                "height" => format!("{}", f.height),
                _ => String::new(),
            };
        }
        let Some(c) = self.form.controls.get(idx) else {
            return String::new();
        };
        match key {
            "name" => c.name.clone(),
            "caption" => c.caption.clone(),
            "text" => c.text.clone().unwrap_or_default(),
            "items" => c.items.join(", "),
            "left" => format!("{}", c.rect.left),
            "top" => format!("{}", c.rect.top),
            "width" => format!("{}", c.rect.width),
            "height" => format!("{}", c.rect.height),
            "interval" => format!("{}", c.interval_ms),
            "style" => format!("{}", c.style),
            _ => String::new(),
        }
    }

    /// Apply an inspector edit as it is typed. `Name` is not applied here
    /// (see [`Ide::commit_rename`]); numbers apply once they parse.
    fn apply_prop(&mut self, idx: usize, key: &str, val: &str) {
        let num = val
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.0);
        if idx == FORM_IDX {
            match key {
                "caption" => self.form.caption = val.to_string(),
                "width" => self.form.width = num.unwrap_or(self.form.width),
                "height" => self.form.height = num.unwrap_or(self.form.height),
                _ => {}
            }
            return;
        }
        let Some(c) = self.form.controls.get_mut(idx) else {
            return;
        };
        match key {
            "caption" => c.caption = val.to_string(),
            "text" => c.text = Some(val.to_string()),
            "items" => {
                c.items = val
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "left" => c.rect.left = num.unwrap_or(c.rect.left),
            "top" => c.rect.top = num.unwrap_or(c.rect.top),
            "width" => c.rect.width = num.unwrap_or(c.rect.width),
            "height" => c.rect.height = num.unwrap_or(c.rect.height),
            "interval" => c.interval_ms = val.trim().parse().unwrap_or(c.interval_ms),
            // Style 0 Dropdown Combo, 1 Simple Combo, 2 Dropdown List; a
            // Simple Combo needs room for its list, so it grows to show it
            "style" => {
                if let Ok(s @ 0..=2) = val.trim().parse::<u8>() {
                    c.style = s;
                    if s == 1 && c.rect.height < 80.0 {
                        c.rect.height = 80.0;
                    }
                }
            }
            _ => {}
        }
    }

    /// Why `new` can't name control `idx` (or the form), if it can't:
    /// it must be a BASIC identifier, not a keyword, and unique on the form.
    fn rename_error(&self, idx: usize, new: &str) -> Option<String> {
        let mut chars = new.chars();
        match chars.next() {
            None => return Some("a name is required".into()),
            Some(c) if !c.is_ascii_alphabetic() => {
                return Some("a name must start with a letter".into())
            }
            _ => {}
        }
        if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Some("use only letters, digits and _".into());
        }
        if new.len() > 40 {
            return Some("a name can be at most 40 characters".into());
        }
        if highlight::is_keyword(new) {
            return Some(format!("\"{}\" is a reserved word", new));
        }
        if idx != FORM_IDX && new.eq_ignore_ascii_case("Form") {
            return Some("\"Form\" names the form's own events".into());
        }
        let taken = self
            .form
            .controls
            .iter()
            .enumerate()
            .any(|(i, c)| i != idx && c.name.eq_ignore_ascii_case(new))
            || (idx != FORM_IDX && self.form.name.eq_ignore_ascii_case(new));
        taken.then(|| format!("\"{}\" is already used on this form", new))
    }

    /// Validate and apply a pending `Name` edit. A control's event
    /// handlers and references in the code are renamed along with it
    /// (`Command1_Click` -> `OkButton_Click`), as VB's designer does.
    /// Returns false (keeping the edit pending) when the name is invalid.
    fn commit_rename(&mut self, idx: usize) -> bool {
        let Some(new) = self
            .prop_bufs
            .get(&(idx, "name"))
            .map(|s| s.trim().to_string())
        else {
            return true;
        };
        let old = self.prop_value(idx, "name");
        if new == old {
            self.rename_error = None;
            return true;
        }
        if let Some(err) = self.rename_error(idx, &new) {
            self.status = format!("cannot rename {}: {}", old, err);
            self.rename_error = Some((idx, err));
            return false;
        }
        self.rename_error = None;
        self.prop_bufs.insert((idx, "name"), new.clone());
        if idx == FORM_IDX {
            // Form events are `Form_<Event>` whatever the form's name.
            self.form.name = new.clone();
            self.status = format!("renamed form {} to {}", old, new);
            return true;
        }
        let kind = self.form.controls[idx].kind;
        self.form.controls[idx].name = new.clone();
        let events = events_for_control(kind.basic_name());
        let n = code_model::rename_control(&mut self.lines, &old, &new, &events);
        self.cursor.1 = self.cursor.1.min(self.line_len(self.cursor.0));
        self.status = if n == 0 {
            format!("renamed {} to {}", old, new)
        } else {
            format!(
                "renamed {} to {} ({} reference(s) updated in code)",
                old, new, n
            )
        };
        true
    }

    /// Drop a pending `Name` edit, restoring the current name.
    fn cancel_rename(&mut self, idx: usize) {
        let current = self.prop_value(idx, "name");
        self.prop_bufs.insert((idx, "name"), current);
        self.rename_error = None;
    }

    /// Commit whatever `Name` edit is pending (another control was
    /// selected, or focus moved away); an invalid one is dropped.
    fn commit_pending_rename(&mut self) {
        let pending: Vec<usize> = self
            .prop_bufs
            .iter()
            .filter(|((i, k), v)| *k == "name" && v.trim() != self.prop_value(*i, "name"))
            .map(|((i, _), _)| *i)
            .collect();
        for idx in pending {
            if !self.commit_rename(idx) {
                self.cancel_rename(idx);
            }
        }
    }

    /// Select control `idx` (or the form, for `None`), committing any
    /// pending rename of the previous selection first.
    fn select(&mut self, idx: Option<usize>) {
        if self.selected != idx {
            self.commit_pending_rename();
        }
        self.selected = idx;
    }

    /// One keystroke into inspector field `key` of `idx`.
    fn prop_key(&mut self, idx: usize, key: &'static str, ev: &KeyDownEvent, win: &mut Window) {
        let ks = &ev.keystroke;
        if ks.modifiers.platform || ks.modifiers.control {
            return;
        }
        match ks.key.as_str() {
            "enter" | "tab" => {
                if key != "name" || self.commit_rename(idx) {
                    win.blur();
                }
                return;
            }
            "escape" => {
                if key == "name" {
                    self.cancel_rename(idx);
                } else {
                    let v = self.prop_value(idx, key);
                    self.prop_bufs.insert((idx, key), v);
                }
                win.blur();
                return;
            }
            _ => {}
        }
        let buf = self.prop_bufs.entry((idx, key)).or_default();
        if ks.key == "backspace" {
            buf.pop();
        } else if let Some(s) = typed_text(ev) {
            buf.push_str(&s);
        } else {
            return;
        }
        let val = buf.clone();
        if key == "name" {
            // validate as you type; the rename itself waits for Enter
            let current = self.prop_value(idx, "name");
            self.rename_error = if val.trim() == current {
                None
            } else {
                self.rename_error(idx, val.trim()).map(|e| (idx, e))
            };
        } else {
            self.apply_prop(idx, key, &val);
        }
    }

    fn prop_row(
        &mut self,
        idx: usize,
        key: &'static str,
        label: &str,
        win: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Each field owns its focus handle, so keystrokes always edit the
        // row that was clicked.
        let focus = self
            .prop_focus
            .entry((idx, key))
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let focused = focus.is_focused(win);
        let error = self
            .rename_error
            .as_ref()
            .filter(|(i, _)| key == "name" && *i == idx)
            .map(|(_, e)| e.clone());
        // A field that isn't being edited always shows the live value (so
        // dragging a control updates Left/Top); a rejected name stays put
        // until it is fixed or cancelled.
        if !focused && error.is_none() {
            let v = self.prop_value(idx, key);
            self.prop_bufs.insert((idx, key), v);
        }
        let text = self.prop_bufs.get(&(idx, key)).cloned().unwrap_or_default();
        let border = match (&error, focused) {
            (Some(_), _) => ERROR,
            (None, true) => ACCENT,
            (None, false) => BORDER,
        };
        let focus_click = focus.clone();
        let mut field = div()
            .id(SharedString::from(format!("prop-{}-{}", key, idx)))
            .debug_selector(move || format!("prop-{}", key))
            .flex_1()
            .min_w_0()
            .h_6()
            .px_1()
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(border))
            .rounded_sm()
            .flex()
            .items_center()
            .overflow_hidden()
            .whitespace_nowrap()
            .track_focus(&focus)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |_: &mut Ide, _ev: &MouseDownEvent, win, cx| {
                    // click-to-edit: give this row keyboard focus
                    cx.stop_propagation();
                    win.focus(&focus_click);
                }),
            )
            .on_key_down(
                cx.listener(move |this: &mut Ide, ev: &KeyDownEvent, win, cx| {
                    this.prop_key(idx, key, ev, win);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(SharedString::from(text));
        if key == "name" {
            field = field.font_weight(gpui::FontWeight::SEMIBOLD);
        }
        if focused {
            field = field.child(div().flex_none().w(px(1.5)).h(px(14.0)).bg(rgb(ACCENT)));
        }
        let row = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w_16()
                    .flex_none()
                    .text_color(rgb(DIM))
                    .child(label.to_string()),
            )
            .child(field);
        match error {
            Some(err) => div()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(row)
                .child(
                    div()
                        .debug_selector(|| "rename-error".into())
                        .pl_20()
                        .text_xs()
                        .text_color(rgb(ERROR))
                        .child(SharedString::from(err)),
                )
                .into_any_element(),
            None => row.into_any_element(),
        }
    }
}

/// The text a keystroke types, if any (see `oxbasic_ui::gpui_controls`).
fn typed_text(ev: &KeyDownEvent) -> Option<String> {
    ks_text(&ev.keystroke)
}

/// Inspector panel and output/watch panes.
impl Ide {
    /// The properties the inspector lists for control `idx` (or the form),
    /// as `(key, label)`: only those the control has in VB6 — Caption for
    /// buttons, labels and frames; Text for text boxes and combos; List for
    /// list and combo boxes; Style for combos; Interval for timers.
    fn inspector_rows(&self, idx: usize) -> Vec<(&'static str, &'static str)> {
        let mut rows = vec![("name", "(Name)")];
        if idx == FORM_IDX {
            rows.extend([
                ("caption", "Caption"),
                ("width", "Width"),
                ("height", "Height"),
            ]);
            return rows;
        }
        let k = self.form.controls[idx].kind;
        if k.has_caption() {
            rows.push(("caption", "Caption"));
        }
        if k.has_text() {
            rows.push(("text", "Text"));
        }
        if matches!(k, ControlKind::ListBox | ControlKind::ComboBox) {
            rows.push(("items", "List"));
        }
        if k == ControlKind::ComboBox {
            rows.push(("style", "Style"));
        }
        if k == ControlKind::Timer {
            rows.push(("interval", "Interval"));
        } else {
            rows.extend([
                ("left", "Left"),
                ("top", "Top"),
                ("width", "Width"),
                ("height", "Height"),
            ]);
        }
        rows
    }

    fn render_inspector(&mut self, win: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The selected control, or the form itself (as VB's Properties
        // window shows when the form background is selected).
        let (idx, title, kind) = match self.selected.and_then(|i| self.form.controls.get(i)) {
            Some(c) => (self.selected.unwrap(), c.name.clone(), c.kind.basic_name()),
            None => (FORM_IDX, self.form.name.clone(), "Form"),
        };
        // Only the properties the control has in VB6.
        let rows = self.inspector_rows(idx);
        let mut list = div().p_2().flex().flex_col().gap_1();
        for (key, label) in rows {
            list = list.child(self.prop_row(idx, key, label, win, cx));
        }
        let handler_target = if idx == FORM_IDX { None } else { Some(idx) };
        div()
            .flex_1()
            .min_h_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(WINDOW_BG))
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.dock_title("Properties"))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(rgb(PANEL))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .debug_selector(|| "inspector-title".into())
                            .flex_1()
                            .min_w_0()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(SharedString::from(title)),
                    )
                    .child(div().text_color(rgb(DIM)).child(kind)),
            )
            .child(list)
            .child(
                div()
                    .px_3()
                    .pt_1()
                    .text_xs()
                    .text_color(rgb(DIM))
                    .child("Enter applies a new name, Esc reverts it."),
            )
            .child(
                div().px_2().pt_2().child(
                    self.action_button(
                        "inspector-view-code",
                        "Edit Event Code…",
                        move |this, cx| this.open_default_event(handler_target, cx),
                        cx,
                    )
                    .border_1()
                    .border_color(rgb(BORDER)),
                ),
            )
    }

    fn render_output(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let output_width = self
            .output
            .lines()
            .map(|line| line.chars().count() as f32 * 8.0)
            .fold(160.0_f32, f32::max);
        let mut panel = div()
            .h_40()
            .mx_2()
            .mb_2()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(WINDOW_BG))
            .flex()
            .overflow_hidden();
        // VB's Immediate window receives Print, MsgBox, and debugger output.
        panel = panel.child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(self.dock_title("Immediate"))
                .child(
                    div()
                        .id("output-scroll")
                        .flex_1()
                        .min_h_0()
                        .p_2()
                        .overflow_scroll()
                        .scrollbar_width(px(8.0))
                        .track_scroll(&self.output_scroll)
                        .font_family("Menlo")
                        .whitespace_nowrap()
                        .child(
                            div()
                                .flex_none()
                                .w(px(output_width))
                                .whitespace_nowrap()
                                .child(SharedString::from(self.output.clone())),
                        )
                        .child(self.scrollbar_overlays("output", &self.output_scroll, cx)),
                ),
        );
        // Show debugger state beside output only while paused.
        if let Some(p) = &self.paused {
            let mut watch = div()
                .w_2_5()
                .flex()
                .flex_col()
                .border_l_1()
                .border_color(rgb(BORDER))
                .child(self.dock_title("Variables"))
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .text_color(rgb(ACCENT))
                        .child(SharedString::from(format!(
                            "Line {} · {}",
                            p.line,
                            if p.proc.is_empty() {
                                "(module)"
                            } else {
                                &p.proc
                            }
                        ))),
                );
            for (name, value) in p.locals.iter().chain(p.globals.iter()) {
                watch = watch.child(
                    div()
                        .flex()
                        .gap_2()
                        .px_2()
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
            panel.child(watch)
        } else {
            panel
        }
    }
}
