//! The OxidizedBASIC IDE (Phase 3): a GPUI application with
//!
//! - a syntax-highlighted code editor with a breakpoint gutter,
//! - a visual form designer (drag-to-move, add controls, save `.oxform`),
//! - a property inspector for the selected control,
//! - an output / immediate pane,
//! - debug controls (run, step-over, stop) with a variable watch pane.

pub mod about_window;
pub mod choose_file;
pub mod code_model;

pub mod code_window;
pub mod debug_session;
pub mod editor;
pub mod editor_input;
pub mod find;
pub mod find_bar;
pub mod find_bar_view;
#[cfg(test)]
mod find_tests;
pub mod highlight;
pub mod launch;
pub mod live_forms;
pub mod live_input;
#[cfg(test)]
mod menu_editor_tests;
pub mod menu_editor_window;
pub mod package;
pub mod package_command;
pub mod play;
pub mod program;
pub mod project_commands;
pub mod project_program;
pub mod recent;
pub mod recent_menu;
pub mod recent_store;
#[cfg(test)]
mod recent_tests;
#[cfg(test)]
mod scroll_drag_tests;
#[cfg(test)]
pub mod showcase;
#[cfg(test)]
mod showcase_control_tests;
#[cfg(test)]
mod showcase_flow_tests;
#[cfg(test)]
mod showcase_form_tests;
#[cfg(test)]
mod fileio_project_tests;
#[cfg(test)]
mod showcase_menu_tests;
#[cfg(test)]
mod showcase_run;
pub mod splitters;
pub mod text_field;
pub mod toolbar;
pub mod workspace;

use debug_session::{DebugCommand, DebugEvent, DebugSession};
#[cfg(test)]
use gpui::point;
use oxbasic_runtime::PausedInfo;
use oxbasic_ui::gpui_controls::{
    paint_control, typed_text as ks_text, ControlLook, ControlState, Part, FORM_BACK,
};
use oxbasic_ui::host::{set_value, FormSnapshot, UserEdit};
use oxbasic_ui::{
    default_event, event_params, event_signatures, events_for_control, ControlKind, ControlSpec,
    FormSpec, Rect,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use toolbar::file_name;
use workspace::DocKind;

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
/// The underline of a syntax error in the code.
const C_ERROR: u32 = 0xD32F2F;
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
/// The index the project's package settings use in the property buffers. No
/// control or form has it, so the project's fields share the editing code of
/// the other inspector fields without colliding with them.
const PROJECT_IDX: usize = usize::MAX - 1;
/// The code window's object list entry for module-level code.
const GENERAL: &str = "(General)";
/// The code window's procedure list entry for the module-level code.
const DECLARATIONS: &str = "(Declarations)";
/// Width of the code window's object / procedure select boxes.
const SELECT_W: f32 = 240.0;
/// Height of one editor row (`h_5`).
const ROW_H: f32 = 20.0;
/// The Properties window's labels: a size smaller than the values beside
/// them, in a column wide enough for the longest name on one line.
const PROP_LABEL_SIZE: f32 = 11.0;
const PROP_LABEL_W: f32 = 84.0;
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
        AboutIde,
        SaveCode,
        SaveForm,
        NewProject,
        OpenProject,
        OpenRecent1,
        OpenRecent2,
        OpenRecent3,
        OpenRecent4,
        OpenRecent5,
        ClearRecent,
        SaveProject,
        SaveProjectAs,
        PackageProject,
        CloseProject,
        AddForm,
        SetStartForm,
        AddModule,
        RemoveDocument,
        RunProgram,
        StepProgram,
        StepIntoProgram,
        StopProgram,
        ViewCode,
        ViewObject,
        EditUndo,
        EditRedo,
        EditCut,
        EditCopy,
        EditPaste,
        EditSelectAll,
        EditFind,
        EditReplace,
        EditFindNext,
        EditFindPrevious
    ]
);

/// Register the IDE's keyboard shortcuts. The app calls it at start-up; the
/// tests call it too, so they use the same shortcuts a user does.
pub(super) fn bind_shortcuts(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-q", Quit, None),
        gpui::KeyBinding::new("cmd-n", NewProject, None),
        gpui::KeyBinding::new("cmd-o", OpenProject, None),
        gpui::KeyBinding::new("cmd-s", SaveProject, None),
        gpui::KeyBinding::new("cmd-shift-s", SaveProjectAs, None),
        gpui::KeyBinding::new("cmd-r", RunProgram, None),
        gpui::KeyBinding::new("cmd-shift-r", StopProgram, None),
        gpui::KeyBinding::new("f7", ViewCode, None),
        gpui::KeyBinding::new("f8", StepIntoProgram, None),
        gpui::KeyBinding::new("shift-f7", ViewObject, None),
        gpui::KeyBinding::new("cmd-z", EditUndo, None),
        gpui::KeyBinding::new("cmd-shift-z", EditRedo, None),
        gpui::KeyBinding::new("cmd-x", EditCut, None),
        gpui::KeyBinding::new("cmd-c", EditCopy, None),
        gpui::KeyBinding::new("cmd-v", EditPaste, None),
        gpui::KeyBinding::new("cmd-a", EditSelectAll, None),
        gpui::KeyBinding::new("cmd-f", EditFind, None),
        gpui::KeyBinding::new("cmd-alt-f", EditReplace, None),
        gpui::KeyBinding::new("cmd-g", EditFindNext, None),
        gpui::KeyBinding::new("cmd-shift-g", EditFindPrevious, None),
    ]);
}

/// The File menu. Its Open Recent submenu lists `recent`'s projects, each
/// with its name and path; the entries and their actions are numbered, so
/// the menu is built afresh whenever the list changes.
fn file_menu(recent: &recent::RecentList) -> Vec<gpui::MenuItem> {
    let recent_items: Vec<gpui::MenuItem> = if recent.items().is_empty() {
        vec![gpui::MenuItem::action("No Recent Projects", gpui::NoAction)]
    } else {
        let mut v: Vec<gpui::MenuItem> = recent
            .items()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                // the label shows the name and where it is; a file that has
                // gone is marked, and opening it says so in the status line
                let mut label = format!("{} — {}", p.name, p.path.display());
                if !p.path.exists() {
                    label.push_str("  (missing)");
                }
                recent_action(i, label)
            })
            .collect();
        v.push(gpui::MenuItem::separator());
        v.push(gpui::MenuItem::action("Clear Menu", ClearRecent));
        v
    };
    vec![
        gpui::MenuItem::action("New Project", NewProject),
        gpui::MenuItem::action("Open Project…", OpenProject),
        gpui::MenuItem::Submenu(gpui::Menu {
            name: "Open Recent".into(),
            items: recent_items,
        }),
        gpui::MenuItem::separator(),
        gpui::MenuItem::action("Save Project", SaveProject),
        gpui::MenuItem::action("Save Project As…", SaveProjectAs),
        gpui::MenuItem::action("Package Project…", PackageProject),
        gpui::MenuItem::action("Save Code", SaveCode),
        gpui::MenuItem::action("Save Form", SaveForm),
        gpui::MenuItem::separator(),
        gpui::MenuItem::action("Add Form", AddForm),
        gpui::MenuItem::action("Set as Start Form", SetStartForm),
        gpui::MenuItem::action("Add Module", AddModule),
        gpui::MenuItem::action("Remove Form or Module", RemoveDocument),
        gpui::MenuItem::separator(),
        gpui::MenuItem::action("Close Project", CloseProject),
    ]
}

/// The menu item for recent entry `i` (0 is the most recent).
fn recent_action(i: usize, label: String) -> gpui::MenuItem {
    match i {
        0 => gpui::MenuItem::action(label, OpenRecent1),
        1 => gpui::MenuItem::action(label, OpenRecent2),
        2 => gpui::MenuItem::action(label, OpenRecent3),
        3 => gpui::MenuItem::action(label, OpenRecent4),
        _ => gpui::MenuItem::action(label, OpenRecent5),
    }
}

/// Launch the IDE, optionally loading a `.bas` source file and an
/// `.oxform` layout file.
pub fn run_ide(bas_path: Option<&str>, oxform_path: Option<&str>) {
    let bas_path = bas_path.map(|s| s.to_string());
    let oxform_path = oxform_path.map(|s| s.to_string());
    Application::new().run(move |cx: &mut App| {
        bind_shortcuts(cx);
        // the File menu lists the projects kept from earlier runs
        let recent_at_start = recent_store::load(&recent_store::default_file());
        cx.set_menus(recent_menu::file_menu_bar(&recent_at_start));
        let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(SharedString::from("OxidizedBASIC IDE")),
                    // the toolbar is the title bar (toolbar.rs)
                    appears_transparent: true,
                    traffic_light_position: Some(toolbar::traffic_lights()),
                }),
                ..Default::default()
            },
            |win, cx| {
                let bas = bas_path.clone();
                let oxform = oxform_path.clone();
                let entity = cx.new(|cx| Ide::new(bas.as_deref(), oxform.as_deref(), cx));
                win.focus(&entity.read(cx).designer_focus);
                // the window's close box asks before losing unsaved work
                let weak = entity.downgrade();
                win.on_window_should_close(cx, move |win, cx| {
                    let Some(ide) = weak.upgrade() else {
                        return true;
                    };
                    ide.update(cx, |ide, cx| ide.allow_close || ide.may_close(win, cx))
                });
                entity
            },
        )
        .expect("failed to open IDE window");
        cx.activate(true);
    });
}

pub struct Ide {
    // files: the open project and its documents (see `workspace`)
    workspace: workspace::Workspace,
    /// The user answered the save prompt: the next close of the IDE's
    /// window goes through without asking again.
    allow_close: bool,
    /// The File menu's recent projects, kept in the user's app-data folder.
    recent: recent::RecentList,
    /// Where `recent` is stored (see `recent_store`).
    recent_file: PathBuf,
    // code editor
    lines: Vec<String>,
    cursor: (usize, usize), // (row, char-col)
    /// The other end of the selection: the text between it and `cursor` is
    /// selected. `None` when nothing is.
    anchor: Option<(usize, usize)>,
    /// Undo and redo for the code.
    history: editor::History,
    /// A mouse button is down in the editor: dragging extends the selection.
    selecting: bool,
    editor_focus: FocusHandle,
    /// The code window's find bar, when it is open.
    find: Option<find_bar::FindBar>,
    find_focus: FocusHandle,
    replace_focus: FocusHandle,
    /// Focus for the designer, so window-level shortcuts (F7) reach the
    /// IDE while no text field is being edited.
    designer_focus: FocusHandle,
    editor_scroll: ScrollHandle,
    designer_scroll: ScrollHandle,
    /// The toolbox's buttons scroll when the window is too short for them.
    toolbox_scroll: ScrollHandle,
    output_scroll: ScrollHandle,
    scrollbar_drag: Option<ScrollbarDrag>,
    /// The sizes of the main window's panes, set by dragging the bars
    /// between them (splitters.rs).
    panes: splitters::Panes,
    split_drag: Option<splitters::SplitDrag>,
    /// The window's size at the last draw, which bounds the panes.
    viewport: gpui::Size<gpui::Pixels>,
    scrollbars_initialized: bool,
    /// Header rows of the code blocks the editor shows folded (collapsed).
    folded: std::collections::BTreeSet<usize>,

    breakpoints: Arc<Mutex<HashSet<usize>>>,
    // form designer
    form: FormSpec,
    selected: Option<usize>,
    /// The project itself is selected in the Project panel: the Properties
    /// panel then shows its package settings (version, icon, bundle id).
    project_selected: bool,
    drag: Option<DesignDrag>, // moving or resizing a control, with the last pointer position
    // property inspector buffers: (control idx or FORM_IDX, prop) -> live text
    prop_bufs: HashMap<(usize, &'static str), String>,
    /// The caret and selection of each project package field (Version, Icon,
    /// Bundle Id), keyed by the field's name.
    prop_sel: HashMap<&'static str, text_field::Sel>,
    /// The first character a project field shows (scrolled to keep the caret in view).
    prop_scroll: HashMap<&'static str, usize>,
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
    /// The running program's windows, by form index — in classic VB each
    /// form *is* a window of the application, not a pane inside the IDE.
    form_windows: HashMap<usize, WindowHandle<FormRunner>>,
    /// Per form of the running program, the events it has handlers for
    /// (`(object, event)` in lower case); the windows skip the rest of the
    /// frequent ones (`MouseMove`).
    live_handlers: Vec<HashSet<(String, String)>>,
    /// Each running form as the program has changed it
    /// (`Label1.Caption = "x"`), kept so a window that opens later (or
    /// reopens) starts from it.
    live_forms: HashMap<usize, (FormSpec, Vec<ControlState>)>,
    status: String,
    /// The combined program of the run in progress, to map its line
    /// numbers back to documents.
    run_lines: Option<program::Program>,
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

        // A `.oxproj` opens that project; otherwise the two files (either
        // may be absent) make a one-form project.
        let opened = bas_path
            .map(std::path::Path::new)
            .filter(|p| workspace::Workspace::is_project_file(p))
            .and_then(|p| workspace::Workspace::open(p).ok());
        let workspace = match opened {
            Some(w) => w,
            None => {
                let lines: Vec<String> =
                    match bas_path.and_then(|p| std::fs::read_to_string(p).ok()) {
                        Some(src) => workspace::to_lines(&src),
                        None => default_src.lines().map(|l| l.to_string()).collect(),
                    };
                let form = match oxform_path
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .and_then(|json| FormSpec::from_oxform(&json).ok())
                {
                    Some(f) => f,
                    None => default_form(),
                };
                let mut w = workspace::Workspace::new_project();
                let name = form.name.clone();
                w.name = bas_path
                    .and_then(|p| std::path::Path::new(p).file_stem())
                    .and_then(|s| s.to_str())
                    .unwrap_or("Project1")
                    .to_string();
                w.startup = name.clone();
                w.docs[0] = workspace::Doc::saved(
                    workspace::DocKind::Form,
                    &name,
                    oxform_path.map(std::path::PathBuf::from),
                    bas_path.map(std::path::PathBuf::from),
                    form,
                    lines,
                );
                // a file that wasn't there yet is a new, unsaved document
                if oxform_path.is_none() || bas_path.is_none() {
                    w.docs[0].mark_unsaved();
                }
                w
            }
        };
        let (form, lines) = (
            workspace.active_doc().form.clone(),
            workspace.active_doc().lines.clone(),
        );

        let mut ide = Ide {
            workspace,
            lines,
            cursor: (0, 0),
            anchor: None,
            history: editor::History::default(),
            selecting: false,
            editor_focus: cx.focus_handle(),
            find: None,
            find_focus: cx.focus_handle(),
            replace_focus: cx.focus_handle(),
            designer_focus: cx.focus_handle(),
            editor_scroll: ScrollHandle::new(),
            designer_scroll: ScrollHandle::new(),
            toolbox_scroll: ScrollHandle::new(),
            output_scroll: ScrollHandle::new(),
            scrollbar_drag: None,
            panes: splitters::Panes::default(),
            split_drag: None,
            viewport: size(px(1180.0), px(760.0)),
            scrollbars_initialized: false,
            breakpoints: Arc::new(Mutex::new(HashSet::new())),
            folded: std::collections::BTreeSet::new(),
            form,
            selected: None,
            project_selected: false,
            drag: None,
            prop_bufs: HashMap::new(),
            prop_focus: HashMap::new(),
            prop_sel: HashMap::new(),
            prop_scroll: HashMap::new(),
            rename_error: None,
            code_window: None,
            code_menu: None,
            code_object: None,
            reveal_cursor: false,
            output: String::new(),
            session: None,
            paused: None,
            live: false,
            form_windows: HashMap::new(),
            live_handlers: Vec::new(),
            live_forms: HashMap::new(),
            status: "Ready".into(),
            run_lines: None,
            allow_close: false,
            recent_file: recent_store::default_file(),
            recent: recent_store::load(&recent_store::default_file()),
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
            DebugEvent::Paused(mut info) => {
                // The program's line numbers are the combined program's:
                // find the document and line the user wrote, and show them.
                if let Some(origin) = self.origin_of(info.line) {
                    info.line = origin.line;
                    if origin.doc != self.workspace.active {
                        self.switch_to(origin.doc);
                    }
                }
                // a form's handlers are `Form2__Command1_Click` in the
                // combined program; show the name the user wrote
                if let Some((_, handler)) = oxbasic_ui::project_host::split_handler(&info.proc) {
                    info.proc = handler.to_string();
                }
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
                    self.anchor = None;
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
            DebugEvent::ChooseFile {
                filter,
                save,
                reply,
            } => choose_file::show_file_dialog(filter, save, reply, cx),
            DebugEvent::Form(snap) => self.on_form_changed(snap, cx),
            DebugEvent::FormShown(snap) => self.open_form_window(snap, cx),
            DebugEvent::FormHidden(i) => self.close_form_window_at(i, false, cx),
            DebugEvent::FormClosed(i) => self.close_form_window_at(i, true, cx),
            DebugEvent::CloseRefused(i) => self.status = format!("Form {} refused to close", i + 1),
            DebugEvent::KeyResult {
                serial,
                keep,
                typed,
            } => self.on_key_result(serial, keep, typed, cx),
            DebugEvent::Handlers(sets) => {
                self.live_handlers = sets;
                for (&i, handle) in &self.form_windows {
                    let set = self.live_handlers.get(i).cloned().unwrap_or_default();
                    let _ = handle.update(cx, |runner, _, _| runner.handlers = Some(set));
                }
            }
            DebugEvent::Live => {
                self.live = true;
                self.paused = None;
                self.status =
                    "Live — the forms run in their own windows; Stop (or closing them) ends the program"
                        .into();
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
}

/// Toolbar actions and control additions.
impl Ide {
    fn run_program(&mut self) {
        if let Some(s) = &self.session {
            s.send(DebugCommand::Continue);
            return;
        }
        self.launch(false);
    }

    fn step_program(&mut self) {
        if let Some(s) = &self.session {
            s.send(DebugCommand::StepOver);
            return;
        }
        self.launch(true);
    }

    /// Step Into (F8): run the current statement, entering a sub it calls.
    fn step_into_program(&mut self) {
        if let Some(s) = &self.session {
            s.send(DebugCommand::StepInto);
            return;
        }
        self.launch(true);
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

    /// File > Save Code: the active document's code.
    fn save_code(&mut self) {
        match self.save_active_code() {
            Ok(()) => {}
            Err(e) => self.status = format!("save failed: {}", e),
        }
    }

    /// File > Save Form: the active form's layout.
    fn save_form(&mut self) {
        self.store_active();
        let i = self.workspace.active;
        let Some(file) = self.workspace.file.clone() else {
            self.status = "save failed: the project has no file yet".into();
            return;
        };
        let dir = file
            .parent()
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        self.workspace.ensure_files(i, &dir);
        let doc = &mut self.workspace.docs[i];
        if doc.kind != DocKind::Form {
            return;
        }
        let path = doc.layout.clone().unwrap_or_default();
        match std::fs::write(&path, doc.form.to_oxform()) {
            Ok(()) => {
                doc.mark_form_saved();
                self.status = format!("Saved {}", file_name(&path));
            }
            Err(e) => self.status = format!("save failed: {}", e),
        }
    }

    /// The problem to show in the code: a syntax error; else a bare word that
    /// names nothing (`Elsee` on its own line); else a misspelled member of a
    /// control (`lstFruit.RemooveItem`). The form's controls are the names the
    /// module may use.
    pub(super) fn current_diagnostic(&self) -> Option<code_model::Diagnostic> {
        // the names the module may use: this form's controls, and the
        // procedures of every other module of the project (a call to a Sub or
        // Function defined in Module1 is not an unknown name here)
        let mut controls: Vec<String> = self.form.controls.iter().map(|c| c.name.clone()).collect();
        for d in &self.workspace.docs {
            if d.kind == crate::ide::workspace::DocKind::Module || d.kind == crate::ide::workspace::DocKind::Form {
                for p in code_model::find_procedures(&d.lines) {
                    controls.push(p.name);
                }
            }
        }
        let kinds: Vec<(String, String)> = self
            .form
            .controls
            .iter()
            .map(|c| (c.name.clone(), c.kind.basic_name().to_string()))
            .collect();
        code_model::diagnostic(&self.lines)
            .or_else(|| code_model::unknown_name(&self.lines, &controls))
            .or_else(|| code_model::unknown_member(&self.lines, &kinds))
    }

    fn toggle_fold(&mut self, row: usize) {
        if !self.folded.insert(row) {
            self.folded.remove(&row);
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
            ControlKind::CheckBox => "Check",
            ControlKind::OptionButton => "Option",
            ControlKind::Image => "Image",
            ControlKind::HScrollBar => "HScroll",
            ControlKind::VScrollBar => "VScroll",
            ControlKind::Line => "Line",
            ControlKind::Shape => "Shape",
            ControlKind::VolumeListBox => "Volume",
            ControlKind::DirListBox => "Dir",
            ControlKind::FileListBox => "File",
            ControlKind::Slider => "Slider",
            ControlKind::ProgressBar => "Progress",
            ControlKind::TabControl => "Tab",
            ControlKind::DataGrid => "Grid",
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
        let rect = if kind == ControlKind::Timer {
            Rect::new(0.0, 0.0, 0.0, 0.0)
        } else {
            Rect::new(16.0, top, w, h)
        };
        let mut spec = ControlSpec::new(&name, kind, rect);
        if kind.has_caption() {
            spec.caption = name.clone();
        }
        if kind.has_text() {
            spec.text = Some(name.clone());
        }
        if kind == ControlKind::Timer {
            spec.visible = false;
            spec.interval_ms = 1000;
        }
        self.form.controls.push(spec);
        self.select(Some(self.form.controls.len() - 1));
    }
}

#[cfg(test)]
mod editor_tests;
#[cfg(test)]
mod diagnostic_tests;
#[cfg(test)]
mod fold_tests;
#[cfg(test)]
mod grid_draw_tests;
#[cfg(test)]
mod grid_input_tests;
#[cfg(test)]
mod event_tests;
#[cfg(test)]
mod member_tests;
#[cfg(test)]
mod multiform_tests;
#[cfg(test)]
mod project_settings_tests;
#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod splitter_tests;
#[cfg(test)]
mod toolbar_tests;
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
            // the program shows its (only) form
            let form = ide.form.clone();
            let state = form.controls.iter().map(ControlState::new).collect();
            ide.on_debug_event(
                DebugEvent::FormShown(FormSnapshot {
                    touched: vec![true; form.controls.len()],
                    form,
                    state,
                    focus: None,
                    form_index: 0,
                }),
                cx,
            );
            cx.notify();
        });
        cx.run_until_parked();

        ide.update(cx, |ide, _| {
            assert!(ide.live);
            assert!(
                ide.form_window().is_some(),
                "going live must open the form in its own window"
            );
        });

        // The program ends -> its form window is taken down with it.
        ide.update(cx, |ide, cx| {
            ide.on_debug_event(DebugEvent::Finished("done".into()), cx);
            cx.notify();
        });
        cx.run_until_parked();
        ide.update(cx, |ide, _| assert!(ide.form_window().is_none()));
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

/// Layout & rendering.
impl Render for Ide {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // the window shows the project, with `*` while anything is unsaved
        let dirty = if self.has_unsaved_work() { " *" } else { "" };
        window.set_window_title(&format!("{}{} — OxidizedBASIC", self.workspace.name, dirty));
        // the window's size bounds how far the bars can be dragged; a
        // window made smaller pulls the panes back into range
        let vs = window.viewport_size();
        if vs != self.viewport {
            self.viewport = vs;
            let view = self.viewport();
            for which in [
                splitters::Split::Toolbox,
                splitters::Split::Side,
                splitters::Split::Project,
                splitters::Split::Output,
            ] {
                let now = self.panes.get(which);
                self.panes.set(which, now, view);
            }
        }
        if !self.scrollbars_initialized {
            self.scrollbars_initialized = true;
            cx.on_next_frame(window, |_this, _window, cx| cx.notify());
        }
        div()
            .id("ide-workspace")
            .on_action(cx.listener(|this, _: &Quit, win, cx| {
                this.quit(win, cx);
            }))
            .on_action(cx.listener(|this, _: &AboutIde, _win, cx| {
                this.open_about(cx);
            }))
            .on_action(cx.listener(|this, _: &NewProject, win, cx| {
                this.new_project(win, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenProject, win, cx| {
                this.open_project(win, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenRecent1, win, cx| {
                this.open_recent(0, win, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenRecent2, win, cx| {
                this.open_recent(1, win, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenRecent3, win, cx| {
                this.open_recent(2, win, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenRecent4, win, cx| {
                this.open_recent(3, win, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenRecent5, win, cx| {
                this.open_recent(4, win, cx);
            }))
            .on_action(cx.listener(|this, _: &ClearRecent, _win, cx| {
                this.clear_recent(cx);
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
            .on_action(cx.listener(|this, _: &SaveProjectAs, win, cx| {
                this.save_project_as(win, cx);
            }))
            .on_action(cx.listener(|this, _: &PackageProject, win, cx| {
                this.package_project(win, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseProject, win, cx| {
                this.close_project(win, cx);
            }))
            .on_action(cx.listener(|this, _: &AddForm, _win, cx| {
                this.add_form(cx);
            }))
            .on_action(cx.listener(|this, _: &SetStartForm, _win, cx| {
                this.set_start_form(cx);
            }))
            .on_action(cx.listener(|this, _: &AddModule, _win, cx| {
                this.add_module(cx);
            }))
            .on_action(cx.listener(|this, _: &RemoveDocument, win, cx| {
                this.remove_active(win, cx);
            }))
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
            .on_action(cx.listener(|this, _: &StepIntoProgram, _win, cx| {
                this.step_into_program();
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
                this.update_split_drag(ev, cx);
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _ev: &gpui::MouseUpEvent, _win, cx| {
                    this.scrollbar_drag = None;
                    if this.split_drag.take().is_some() {
                        cx.notify();
                    }
                    if let Some(DesignDrag::Move(idx, ..)) = this.drag.take() {
                        this.drop_into_container(idx);
                        cx.notify();
                    }
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
                    .px_1()
                    .pt_1()
                    .child(self.render_toolbox(cx))
                    .child(self.splitter(splitters::Split::Toolbox, cx))
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
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(self.render_designer(cx)),
                            ),
                    )
                    .child(self.splitter(splitters::Split::Side, cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .w(px(self.panes.side_w))
                            .min_w_0()
                            .child(self.render_project_explorer(cx))
                            .child(self.splitter(splitters::Split::Project, cx))
                            .child(self.render_inspector(window, cx)),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .child(self.splitter(splitters::Split::Output, cx)),
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
    /// The Project panel: the project's name and its forms and modules.
    /// Clicking a document shows it (its layout, or its code for a module);
    /// a form's second line shows its code file, double-clicked to edit.
    fn render_project_explorer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ws = &self.workspace;
        let mut list = div().p_2().flex().flex_col().gap_1().child(
            div()
                .id("project-row")
                .debug_selector(|| "project-row".into())
                .px_1()
                .rounded_sm()
                .cursor_pointer()
                .when(self.project_selected, |s| s.bg(rgb(SEL_BG)))
                .hover(|s| s.bg(rgb(SEL_BG)))
                .child(SharedString::from(format!(
                    "▾  {}{}",
                    ws.name,
                    if ws.is_dirty() || self.working_copy_changed() {
                        " ●"
                    } else {
                        ""
                    }
                )))
                .on_click(cx.listener(|this: &mut Ide, _ev, _win, cx| {
                    // the project's settings replace the form's in Properties
                    this.project_selected = true;
                    this.selected = None;
                    cx.notify();
                })),
        );
        for (i, doc) in ws.docs.iter().enumerate() {
            let active = i == ws.active;
            let icon = match doc.kind {
                DocKind::Form => "◻",
                DocKind::Module => "◧",
            };
            let startup = doc.kind == DocKind::Form && doc.name.eq_ignore_ascii_case(&ws.startup);
            // the active document's unsaved edits are in the working copy
            let unsaved = if active {
                self.working_copy_changed() || doc.is_dirty()
            } else {
                doc.is_dirty()
            };
            let label = format!(
                "{}  {}{}{}",
                icon,
                doc.name,
                if startup { "  (start)" } else { "" },
                if unsaved { " ●" } else { "" }
            );
            list = list.child(
                div()
                    .id(SharedString::from(format!("doc-{}", i)))
                    .debug_selector(move || format!("project-doc-{}", i))
                    .px_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .when(active, |s| s.bg(rgb(SEL_BG)))
                    .hover(|s| s.bg(rgb(SEL_BG)))
                    .child(SharedString::from(format!("    {}", label)))
                    .on_click(cx.listener(
                        move |this: &mut Ide, ev: &gpui::ClickEvent, _win, cx| {
                            this.switch_to(i);
                            if ev.click_count() >= 2 {
                                // double-click: the code window for the document
                                this.show_code_window(cx);
                            }
                            cx.notify();
                        },
                    )),
            );
        }
        div()
            .flex_none()
            .h(px(self.panes.project_h))
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(WINDOW_BG))
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.dock_title("Project"))
            .child(list)
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
            .flex_none()
            .w(px(self.panes.toolbox_w))
            .min_w_0()
            // the column takes no more height than the row it sits in, so the
            // scrolling buttons stay above the Immediate pane instead of under it
            .min_h_0()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PANEL))
            .flex()
            .flex_col()
            .child(self.dock_title("Toolbox"))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("toolbox-viewport")
                            .size_full()
                            .overflow_scroll()
                            .scrollbar_width(px(8.0))
                            .track_scroll(&self.toolbox_scroll)
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .content_start()
                                    .p_1()
                                    .child(self.toolbox_button("Aa Label", ControlKind::Label, cx))
                                    .child(self.toolbox_button(
                                        "ab TextBox",
                                        ControlKind::TextBox,
                                        cx,
                                    ))
                                    .child(self.toolbox_button(
                                        "OK CommandButton",
                                        ControlKind::CommandButton,
                                        cx,
                                    ))
                                    .child(self.toolbox_button("☐ Frame", ControlKind::Frame, cx))
                                    .child(self.toolbox_button(
                                        "≡ ListBox",
                                        ControlKind::ListBox,
                                        cx,
                                    ))
                                    .child(self.toolbox_button(
                                        "▤ VolumeListBox",
                                        ControlKind::VolumeListBox,
                                        cx,
                                    ))
                                    .child(self.toolbox_button(
                                        "▥ DirListBox",
                                        ControlKind::DirListBox,
                                        cx,
                                    ))
                                    .child(self.toolbox_button(
                                        "▦ FileListBox",
                                        ControlKind::FileListBox,
                                        cx,
                                    ))
                                    .child(self.toolbox_button("⇆ Slider", ControlKind::Slider, cx))
                                    .child(self.toolbox_button(
                                        "▰ ProgressBar",
                                        ControlKind::ProgressBar,
                                        cx,
                                    ))
                                    .child(self.toolbox_button(
                                        "▤ TabControl",
                                        ControlKind::TabControl,
                                        cx,
                                    ))
                                    .child(self.toolbox_button(
                                        "⌄ ComboBox",
                                        ControlKind::ComboBox,
                                        cx,
                                    ))
                                    .child(self.toolbox_button(
                                        "▧ PictureBox",
                                        ControlKind::PictureBox,
                                        cx,
                                    ))
                                    .child(self.toolbox_button(
                                        "☑ CheckBox",
                                        ControlKind::CheckBox,
                                        cx,
                                    ))
                                    .child(self.toolbox_button(
                                        "◉ OptionButton",
                                        ControlKind::OptionButton,
                                        cx,
                                    ))
                                    .child(self.toolbox_button("▣ Image", ControlKind::Image, cx))
                                    .child(self.toolbox_button("◷ Timer", ControlKind::Timer, cx))
                                    .child(self.toolbox_button(
                                        "↔ HScrollBar",
                                        ControlKind::HScrollBar,
                                        cx,
                                    ))
                                    .child(self.toolbox_button(
                                        "↕ VScrollBar",
                                        ControlKind::VScrollBar,
                                        cx,
                                    ))
                                    .child(self.toolbox_button("╱ Line", ControlKind::Line, cx))
                                    .child(self.toolbox_button("▭ Shape", ControlKind::Shape, cx)),
                            ),
                    )
                    .child(self.scrollbar_overlays("toolbox", &self.toolbox_scroll, cx)),
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
        // the overlay is a child of the scrolled area, so it moves with the
        // content: shift it back by the scroll offset to keep the bars on
        // the visible edges
        let overlay = div()
            .debug_selector(move || format!("scrollbars-{}", pane))
            .absolute()
            .left(-offset.x)
            .top(-offset.y)
            .w(viewport.width)
            .h(viewport.height)
            .child(if max.height > px(0.0) {
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
                                cx.listener(
                                    move |this: &mut Ide, ev: &MouseDownEvent, _win, cx| {
                                        cx.stop_propagation();
                                        this.scrollbar_drag = Some(ScrollbarDrag {
                                            pane,
                                            vertical: true,
                                            pointer_start: f32::from(ev.position.y),
                                            offset_start: f32::from(scroll.offset().y),
                                            max_offset: f32::from(scroll.max_offset().height),
                                            thumb_travel: travel,
                                        });
                                    },
                                ),
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
            "toolbox" => self.toolbox_scroll.clone(),
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
        let selection = self.selection();
        // the find bar's matches and the current one, tinted in the code
        let find_matches: Vec<editor::Range> = self
            .find
            .as_ref()
            .map(|b| b.found.clone())
            .unwrap_or_default();
        let find_current = self.find.as_ref().and_then(|b| b.current);
        let bps = self.breakpoints.lock().unwrap().clone();
        // Code folding: the blocks in the source, and the rows hidden by the
        // ones that are folded. The buffer itself is unchanged, so line
        // numbers, breakpoints and the debugger keep their meaning.
        let folds = code_model::find_folds(&self.lines);
        // the caret is never left on a hidden row: open whatever hides it
        // (typing, moving, find and navigation all reach here)
        while let Some(f) = code_model::folded_block_hiding(&folds, &self.folded, self.cursor.0) {
            self.folded.remove(&f.start);
        }
        // Diagnostics: a syntax error in the module is underlined on its row.
        let diag = self.current_diagnostic();
        // Colouring: the whole module is parsed once, so a block's header and
        // footer lines get their grammar colours.
        let token_rows = highlight::tokenize_module(&self.lines);
        let hidden = code_model::hidden_rows(&folds, &self.folded);
        let fold_of_row: std::collections::HashMap<usize, bool> = folds
            .iter()
            .map(|f| (f.start, self.folded.contains(&f.start)))
            .collect();
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
            // a row inside a folded block is not drawn at all
            if hidden.contains(&idx) {
                continue;
            }
            let lineno = idx + 1;
            let has_bp = bps.contains(&lineno);
            let is_paused = paused_line == Some(lineno);
            let is_cursor = self.cursor.0 == idx;

            // a header that opens a block shows its fold marker: ▾ open, ▸ folded
            let foldable = fold_of_row.contains_key(&idx);
            let fold_marker = match fold_of_row.get(&idx) {
                Some(true) => "\u{25b8}",
                Some(false) => "\u{25be}",
                None => "",
            };

            // the fold marker is its own cell, beside the gutter: a click on it
            // folds the block and does not toggle a breakpoint
            let fold_cell = div()
                .id((SharedString::from("fold"), idx))
                .debug_selector(move || format!("fold-{}", idx))
                .w(px(14.0))
                .h_full()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(DIM))
                .cursor_pointer()
                .child(SharedString::from(fold_marker))
                .on_click(cx.listener(
                    move |this: &mut Ide, _ev: &gpui::ClickEvent, _win, cx| {
                        if foldable {
                            this.toggle_fold(idx);
                        }
                        cx.notify();
                    },
                ));

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
            let len = line.chars().count();
            let selected = selection.and_then(|r| r.columns_on(idx, len));
            let caret = is_cursor.then_some(self.cursor.1);
            // the find matches on this row, as (from, to, is current)
            let found: Vec<(usize, usize, bool)> = find_matches
                .iter()
                .enumerate()
                .filter(|(_, r)| r.start.0 <= idx && idx <= r.end.0)
                .filter_map(|(i, r)| {
                    let from = if r.start.0 == idx { r.start.1 } else { 0 };
                    let to = if r.end.0 == idx { r.end.1 } else { len };
                    (from < to).then_some((from, to, find_current == Some(i)))
                })
                .collect();
            let error_from = diag.as_ref().filter(|d| d.row == idx).map(|d| d.column);
            code = code.child(render_code_line(
                &token_rows[idx],
                selected,
                caret,
                &found,
                error_from,
            ));

            list = list.child(
                div()
                    .debug_selector(|| format!("editor-row-{}", idx))
                    .flex()
                    .flex_none()
                    .w(px(content_width))
                    .h_5()
                    .items_center()
                    .whitespace_nowrap()
                    .child(fold_cell)
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
                if this.editor_key(ev, cx) {
                    // keep the caret in view, and stop the key reaching the
                    // window's own shortcuts
                    this.reveal_cursor = true;
                    cx.stop_propagation();
                }
                this.code_object = None;
                cx.notify();
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut Ide, ev: &MouseDownEvent, win, cx| {
                    // click-to-type: focus the editor and put the caret
                    // under the pointer (or select a word or a line)
                    win.focus(&this.editor_focus);
                    this.code_menu = None;
                    this.code_object = None;
                    this.editor_mouse_down(ev, win);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this: &mut Ide, ev: &MouseMoveEvent, win, cx| {
                if this.editor_mouse_drag(ev, win) {
                    cx.notify();
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut Ide, _ev: &gpui::MouseUpEvent, _win, _cx| {
                    this.selecting = false;
                }),
            )
            .child(list)
            .child(self.scrollbar_overlays("editor", &self.editor_scroll, cx))
    }
}

/// The selection's tint behind code.
const SELECTION_BG: u32 = 0xB4D7FF;
/// The tint of a find match, and the stronger tint of the current one.
const FIND_BG: u32 = 0xFFF2A8;
const FIND_CURRENT_BG: u32 = 0xFFB020;

/// One line of code with its syntax colours, the selected columns
/// (`(from, to)`; `to` is `None` when the selection runs on into the next
/// line) tinted, and the caret drawn as a thin bar before column `caret`.
fn render_code_line(
    tokens: &[(String, highlight::Tok)],
    selected: Option<(usize, Option<usize>)>,
    caret: Option<usize>,
    found: &[(usize, usize, bool)],
    error_from: Option<usize>,
) -> gpui::Div {
    // colour each character by its token, then cut the line wherever the
    // selection, a find match or the caret starts or ends
    let mut colors: Vec<(char, u32)> = Vec::new();
    for (text, kind) in tokens {
        let color = match kind {
            highlight::Tok::Keyword => C_KEYWORD,
            highlight::Tok::Str => C_STRING,
            highlight::Tok::Num => C_NUMBER,
            highlight::Tok::Comment => C_COMMENT,
            highlight::Tok::Ident => C_IDENT,
            highlight::Tok::Punct => C_PUNCT,
        };
        colors.extend(text.chars().map(|c| (c, color)));
    }
    let len = colors.len();
    let (sel_from, sel_to) = match selected {
        Some((a, b)) => (a, b.unwrap_or(len)),
        None => (len + 1, len + 1),
    };
    // per character: 0 no match, 1 a match, 2 the current match
    let mut mark = vec![0u8; len];
    for &(from, to, current) in found {
        for m in mark.iter_mut().take(to.min(len)).skip(from) {
            *m = (*m).max(if current { 2 } else { 1 });
        }
    }
    // the syntax error runs from its column to the end of the row
    let is_error = |i: usize| error_from.is_some_and(|from| i >= from);
    let caret_bar = || div().flex_none().w(px(1.5)).h(px(15.0)).bg(rgb(ACCENT));
    let mut row = div().flex().items_center();
    let mut i = 0;
    while i < len {
        if caret == Some(i) {
            row = row.child(caret_bar());
        }
        let in_sel = i >= sel_from && i < sel_to;
        let color = colors[i].1;
        // a run of characters with the same colour, selection, match and error
        let mut j = i + 1;
        while j < len
            && colors[j].1 == color
            && (j >= sel_from && j < sel_to) == in_sel
            && mark[j] == mark[i]
            && is_error(j) == is_error(i)
            && caret != Some(j)
        {
            j += 1;
        }
        let text: String = colors[i..j].iter().map(|(c, _)| c).collect();
        let mut span = div().text_color(rgb(color)).child(SharedString::from(text));
        if in_sel {
            span = span.bg(rgb(SELECTION_BG));
        } else if mark[i] == 2 {
            // the current match, stronger than the others
            span = span.bg(rgb(FIND_CURRENT_BG));
        } else if mark[i] == 1 {
            span = span.bg(rgb(FIND_BG));
        }
        if is_error(i) {
            span = span.border_b_2().border_color(rgb(C_ERROR));
        }
        row = row.child(span);
        i = j;
    }
    // the line break is selected: show it as a little space after the text
    if selected.is_some_and(|(_, to)| to.is_none()) {
        row = row.child(div().w(px(6.0)).h(px(15.0)).bg(rgb(SELECTION_BG)));
    }
    if caret.is_some_and(|c| c >= len) {
        row = row.child(caret_bar());
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
            // the form keeps its own size whatever room the designer has:
            // a narrower pane scrolls instead of squeezing it
            .flex_none()
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
            let mut look = ControlLook::design(&c);
            look.form_size = Some((self.form.width, self.form.height));
            look.base_dir = self
                .workspace
                .file
                .as_deref()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf());
            // inside a Frame or PictureBox: drawn where its container puts it
            let (ox, oy) = form.form_origin(idx);
            if !is_timer {
                look.origin = Some((ox, oy));
            }
            // and the selection outline follows it there
            let shown_rect = if is_timer {
                c.rect
            } else {
                Rect::new(ox, oy, c.rect.width, c.rect.height)
            };
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
                        .left(px(shown_rect.left - 2.0))
                        .top(px(shown_rect.top - 2.0))
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
                        let (hx, hy) = h.center(&shown_rect);
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

        // the designer region tracks mouse movement for dragging; it fills
        // its pane and scrolls a form bigger than that
        div()
            .id("designer")
            .debug_selector(|| "designer-viewport".into())
            .size_full()
            .overflow_scroll()
            .scrollbar_width(px(8.0))
            .track_scroll(&self.designer_scroll)
            .bg(rgb(PANEL))
            .flex()
            .items_start()
            .justify_start()
            // the margin is inside the scrolled content, so the scroll range is
            // only what the form needs beyond the pane: a form that fits shows
            // no bars
            .child(div().flex_none().p_4().child(form_area))
            .child(self.scrollbar_overlays("designer", &self.designer_scroll, cx))
    }
    /// A control dragged in the designer is dropped where its top-left
    /// corner is: into the topmost Frame or PictureBox there (not one inside
    /// it), or onto the form. It stays where it is on screen; its
    /// `Left`/`Top` become relative to its new container, as in VB.
    fn drop_into_container(&mut self, idx: usize) {
        if idx >= self.form.controls.len() || self.form.controls[idx].kind == ControlKind::Timer {
            return;
        }
        let (x, y) = self.form.form_origin(idx);
        let target = oxbasic_ui::gpui_controls::z_order(&self.form)
            .into_iter()
            .rev()
            .find(|&j| {
                let c = &self.form.controls[j];
                let (l, t) = self.form.form_origin(j);
                j != idx
                    && c.kind.is_container()
                    && !self.form.contains(idx, j)
                    && x >= l
                    && x < l + c.rect.width
                    && y >= t
                    && y < t + c.rect.height
            });
        let (bx, by) = target.map_or((0.0, 0.0), |j| self.form.form_origin(j));
        let key = target.map(|j| oxbasic_ui::container_key(&self.form.controls[j]));
        let c = &mut self.form.controls[idx];
        if c.container != key {
            c.container = key;
            c.rect.left = (x - bx).round();
            c.rect.top = (y - by).round();
        }
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
                // on the form a control stays inside it; in a container it
                // may go past the container's edge (the drop takes it out)
                let floor = if self.form.container_of(idx).is_some() {
                    f32::MIN
                } else {
                    0.0
                };
                let c = &mut self.form.controls[idx];
                c.rect.left = (c.rect.left + x - last_x).max(floor).round();
                c.rect.top = (c.rect.top + y - last_y).max(floor).round();
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
    /// Which form of the project this window shows (0 in a one-form
    /// program); events and edits are addressed to it.
    form_index: usize,
    /// Run-time state (text, selection, open list, value) per control.
    state: Vec<ControlState>,
    /// Keyboard focus handles for the controls that take the keyboard.
    focus: HashMap<usize, FocusHandle>,
    /// The running timers, by control: `(interval in ms, serial)`. A timer
    /// whose interval or `Enabled` the program changed is restarted under a
    /// new serial; the task of the old one sees that and ends.
    timers: HashMap<usize, (u64, u64)>,
    timer_serial: u64,
    /// The project's folder, which a PictureBox's `Picture` file name is
    /// relative to (`LoadPicture("logo.png")`).
    base_dir: Option<std::path::PathBuf>,
    /// The modifier keys held at the last press (a MultiSelect list's
    /// Ctrl- and Shift-clicks).
    press_mods: gpui::Modifiers,
    /// The text field a button went down in: moving with it held extends
    /// its selection.
    text_drag: Option<usize>,
    /// The scrollbar a button went down on: moving with it held sets its
    /// `Value` from the pointer's place along the track.
    scroll_drag: Option<usize>,
    /// Where the scrollbar drag began (window position, the value then).
    scroll_start: Option<(gpui::Point<gpui::Pixels>, i32)>,
    /// The events the program has handlers for, as `(object, event)` in
    /// lower case: events nobody handles aren't sent. `None` until the
    /// program says (then every event VB raises is sent).
    handlers: Option<HashSet<(String, String)>>,
    /// The focus of the form itself, for keys when no control has it.
    form_focus: FocusHandle,
    /// Mouse capture, as VB does it: the object a button was pressed on
    /// (`None` = the form) gets every move and the release until the
    /// button comes up, wherever the pointer goes.
    pressed: Option<live_input::Press>,
    /// The control that has the keyboard focus, as the program was told
    /// (`GotFocus` / `LostFocus` fire when it changes).
    focused: Option<usize>,
    /// Keys sent to the program whose answer hasn't come back: the window
    /// applies each when the program says whether, and what, it types.
    pending_keys: HashMap<u64, (usize, gpui::Keystroke)>,
    /// The DataGrid cell being typed into, if any (its control and edit).
    cell_edit: Option<(usize, oxbasic_ui::grid_edit::CellEdit)>,
    key_serial: u64,
    /// The window's last inner size, to raise `Form_Resize` when it changes.
    last_size: Option<(f32, f32)>,
    /// The window is the active one (for `Form_Activate`/`Deactivate`).
    active: bool,
    _subscriptions: Vec<gpui::Subscription>,
}

impl FormRunner {
    fn new(
        form: FormSpec,
        form_index: usize,
        session: DebugSession,
        cx: &mut Context<Self>,
    ) -> FormRunner {
        let state = form.controls.iter().map(ControlState::new).collect();
        let mut focus = HashMap::new();
        for (i, c) in form.controls.iter().enumerate() {
            if c.kind.can_focus() {
                focus.insert(i, cx.focus_handle());
            }
        }
        let mut runner = FormRunner {
            form,
            session,
            form_index,
            state,
            focus,
            timers: HashMap::new(),
            timer_serial: 0,
            base_dir: None,
            press_mods: gpui::Modifiers::none(),
            text_drag: None,
            scroll_drag: None,
            scroll_start: None,
            handlers: None,
            form_focus: cx.focus_handle(),
            pressed: None,
            focused: None,
            pending_keys: HashMap::new(),
            cell_edit: None,
            key_serial: 0,
            last_size: None,
            active: false,
            _subscriptions: Vec::new(),
        };
        runner.sync_timers(cx);
        runner
    }

    /// Start, stop or restart each Timer so it matches the form: it runs
    /// while `Enabled` and its `Interval` is above 0, and fires
    /// `<Timer>_Timer` (with its `Index`, in a control array) every
    /// interval — as the program sets `Timer1.Enabled` / `Timer1.Interval`.
    fn sync_timers(&mut self, cx: &mut Context<Self>) {
        let wanted: Vec<(usize, Option<u64>)> = self
            .form
            .controls
            .iter()
            .enumerate()
            .filter(|(_, c)| c.kind == ControlKind::Timer)
            .map(|(i, c)| (i, (c.enabled && c.interval_ms > 0).then_some(c.interval_ms)))
            .collect();
        for (i, want) in wanted {
            if want == self.timers.get(&i).map(|t| t.0) {
                continue;
            }
            self.timers.remove(&i);
            let Some(ms) = want else { continue };
            self.timer_serial += 1;
            let serial = self.timer_serial;
            self.timers.insert(i, (ms, serial));
            cx.spawn(async move |this, cx| loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(ms))
                    .await;
                let again = this.update(cx, |this, cx| {
                    if this.timers.get(&i).map(|t| t.1) != Some(serial) {
                        return false; // stopped or restarted
                    }
                    this.fire(i, "Timer");
                    cx.notify();
                    true
                });
                if !matches!(again, Ok(true)) {
                    break; // window closed, or this timer was replaced
                }
            })
            .detach();
        }
    }

    /// Fire `<control>_<event>` in the running program (a missing handler
    /// is a no-op there). A control array element's handler also gets its
    /// `Index`.
    fn fire(&self, idx: usize, event: &str) {
        let c = &self.form.controls[idx];
        self.session.send(DebugCommand::Call(
            self.form_index,
            format!("{}_{}", c.name, event),
            c.index.into_iter().collect(),
        ));
    }

    /// Tell the program what the user did, before the event it causes, so
    /// the handler reads the new `Text` / `ListIndex`.
    fn tell(&self, edit: UserEdit) {
        self.session.send(DebugCommand::Edit(self.form_index, edit));
    }

    /// Show the form as the program has changed it: its properties, the
    /// state of the controls it touched, the timers it started or stopped,
    /// and the control it gave the focus to.
    fn show(&mut self, snap: &FormSnapshot, win: &mut Window, cx: &mut Context<Self>) {
        // Load / Unload / ZOrder changed the controls: what the window keys
        // by a control's place (focus handles, timers) follows its control
        let before: Vec<(String, Option<i32>)> = self
            .form
            .controls
            .iter()
            .map(|c| (c.name.to_lowercase(), c.index))
            .collect();
        snap.apply_to(&mut self.form, &mut self.state);
        let after: Vec<(String, Option<i32>)> = self
            .form
            .controls
            .iter()
            .map(|c| (c.name.to_lowercase(), c.index))
            .collect();
        if before != after {
            self.renumber(&before, &after, cx);
        }
        win.set_window_title(&self.form.caption);
        self.sync_timers(cx);
        // `SetFocus`
        if let Some(i) = snap.focus {
            if let Some(handle) = self.focus.get(&i) {
                win.focus(handle);
            }
        }
    }

    /// The controls were reordered or some added or removed: keep each
    /// control's focus handle and timer, make handles for new controls, and
    /// forget those of removed ones.
    fn renumber(
        &mut self,
        before: &[(String, Option<i32>)],
        after: &[(String, Option<i32>)],
        cx: &mut Context<Self>,
    ) {
        let old_focus = std::mem::take(&mut self.focus);
        // a running timer's task holds its control's place: the timers
        // start again (sync_timers) at their new places
        self.timers.clear();
        let place = |key: &(String, Option<i32>)| before.iter().position(|k| k == key);
        for (i, key) in after.iter().enumerate() {
            let old = place(key);
            if self.form.controls[i].kind.can_focus() {
                let handle = old
                    .and_then(|o| old_focus.get(&o).cloned())
                    .unwrap_or_else(|| cx.focus_handle());
                self.focus.insert(i, handle);
            }
        }
        self.focused = self
            .focused
            .and_then(|f| before.get(f))
            .and_then(|key| after.iter().position(|k| k == key));
        self.pressed = None;
        self.pending_keys.clear();
    }

    fn close_lists(&mut self, keep: Option<usize>) {
        for (i, s) in self.state.iter_mut().enumerate() {
            if Some(i) != keep {
                s.dropped = false;
            }
        }
    }

    /// Whether pressing `part` of control `idx` clicks it on release (VB
    /// clicks a button, a check box, a label... when the button comes up
    /// over it); a list row and a combo's arrow act on the press instead.
    fn clicks_on_release(&self, idx: usize, part: Part) -> bool {
        let kind = self.form.controls[idx].kind;
        match part {
            Part::ListItem(_) | Part::DropItem(_) | Part::Arrow => false,
            Part::Field if kind == ControlKind::ComboBox => false,
            // a list's own background (below its rows) raises no Click
            _ if kind == ControlKind::ListBox => false,
            // the text box clicks; it also keeps the caret where it was
            _ => true,
        }
    }

    /// What a press on `part` of control `idx` does by itself, before the
    /// release: a list row is picked (its `Click` follows straight away, as
    /// in VB), a combo's ▾ drops its list.
    fn act_on_press(&mut self, idx: usize, part: Part) {
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
                        self.fire(idx, "DropDown");
                    }
                } else {
                    self.close_lists(None);
                }
            }
            Part::ListItem(i) | Part::DropItem(i) => {
                // MultiSelect: a click (Simple, 1) or a Ctrl-click
                // (Extended, 2) toggles the row; Extended's plain click
                // selects just it
                if spec.kind == ControlKind::ListBox && spec.multi_select != 0 {
                    let ctrl = self.press_mods.control || self.press_mods.platform;
                    let shift = self.press_mods.shift;
                    if spec.multi_select == 1 || ctrl {
                        self.toggle_row(idx, i);
                    } else if shift {
                        // Extended + Shift: the rows from the focused one
                        let from = self.state[idx].list_index.unwrap_or(i);
                        let (a, b) = (from.min(i), from.max(i));
                        self.state[idx].selected = (a..=b).collect();
                        self.state[idx].list_index = Some(i);
                        self.tell_selected(idx);
                        self.fire(idx, "Click");
                    } else {
                        self.state[idx].selected = vec![i];
                        self.state[idx].list_index = Some(i);
                        self.tell_selected(idx);
                        self.fire(idx, "Click");
                    }
                    return;
                }
                let was = self.state[idx].text.clone();
                self.state[idx].pick(&spec, i);
                self.close_lists(None);
                self.tell(UserEdit::Pick {
                    control: idx,
                    index: i,
                });
                if spec.kind == ControlKind::ComboBox && self.state[idx].text != was {
                    self.fire(idx, "Change");
                }
                self.fire(idx, "Click");
            }
            _ => self.close_lists(None),
        }
    }

    /// The release clicked control `idx`: a CheckBox toggles, an
    /// OptionButton is chosen, and its `Click` runs.
    fn click(&mut self, idx: usize) {
        let spec = self.form.controls[idx].clone();
        if spec.kind.has_value() {
            // A click toggles a CheckBox (0 <-> 1; a grayed one is
            // checked) and selects an OptionButton, clearing the others.
            let value = match spec.kind {
                ControlKind::CheckBox => {
                    if self.state[idx].value == 1 {
                        0
                    } else {
                        1
                    }
                }
                _ => 1,
            };
            set_value(&self.form, &mut self.state, idx, value);
            self.tell(UserEdit::Value {
                control: idx,
                value,
            });
        }
        self.fire(idx, "Click");
    }

    /// Handle a keystroke in control `idx`. Returns whether it changed.
    fn key(&mut self, idx: usize, ks: &gpui::Keystroke) -> bool {
        let spec = self.form.controls[idx].clone();
        let editable = match spec.kind {
            ControlKind::TextBox => true,
            ControlKind::ComboBox => spec.style != 2,
            ControlKind::ListBox => return self.list_key(idx, ks),
            _ => false,
        };
        if !editable {
            return false;
        }
        let typed = ks_text(ks);
        let m = &ks.modifiers;
        if m.platform || m.control {
            return false;
        }
        let (changed, moved) = self.state[idx].edit_key(&spec, &ks.key, typed.as_deref(), m.shift);
        if changed {
            self.state[idx].list_index = None;
            self.tell(UserEdit::Text {
                control: idx,
                text: self.state[idx].text.clone(),
            });
        }
        if changed || moved {
            // after the text: SelStart / SelLength, as the program reads them
            self.tell(UserEdit::Caret {
                control: idx,
                caret: self.state[idx].caret,
                anchor: self.state[idx].anchor,
            });
        }
        if changed {
            self.fire(idx, "Change");
        }
        changed || moved
    }

    /// Up / Down (and Space, in a MultiSelect list) in a ListBox: move the
    /// focused row, picking it in a one-row list, as VB does.
    fn list_key(&mut self, idx: usize, ks: &gpui::Keystroke) -> bool {
        let spec = self.form.controls[idx].clone();
        let n = spec.items.len();
        if n == 0 {
            return false;
        }
        let at = self.state[idx].list_index;
        let to = match ks.key.as_str() {
            "up" => at.map_or(0, |i| i.saturating_sub(1)),
            "down" => at.map_or(0, |i| (i + 1).min(n - 1)),
            "home" => 0,
            "end" => n - 1,
            "space" if spec.multi_select != 0 => {
                let row = at.unwrap_or(0);
                self.toggle_row(idx, row);
                return true;
            }
            _ => return false,
        };
        if Some(to) == at {
            return false;
        }
        if spec.multi_select == 0 {
            self.act_on_press(idx, Part::ListItem(to));
        } else {
            // the focus moves; the selection stays (Space toggles it)
            self.state[idx].list_index = Some(to);
            self.tell_selected(idx);
        }
        true
    }

    /// A MultiSelect list's row `row` is selected or not, the other way
    /// round, and has the focus.
    fn toggle_row(&mut self, idx: usize, row: usize) {
        let s = &mut self.state[idx];
        if let Some(p) = s.selected.iter().position(|&r| r == row) {
            s.selected.remove(p);
        } else {
            s.selected.push(row);
            s.selected.sort_unstable();
        }
        s.list_index = Some(row);
        self.tell_selected(idx);
        self.fire(idx, "Click");
    }

    fn tell_selected(&self, idx: usize) {
        self.tell(UserEdit::Selected {
            control: idx,
            rows: self.state[idx].selected.clone(),
            index: self.state[idx].list_index,
        });
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
        if idx == PROJECT_IDX {
            return match key {
                "version" => self.workspace.version.clone(),
                "icon" => self.workspace.icon.clone(),
                "bundleid" => self.workspace.bundle_id.clone(),
                _ => String::new(),
            };
        }
        if idx == FORM_IDX {
            let f = &self.form;
            return match key {
                "name" => f.name.clone(),
                "caption" => f.caption.clone(),
                "width" => format!("{}", f.width),
                "height" => format!("{}", f.height),
                "keypreview" => if f.key_preview { "True" } else { "False" }.to_string(),
                _ => String::new(),
            };
        }
        let Some(c) = self.form.controls.get(idx) else {
            return String::new();
        };
        match key {
            "tabindex" => format!("{}", self.form.tab_index_of(idx)),
            "tabstop" => if c.tab_stop { "True" } else { "False" }.to_string(),
            "locked" => if c.locked { "True" } else { "False" }.to_string(),
            "maxlength" => format!("{}", c.max_length),
            "multiline" => if c.multi_line { "True" } else { "False" }.to_string(),
            "scrollbars" => format!("{}", c.scroll_bars),
            "multiselect" => format!("{}", c.multi_select),
            "picture" => c.picture.clone(),
            "stretch" => if c.stretch { "True" } else { "False" }.to_string(),
            "autosize" => if c.auto_size { "True" } else { "False" }.to_string(),
            // blank: on the form itself
            "container" => c.container.clone().unwrap_or_default(),
            "tooltiptext" => c.tool_tip.clone(),
            "mousepointer" => format!("{}", c.mouse_pointer),
            "mouseicon" => c.mouse_icon.clone(),
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
            "value" => format!("{}", c.value),
            "index" => c.index.map(|i| i.to_string()).unwrap_or_default(),
            "enabled" => if c.enabled { "True" } else { "False" }.to_string(),
            "backcolor" => match c.back_system {
                Some(sys) => format!("&H{:08X}&", sys),
                None => c.color.map(|v| format!("#{:06X}", v)).unwrap_or_default(),
            },
            "forecolor" => match c.fore_system {
                Some(sys) => format!("&H{:08X}&", sys),
                None => c
                    .fore_color
                    .map(|v| format!("#{:06X}", v))
                    .unwrap_or_default(),
            },
            "fontname" => c.font.name.clone(),
            "fontsize" => format!("{}", c.font.size),
            "fontbold" => if c.font.bold { "True" } else { "False" }.to_string(),
            "fontitalic" => if c.font.italic { "True" } else { "False" }.to_string(),
            "fontunderline" => if c.font.underline { "True" } else { "False" }.to_string(),
            _ => String::new(),
        }
    }

    /// A colour typed in the inspector: `#RRGGBB`, `&HBBGGRR` (VB's order)
    /// or blank for "the control's usual colour".
    fn parse_color(val: &str) -> Option<Option<u32>> {
        let v = val.trim();
        if v.is_empty() {
            return Some(None);
        }
        if let Some(hex) = v.strip_prefix('#') {
            return u32::from_str_radix(hex, 16)
                .ok()
                .filter(|n| *n <= 0xFF_FFFF)
                .map(Some);
        }
        let hex = v
            .strip_prefix("&H")
            .or_else(|| v.strip_prefix("&h"))
            .map(|h| h.trim_end_matches('&'))?;
        let vb = i64::from_str_radix(hex, 16).ok()?;
        oxbasic_ui::host::from_vb_color(vb).ok().map(Some)
    }

    /// The system colour bits of a colour typed as `&H8000000F&`, if it is
    /// one (so the Properties window and the program keep showing it).
    fn parse_system_color(val: &str) -> Option<u32> {
        let v = val.trim();
        let hex = v
            .strip_prefix("&H")
            .or_else(|| v.strip_prefix("&h"))?
            .trim_end_matches('&');
        let vb = i64::from_str_radix(hex, 16).ok()?;
        oxbasic_ui::host::system_bits(vb)
    }

    /// `True`/`False` (or 1/0) typed in the inspector.
    fn parse_bool(val: &str) -> Option<bool> {
        match val.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "-1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        }
    }

    /// Apply an inspector edit as it is typed. `Name` is not applied here
    /// (see [`Ide::commit_rename`]); numbers apply once they parse.
    fn apply_prop(&mut self, idx: usize, key: &str, val: &str) {
        if idx == PROJECT_IDX {
            // the package settings: text, saved with the project
            let v = val.trim().to_string();
            match key {
                "version" => self.workspace.version = v,
                "icon" => self.workspace.icon = v,
                "bundleid" => self.workspace.bundle_id = v,
                _ => {}
            }
            return;
        }
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
                "keypreview" => {
                    if let Some(b) = Self::parse_bool(val) {
                        self.form.key_preview = b;
                    }
                }
                _ => {}
            }
            return;
        }
        // TabIndex moves the control within the order, so it needs the form
        if key == "tabindex" {
            if let Ok(n) = val.trim().parse::<i32>() {
                if n >= 0 && (n as usize) < self.form.controls.len() {
                    self.form.set_tab_index(idx, n);
                }
            }
            return;
        }
        // Container: a Frame or PictureBox of the form (blank: the form);
        // taken only once it names one, and never the control itself or
        // what it contains
        if key == "container" {
            let name = val.trim();
            if name.is_empty() {
                self.form.controls[idx].container = None;
                return;
            }
            let (base, index) = oxbasic_ui::split_element(name);
            let target = self
                .form
                .controls
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(base) && c.index == index);
            if let Some(j) = target {
                let ok = self.form.controls[j].kind.is_container()
                    && j != idx
                    && !self.form.contains(idx, j);
                if ok {
                    self.form.controls[idx].container =
                        Some(oxbasic_ui::container_key(&self.form.controls[j]));
                }
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
            // CheckBox 0/1/2; an OptionButton is 0 or 1
            "value" => {
                let max = if c.kind == ControlKind::CheckBox {
                    2
                } else {
                    1
                };
                if let Ok(v) = val.trim().parse::<i32>() {
                    if (0..=max).contains(&v) {
                        c.value = v;
                    }
                }
            }
            // the array's Index: blank leaves the array
            "index" => {
                if val.trim().is_empty() {
                    c.index = None;
                } else if let Ok(n) = val.trim().parse::<i32>() {
                    if n >= 0 {
                        c.index = Some(n);
                    }
                }
            }
            "enabled" => {
                if let Some(b) = Self::parse_bool(val) {
                    c.enabled = b;
                }
            }
            "tabstop" => {
                if let Some(b) = Self::parse_bool(val) {
                    c.tab_stop = b;
                }
            }
            "locked" => {
                if let Some(b) = Self::parse_bool(val) {
                    c.locked = b;
                }
            }
            "multiline" => {
                if let Some(b) = Self::parse_bool(val) {
                    c.multi_line = b;
                }
            }
            "maxlength" => c.max_length = val.trim().parse().unwrap_or(c.max_length),
            "scrollbars" => {
                if let Ok(n) = val.trim().parse::<u8>() {
                    if n <= 3 {
                        c.scroll_bars = n;
                    }
                }
            }
            "multiselect" => {
                if let Ok(n) = val.trim().parse::<u8>() {
                    if n <= 2 {
                        c.multi_select = n;
                    }
                }
            }
            "picture" => c.picture = val.trim().to_string(),
            "stretch" => {
                if let Some(b) = Self::parse_bool(val) {
                    c.stretch = b;
                }
            }
            "autosize" => {
                if let Some(b) = Self::parse_bool(val) {
                    c.auto_size = b;
                }
            }
            "tooltiptext" => c.tool_tip = val.to_string(),
            "mouseicon" => c.mouse_icon = val.trim().to_string(),
            "mousepointer" => {
                if let Ok(n) = val.trim().parse::<i16>() {
                    if (0..=15).contains(&n) || n == 99 {
                        c.mouse_pointer = n;
                    }
                }
            }
            "backcolor" => {
                if let Some(col) = Self::parse_color(val) {
                    c.color = col;
                    c.back_system = Self::parse_system_color(val);
                }
            }
            "forecolor" => {
                if let Some(col) = Self::parse_color(val) {
                    c.fore_color = col;
                    c.fore_system = Self::parse_system_color(val);
                }
            }
            "fontname" => c.font.name = val.trim().to_string(),
            "fontsize" => {
                if let Ok(s) = val.trim().parse::<f32>() {
                    if (1.0..=2048.0).contains(&s) {
                        c.font.size = s;
                    }
                }
            }
            "fontbold" => c.font.bold = Self::parse_bool(val).unwrap_or(c.font.bold),
            "fontitalic" => c.font.italic = Self::parse_bool(val).unwrap_or(c.font.italic),
            "fontunderline" => c.font.underline = Self::parse_bool(val).unwrap_or(c.font.underline),
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
        // A name another control already has is a control array (both in
        // it, with distinct indexes, and the same kind); otherwise taken.
        let me = self.form.controls.get(idx);
        let taken = self.form.controls.iter().enumerate().any(|(i, c)| {
            i != idx
                && c.name.eq_ignore_ascii_case(new)
                && !me.is_some_and(|m| {
                    m.kind == c.kind && m.index.is_some() && c.index.is_some() && m.index != c.index
                })
        }) || (idx != FORM_IDX && self.form.name.eq_ignore_ascii_case(new));
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
            // The form's name is the project document's name: the Project
            // panel, the startup object and `Form2.Show` in code all use
            // it. Form *events* stay `Form_<Event>` whatever it is called.
            self.store_active();
            let active = self.workspace.active;
            if let Err(e) = self.workspace.rename(active, &new) {
                self.status = format!("cannot rename {}: {}", old, e);
                self.rename_error = Some((idx, e));
                return false;
            }
            self.form.name = new.clone();
            // other forms' code refers to this form by name (`Form2.Show`)
            let n = self.rename_form_references(&old, &new);
            self.status = if n == 0 {
                format!("renamed form {} to {}", old, new)
            } else {
                format!(
                    "renamed form {} to {} ({} reference(s) updated)",
                    old, new, n
                )
            };
            return true;
        }
        let kind = self.form.controls[idx].kind;
        self.form.controls[idx].name = new.clone();
        // The code follows the name unless other controls still carry the
        // old one (this control just left a control array, or joined one
        // whose handlers already exist).
        let old_still_used = self
            .form
            .controls
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&old));
        let new_shared = self
            .form
            .controls
            .iter()
            .enumerate()
            .any(|(i, c)| i != idx && c.name.eq_ignore_ascii_case(&new));
        let n = if old_still_used || new_shared {
            0
        } else {
            let events = events_for_control(kind.basic_name());
            code_model::rename_control(&mut self.lines, &old, &new, &events)
        };
        self.cursor.1 = self.cursor.1.min(self.line_len(self.cursor.0));
        // the rename changed the form too, so it isn't undone from the code
        // editor (that would split the two); undo history from before it
        // would restore the old names, so it is dropped
        if n > 0 {
            self.anchor = None;
            self.history.clear();
        }
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
        // choosing a control or the form leaves the project's settings
        self.project_selected = false;
    }

    /// A key in a project package field (Version, Icon, Bundle Id): a real
    /// single-line edit with a caret, a selection, the clipboard (⌘A ⌘C ⌘X
    /// ⌘V) and scrolling. Enter applies the value, Escape reverts it.
    fn project_field_key(&mut self, key: &'static str, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        use text_field::{Move, Sel};
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        let mut buf = self.prop_bufs.get(&(PROJECT_IDX, key)).cloned().unwrap_or_default();
        let len = buf.chars().count();
        let mut sel = self.prop_sel.get(key).copied().unwrap_or(Sel::at(len));
        // the text may have changed since the selection was kept
        sel = Sel { anchor: sel.anchor.min(len), caret: sel.caret.min(len) };
        let cmd = m.platform || m.control;
        match ks.key.as_str() {
            "enter" => {
                self.apply_prop(PROJECT_IDX, key, &buf);
                self.prop_bufs.insert((PROJECT_IDX, key), buf);
                self.prop_sel.remove(key);
                return;
            }
            "escape" => {
                let v = self.prop_value(PROJECT_IDX, key);
                self.prop_bufs.insert((PROJECT_IDX, key), v);
                self.prop_sel.remove(key);
                return;
            }
            "left" | "right" | "home" | "end" if !cmd => {
                let mv = match ks.key.as_str() {
                    "left" => Move::Left,
                    "right" => Move::Right,
                    "home" => Move::Home,
                    _ => Move::End,
                };
                sel = text_field::move_sel(&buf, sel, mv, m.shift);
            }
            "backspace" => sel = text_field::backspace_sel(&mut buf, sel),
            "delete" => sel = text_field::delete_sel(&mut buf, sel),
            "a" if cmd => sel = text_field::select_all(&buf),
            "c" if cmd => {
                let (a, b) = sel.range();
                let s: String = buf.chars().skip(a).take(b - a).collect();
                if !s.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(s));
                }
                return;
            }
            "x" if cmd => {
                let (a, b) = sel.range();
                let s: String = buf.chars().skip(a).take(b - a).collect();
                if !s.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(s));
                    sel = text_field::delete_selection(&mut buf, sel);
                }
            }
            "v" if cmd => {
                if let Some(paste) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    // a path or version is one line: line breaks are dropped
                    let one_line: String = paste.chars().filter(|c| *c != '\r' && *c != '\n').collect();
                    sel = text_field::insert_over(&mut buf, sel, &one_line);
                }
            }
            _ => {
                if cmd {
                    return;
                }
                match typed_text(ev) {
                    Some(s) => sel = text_field::insert_over(&mut buf, sel, &s),
                    None => return,
                }
            }
        }
        self.prop_bufs.insert((PROJECT_IDX, key), buf.clone());
        // the project takes the value as it is typed (Enter only confirms it)
        self.apply_prop(PROJECT_IDX, key, &buf);
        self.prop_sel.insert(key, sel);
        // the field scrolls so the caret stays in view (about 30 characters show)
        let scroll = self.prop_scroll.get(key).copied().unwrap_or(0);
        let scroll = text_field::scrolled_to_caret(scroll, sel.caret, 30);
        self.prop_scroll.insert(key, scroll);
    }



    /// One keystroke into inspector field `key` of `idx`.
    fn prop_key(&mut self, idx: usize, key: &'static str, ev: &KeyDownEvent, win: &mut Window, cx: &mut Context<Self>) {
        // the project's package fields have their own editor (caret, selection,
        // clipboard, scrolling); the form's and control's rows keep theirs
        if idx == PROJECT_IDX {
            self.project_field_key(key, ev, cx);
            return;
        }

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

    /// The Icon's ▸ button: the native file picker, and the file chosen becomes
    /// the project's icon (applied at once, as Enter would).
    fn choose_icon(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let chosen = match rx.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            if let Some(path) = chosen {
                let _ = this.update(cx, |ide, cx| {
                    let v = path.to_string_lossy().to_string();
                    ide.prop_bufs.insert((PROJECT_IDX, "icon"), v.clone());
                    ide.apply_prop(PROJECT_IDX, "icon", &v);
                    ide.prop_sel.remove("icon");
                    cx.notify();
                });
            }
        })
        .detach();
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
                    this.prop_key(idx, key, ev, win, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(SharedString::from(if idx == PROJECT_IDX {
                // a package field shows the text from its scroll on, so the
                // caret and the end of a long path stay in view
                let scroll = self.prop_scroll.get(key).copied().unwrap_or(0);
                text.chars().skip(scroll).collect::<String>()
            } else {
                text.clone()
            }));
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
                    // labels are a size smaller than the values; wide enough
                    // for the longest (ToolTipText, MousePointer, Font.Name)
                    .w(px(PROP_LABEL_W))
                    .flex_none()
                    .text_size(px(PROP_LABEL_SIZE))
                    .text_color(rgb(DIM))
                    .whitespace_nowrap()
                    .overflow_hidden()
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
                        .pl(px(PROP_LABEL_W + 8.0))
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
                ("keypreview", "KeyPreview"),
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
        if k.has_value() {
            rows.push(("value", "Value"));
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
        if k == ControlKind::TextBox {
            rows.extend([
                ("locked", "Locked"),
                ("maxlength", "MaxLength"),
                ("multiline", "MultiLine"),
                ("scrollbars", "ScrollBars"),
            ]);
        }
        if k == ControlKind::ComboBox {
            rows.push(("locked", "Locked"));
        }
        if k == ControlKind::ListBox {
            rows.push(("multiselect", "MultiSelect"));
        }
        if k.has_picture() {
            rows.push(("picture", "Picture"));
        }
        if k == ControlKind::Image {
            rows.push(("stretch", "Stretch"));
        }
        if k == ControlKind::PictureBox {
            rows.push(("autosize", "AutoSize"));
        }
        if k != ControlKind::Timer {
            rows.push(("container", "Container"));
            rows.push(("tooltiptext", "ToolTipText"));
            rows.push(("mousepointer", "MousePointer"));
            rows.push(("mouseicon", "MouseIcon"));
        }
        rows.push(("enabled", "Enabled"));
        rows.push(("index", "Index"));
        if k != ControlKind::Timer {
            rows.push(("tabindex", "TabIndex"));
        }
        if k.can_focus() {
            rows.push(("tabstop", "TabStop"));
        }
        if k != ControlKind::Timer {
            rows.push(("backcolor", "BackColor"));
        }
        if k.has_font() {
            rows.extend([
                ("forecolor", "ForeColor"),
                ("fontname", "Font.Name"),
                ("fontsize", "Font.Size"),
                ("fontbold", "Font.Bold"),
                ("fontitalic", "Font.Italic"),
                ("fontunderline", "Font.Underline"),
            ]);
        }
        rows
    }

    fn render_inspector(&mut self, win: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The project's package settings, when the project is selected in the
        // Project panel: the same fields as the form's, for the app's version,
        // icon and bundle id (saved with the project).
        if self.project_selected {
            let mut list = div().p_2().flex().flex_col().gap_1();
            for (key, label) in [
                ("version", "Version"),
                ("icon", "Icon"),
                ("bundleid", "Bundle Id"),
            ] {
                let row = self.prop_row(PROJECT_IDX, key, label, win, cx);
                if key == "icon" {
                    // the Icon's file picker, beside the field
                    let pick = div()
                        .id("project-icon-pick")
                        .debug_selector(|| "project-icon-pick".into())
                        .flex_none()
                        .px_2()
                        .h_6()
                        .flex()
                        .items_center()
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(BG))
                        .cursor_pointer()
                        .child("…")
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|this: &mut Ide, _ev: &MouseDownEvent, _win, cx| {
                                cx.stop_propagation();
                                this.choose_icon(cx);
                            }),
                        );
                    list = list.child(div().flex().items_center().gap_1().child(row).child(pick));
                } else {
                    list = list.child(row);
                }
            }
            return div()
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
                        .debug_selector(|| "project-settings".into())
                        .px_3()
                        .py_2()
                        .flex()
                        .items_center()
                        .bg(rgb(PANEL))
                        .border_b_1()
                        .border_color(rgb(BORDER))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(SharedString::from(self.workspace.name.clone())),
                )
                .child(list)
                .child(
                    div()
                        .px_3()
                        .pt_1()
                        .text_xs()
                        .text_color(rgb(DIM))
                        .child("Used by File > Package Project. Enter applies a value."),
                )
                .into_any_element();
        }
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
        // a form's menu is edited from the form's own Properties
        let menu_button = (idx == FORM_IDX).then(|| {
            div().px_2().pt_2().child(
                self.action_button(
                    "inspector-menu-editor",
                    "Menu Editor…",
                    move |this, cx| this.open_menu_editor(cx),
                    cx,
                )
                .border_1()
                .border_color(rgb(BORDER)),
            )
        });
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
            .children(menu_button)
            .into_any_element()
    }

    fn render_output(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let output_width = self
            .output
            .lines()
            .map(|line| line.chars().count() as f32 * 8.0)
            .fold(160.0_f32, f32::max);
        let mut panel = div()
            .flex_none()
            .h(px(self.panes.output_h))
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
                        .overflow_scroll()
                        .scrollbar_width(px(8.0))
                        .track_scroll(&self.output_scroll)
                        .font_family("Menlo")
                        .whitespace_nowrap()
                        // the padding is inside the scrolled content, so only
                        // text that is really too big for the window scrolls
                        .child(
                            div()
                                .flex_none()
                                .w(px(output_width + 16.0))
                                .p_2()
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
