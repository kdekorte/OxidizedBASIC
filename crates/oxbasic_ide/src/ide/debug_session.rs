//! The debug session: runs the BASIC interpreter on its own thread (the
//! interpreter is single-threaded `Rc`-based) and communicates with the
//! IDE over channels — pause/step/continue commands in, output and
//! pause notifications out.

use oxbasic_runtime::{DebugAction, Interpreter, ObjectHost, Output, PausedInfo};
use oxbasic_ui::host::{FormSnapshot, UserEdit};
use oxbasic_ui::project_host::{handler_for, FormCommand, ProjectHost};
use oxbasic_ui::FormSpec;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// Commands the IDE sends to the running program.
#[derive(Debug)]
pub enum DebugCommand {
    /// Start executing without the initial break.
    Run,
    /// Start executing, breaking at the first statement.
    Step,
    /// Resume after a pause.
    Continue,
    /// Step over the current statement.
    StepOver,
    /// Abort the program.
    Stop,
    /// Invoke form `form`'s event handler (e.g. `Command1_Click`) on the
    /// live program, with its whole-number arguments (a control array
    /// element's `Index`). The interpreter's values can't cross threads,
    /// so they are made on the program's side.
    Call(usize, String, Vec<i32>),
    /// Raise an event of form `form` that the user caused, with VB's
    /// arguments for it: a mouse event's button, shift and position, a
    /// key's code. `control` is the control's index in the form (its
    /// handler gets the control array `Index` first), `None` for the form.
    Raise {
        form: usize,
        control: Option<usize>,
        event: String,
        args: Vec<EventArg>,
    },
    /// A key the user pressed in a control (or on the form): raise its
    /// `KeyDown`, `KeyPress` (when it types a character) — the form's
    /// first when its `KeyPreview` is on — and answer with
    /// [`DebugEvent::KeyResult`]: whether the key still goes to the control
    /// (`KeyCode` / `KeyAscii` left non-zero) and what it types.
    Key {
        form: usize,
        control: Option<usize>,
        key: KeyInput,
        /// Echoed in the answer, so the window matches it to the key.
        serial: u64,
    },
    /// The user released a key: `KeyUp` (the form's first with
    /// `KeyPreview`).
    KeyUp {
        form: usize,
        control: Option<usize>,
        code: i16,
        shift: i16,
    },
    /// The user typed into or picked from a control of form `form`: the
    /// program's `Text1.Text` / `List1.ListIndex` must show it. Sent before
    /// the event it causes, so the handler already sees the new value.
    Edit(usize, UserEdit),
    /// The user closed form `form`'s window: the form is unloaded, as if
    /// the program had said `Unload`. The program ends with its last form.
    Closed(usize),
    /// The user asked to close form `form`'s window (its close box): the
    /// form's `QueryUnload` and `Unload` run with `UnloadMode` 0 and may
    /// cancel. The program answers with [`DebugEvent::FormClosed`] when the
    /// form went, or [`DebugEvent::CloseRefused`] when it stays.
    Close(usize),
}

/// An event argument, made into a BASIC value on the program's side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventArg {
    Integer(i16),
    Single(f32),
}

impl EventArg {
    fn to_variant(self) -> oxbasic_runtime::Variant {
        match self {
            EventArg::Integer(n) => oxbasic_runtime::Variant::Integer(n),
            EventArg::Single(x) => oxbasic_runtime::Variant::Single(x),
        }
    }
}

/// A keystroke as the program's key events see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyInput {
    /// `KeyCode` (VB's code for the key), if VB has one.
    pub code: Option<i16>,
    /// `Shift` mask of the modifier keys.
    pub shift: i16,
    /// `KeyAscii` of the character it types, if it types one.
    pub ascii: Option<i16>,
}

/// Events the running program sends to the IDE.
pub enum DebugEvent {
    Paused(PausedInfo),
    Output(String),
    /// The program calls `OpenFileDialog` / `SaveFileDialog`: show the native
    /// picker and send the chosen path (or `None`) on `reply`.
    ChooseFile {
        filter: String,
        save: bool,
        reply: Sender<Option<String>>,
    },
    /// The program changed a form (`Label1.Caption = "x"`, `AddItem`...);
    /// `FormSnapshot::form_index` says which.
    Form(FormSnapshot),
    /// The program opened form `n` (`Form2.Show`, or the startup form):
    /// open its window, starting from this snapshot.
    FormShown(FormSnapshot),
    /// The program hid form `n`: close its window, keep its state.
    FormHidden(usize),
    /// The program unloaded form `n`: close its window.
    FormClosed(usize),
    /// The program refused to close form `n` (`Cancel` in its
    /// `QueryUnload` or `Unload`): its window stays.
    CloseRefused(usize),
    /// The outcome of a [`DebugCommand::Key`] with that `serial`: whether
    /// the control still gets the key (the handlers left `KeyCode` set),
    /// and the character it types (`KeyAscii` after `KeyPress`; `None`
    /// when it was set to 0 or the key types nothing).
    KeyResult {
        serial: u64,
        keep: bool,
        typed: Option<char>,
    },
    /// The events the program has handlers for, per form, as
    /// `(control name or "Form", event)` in lower case. The window sends
    /// frequent events (`MouseMove`) only when they have one.
    Handlers(Vec<HashSet<(String, String)>>),
    /// Module-level code finished and the program stays alive to serve
    /// form events (clicks, timers).
    Live,
    Finished(String),
    Failed(String),
}

/// What a session runs: the program, the forms' layouts in project order,
/// and which form starts.
#[derive(Default)]
pub struct ProgramSpec {
    pub source: String,
    pub forms: Vec<FormSpec>,
    pub startup: usize,
    /// Handlers are named without a form prefix (`Command1_Click`, not
    /// `Form1__Command1_Click`): a single-form program that wasn't built
    /// from a project.
    pub bare: bool,
    /// The project's folder: relative file names (`LoadPicture`) are in it.
    pub base_dir: Option<std::path::PathBuf>,
}

/// The program's procedure names, and how a form's handler is spelled in
/// them.
struct Names {
    declared: Vec<String>,
    bare: bool,
}

impl Names {
    /// The declared name of `form`'s `handler`, if the program has one.
    fn find(&self, form: &str, handler: &str) -> Option<String> {
        let want = if self.bare {
            handler.to_string()
        } else {
            handler_for(form, handler)
        };
        self.declared
            .iter()
            .find(|d| d.eq_ignore_ascii_case(&want))
            .cloned()
    }

    /// Whether the program has any event handler at all (so it should stay
    /// alive to serve them): a procedure named `<object>_<event>` for a
    /// control of a form (or `Form`) and an event VB raises for it.
    fn has_events(&self, forms: &[FormSpec]) -> bool {
        if forms.iter().any(|f| !self.handlers_of(f).is_empty()) {
            return true;
        }
        // a program run without its form still names its handlers
        // `<object>_<event>`: any VB event name keeps it alive
        let events: HashSet<String> = [
            "Form",
            "CommandButton",
            "Label",
            "TextBox",
            "Frame",
            "ListBox",
            "ComboBox",
            "PictureBox",
            "CheckBox",
            "OptionButton",
            "Timer",
        ]
        .iter()
        .flat_map(|k| oxbasic_ui::events_for_control(k))
        .map(|e| e.to_lowercase())
        .collect();
        self.declared.iter().any(|n| {
            n.to_lowercase()
                .rsplit_once('_')
                .is_some_and(|(_, e)| events.contains(e))
        })
    }

    /// The events of `form` the program handles: `(object, event)` in lower
    /// case, the object a control's name or `form`.
    fn handlers_of(&self, form: &FormSpec) -> HashSet<(String, String)> {
        let mut objects: Vec<(String, &str)> = vec![("form".into(), "Form")];
        for c in &form.controls {
            objects.push((c.name.to_lowercase(), c.kind.basic_name()));
        }
        let mut out = HashSet::new();
        for (object, kind) in objects {
            let label = if object == "form" { "Form" } else { kind };
            for event in oxbasic_ui::events_for_control(label) {
                let handler = format!("{}_{}", object, event);
                if self.find(&form.name, &handler).is_some() {
                    out.insert((object.clone(), event.to_lowercase()));
                }
            }
        }
        out
    }
}

/// Forwards interpreter output to the IDE as events.
struct ChannelOutput {
    tx: Sender<DebugEvent>,
}

impl Output for ChannelOutput {
    fn write(&mut self, s: &str) {
        let _ = self.tx.send(DebugEvent::Output(s.to_string()));
    }
    /// Ask the IDE (or the packaged app) for a file: the UI shows the native
    /// picker and answers on `reply`. The program waits for that answer.
    fn choose_file(&mut self, filter: &str, save: bool) -> Option<String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel::<Option<String>>();
        let _ = self.tx.send(DebugEvent::ChooseFile {
            filter: filter.to_string(),
            save,
            reply: reply_tx,
        });
        reply_rx.recv().ok().flatten()
    }
}

type Host = Rc<RefCell<ProjectHost>>;

/// Tell the IDE what the program changed on its forms since last time.
fn publish(host: &Option<Host>, tx: &Sender<DebugEvent>) {
    if let Some(h) = host {
        for snap in h.borrow_mut().take_changes() {
            let _ = tx.send(DebugEvent::Form(snap));
        }
    }
}

/// Hand a user edit to the program's copy of form `form`.
fn apply_edit(host: &Option<Host>, form: usize, edit: UserEdit) {
    if let Some(h) = host {
        h.borrow_mut().apply_edit(form, edit);
    }
}

/// The events the last user edit raised on the program's copy of the forms
/// (a scrollbar's `Change`), taken so they run once.
fn take_edit_raised(host: &Option<Host>) -> Vec<oxbasic_runtime::RaisedEvent> {
    match host {
        Some(h) => h.borrow_mut().take_raised(),
        None => Vec::new(),
    }
}

/// A live program under the IDE's control.
#[derive(Clone)]
pub struct DebugSession {
    cmd_tx: Sender<DebugCommand>,
}
impl DebugSession {
    /// A program with one form (or none), whose handlers are named bare
    /// (`Command1_Click`): the shape the tests use.
    #[cfg(test)]
    pub fn start_with_form(
        src: String,
        form: Option<FormSpec>,
        breakpoints: Arc<Mutex<HashSet<usize>>>,
        evt_tx: Sender<DebugEvent>,
    ) -> DebugSession {
        let spec = ProgramSpec {
            source: src,
            forms: form.into_iter().collect(),
            startup: 0,
            bare: true,
            base_dir: None,
        };
        DebugSession::start_program(spec, breakpoints, evt_tx)
    }

    /// A program with no form (a console program).
    #[cfg(test)]
    pub fn start(
        src: String,
        breakpoints: Arc<Mutex<HashSet<usize>>>,
        evt_tx: Sender<DebugEvent>,
    ) -> DebugSession {
        DebugSession::start_with_form(src, None, breakpoints, evt_tx)
    }

    /// Run a whole project's program on its own thread. Its forms'
    /// controls are objects the program reads and writes; what it does to
    /// them, and which forms it shows, hides or unloads, comes back as
    /// events.
    pub fn start_program(
        spec: ProgramSpec,
        breakpoints: Arc<Mutex<HashSet<usize>>>,
        evt_tx: Sender<DebugEvent>,
    ) -> DebugSession {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<DebugCommand>();
        std::thread::spawn(move || run_program(spec, breakpoints, evt_tx, cmd_rx));
        DebugSession { cmd_tx }
    }

    pub fn send(&self, cmd: DebugCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}

/// Tell the IDE which windows to open, hide or close, then what changed.
/// `Load` runs the form's `Form_Load` handler first (it may change the
/// form before it is first shown).
fn flush_forms(
    host: &Option<Host>,
    rt: &mut Interpreter,
    names: &Names,
    tx: &Sender<DebugEvent>,
) -> bool {
    let Some(h) = host else { return true };
    let mut alive = true;
    loop {
        let commands = h.borrow_mut().take_commands();
        if commands.is_empty() {
            break;
        }
        for cmd in commands {
            match cmd {
                FormCommand::Load(i) => {
                    let name = h.borrow().form_name(i).unwrap_or_default().to_string();
                    h.borrow_mut().set_context_index(i);
                    if let Some(handler) = names.find(&name, "Form_Load") {
                        match rt.evaluate_sub(&handler, &[]) {
                            Ok(_) => {}
                            Err(e) if e.number == oxbasic_runtime::DEBUG_ABORT => alive = false,
                            Err(e) => {
                                let _ = tx.send(DebugEvent::Output(format!(
                                    "error in {}: {}\n",
                                    handler, e
                                )));
                            }
                        }
                    }
                }
                FormCommand::Show(i) => {
                    // changes made before the window opens go in its first view
                    publish(host, tx);
                    if let Some(snap) = h.borrow().snapshot(i) {
                        let _ = tx.send(DebugEvent::FormShown(snap));
                    }
                }
                FormCommand::Hide(i) => {
                    let _ = tx.send(DebugEvent::FormHidden(i));
                }
                FormCommand::Unload(i) => {
                    let _ = tx.send(DebugEvent::FormClosed(i));
                }
            }
        }
    }
    publish(host, tx);
    alive && !rt.has_ended()
}

/// The program's thread: parse, run module-level code, open the startup
/// form, then serve what the windows send until the program ends.
fn run_program(
    spec: ProgramSpec,
    breakpoints: Arc<Mutex<HashSet<usize>>>,
    evt_tx: Sender<DebugEvent>,
    cmd_rx: std::sync::mpsc::Receiver<DebugCommand>,
) {
    let prog = match oxbasic_parser::parse(&spec.source) {
        Ok(p) => p,
        Err(e) => {
            let _ = evt_tx.send(DebugEvent::Failed(e));
            return;
        }
    };
    let mut out = ChannelOutput { tx: evt_tx.clone() };
    let mut rt = Interpreter::new(&prog, &mut out);
    rt.set_breakpoints(breakpoints);
    rt.base_dir = spec.base_dir.clone();
    // The program's forms; the interpreter reaches their controls through
    // it, and this thread keeps a handle to publish changes and to apply
    // what the user did in the windows. A program with no form still gets
    // one empty form, so its event handlers can be called.
    let forms = if spec.forms.is_empty() {
        vec![oxbasic_ui::FormSpec::new("Form1", "Form1", 0.0, 0.0)]
    } else {
        spec.forms.clone()
    };
    let mut project = ProjectHost::new(forms);
    project.set_base_dir(spec.base_dir.clone());
    let host: Option<Host> = Some(Rc::new(RefCell::new(project)));
    if let Some(h) = &host {
        rt.set_object_host(h.clone());
    }

    // First command decides whether the initial break-on-start is skipped
    // (Run) or honored (Step).
    let cmd_rx = Rc::new(cmd_rx);
    let run_mode = matches!(cmd_rx.recv(), Ok(DebugCommand::Run));
    let first_pause = Rc::new(Cell::new(run_mode));
    // `DoEvents`: show what the program has drawn so far, apply what the
    // user typed meanwhile, and stop if asked to. Clicks and the like that
    // arrive while a handler is busy are dropped, as a busy VB program
    // can't take them in a loop of its own.
    {
        let (cmd_rx, host, tx) = (cmd_rx.clone(), host.clone(), evt_tx.clone());
        rt.set_doevents_hook(Box::new(move || {
            publish(&host, &tx);
            loop {
                match cmd_rx.try_recv() {
                    Ok(DebugCommand::Stop) => return false,
                    Ok(DebugCommand::Edit(form, e)) => apply_edit(&host, form, e),
                    // closing the window while the program is busy ends it
                    // (a draw can take a while; Stop does the same)
                    Ok(DebugCommand::Close(_)) => return false,
                    // a key waits for an answer: the control just gets it
                    Ok(DebugCommand::Key { serial, key, .. }) => {
                        let _ = tx.send(DebugEvent::KeyResult {
                            serial,
                            keep: true,
                            typed: key.ascii.and_then(oxbasic_ui::events::typed_char),
                        });
                    }
                    Ok(_) => {}
                    Err(std::sync::mpsc::TryRecvError::Empty) => return true,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => return false,
                }
            }
        }));
    }
    {
        let (first_pause, cmd_rx, host, tx) = (
            first_pause.clone(),
            cmd_rx.clone(),
            host.clone(),
            evt_tx.clone(),
        );
        rt.set_debug_hook(Box::new(move |info: &PausedInfo| {
            if first_pause.get() {
                first_pause.set(false);
                return DebugAction::Continue; // skip break-on-start
            }
            // show what the program did to the forms up to here
            publish(&host, &tx);
            let _ = tx.send(DebugEvent::Paused(info.clone()));
            loop {
                match cmd_rx.recv() {
                    Ok(DebugCommand::Continue) | Ok(DebugCommand::Run) => {
                        return DebugAction::Continue
                    }
                    Ok(DebugCommand::Step) | Ok(DebugCommand::StepOver) => {
                        return DebugAction::StepOver
                    }
                    Ok(DebugCommand::Stop) | Err(_) => return DebugAction::Abort,
                    // typing while paused still reaches the form
                    Ok(DebugCommand::Edit(form, e)) => apply_edit(&host, form, e),
                    // a key waits for the program's answer: while paused
                    // its handlers can't run, so the control just gets it
                    Ok(DebugCommand::Key { serial, key, .. }) => {
                        let _ = tx.send(DebugEvent::KeyResult {
                            serial,
                            keep: true,
                            typed: key.ascii.and_then(oxbasic_ui::events::typed_char),
                        });
                    }
                    // the window stays: a paused program can't be asked
                    Ok(DebugCommand::Close(form)) => {
                        let _ = tx.send(DebugEvent::CloseRefused(form));
                    }
                    // other events arriving while paused are dropped
                    Ok(DebugCommand::Call(..))
                    | Ok(DebugCommand::Raise { .. })
                    | Ok(DebugCommand::KeyUp { .. })
                    | Ok(DebugCommand::Closed(_)) => continue,
                }
            }
        }));
    }

    // 1. module-level code
    let result = rt.run();
    publish(&host, &evt_tx);
    if let Err(e) = result {
        let _ = evt_tx.send(DebugEvent::Finished(e.to_string()));
        return;
    }
    if rt.has_ended() {
        let _ = evt_tx.send(DebugEvent::Finished("Program ended".to_string()));
        return;
    }

    // 2. a program with forms stays alive to serve their events (VB
    //    semantics): load and show the startup form, then serve commands
    let names = Names {
        declared: rt.proc_names(),
        bare: spec.bare,
    };
    let Some(h) = host.clone() else {
        let _ = evt_tx.send(DebugEvent::Finished("Program finished".to_string()));
        return;
    };
    let forms_now: Vec<FormSpec> = {
        let hb = h.borrow();
        (0..hb.form_count())
            .filter_map(|i| hb.snapshot(i).map(|s| s.form))
            .collect()
    };
    if spec.bare && !names.has_events(&forms_now) {
        let _ = evt_tx.send(DebugEvent::Finished("Program finished".to_string()));
        return;
    }
    let _ = evt_tx.send(DebugEvent::Live);
    let _ = evt_tx.send(DebugEvent::Handlers(
        forms_now.iter().map(|f| names.handlers_of(f)).collect(),
    ));
    let startup = spec.startup.min(h.borrow().form_count() - 1);
    h.borrow_mut().show(startup);
    let mut alive = flush_forms(&host, &mut rt, &names, &evt_tx);
    serve(&mut rt, &h, &host, &names, &evt_tx, &cmd_rx, &mut alive);
    let ended = rt.has_ended();
    let _ = evt_tx.send(DebugEvent::Finished(
        if ended { "Program ended" } else { "Stopped" }.to_string(),
    ));
}

/// What the serve loop needs at hand to run a window's events.
struct Server<'a, 'out> {
    rt: &'a mut Interpreter<'out>,
    h: &'a Host,
    host: &'a Option<Host>,
    names: &'a Names,
    tx: &'a Sender<DebugEvent>,
    alive: bool,
}

impl Server<'_, '_> {
    /// Run `object`'s `event` handler of form `form` with `args` (a control
    /// array element's `Index` first), if the program has one; returns the
    /// arguments as the handler left them. Errors in it are reported, and
    /// what it did to the forms is published.
    fn raise(
        &mut self,
        form: usize,
        control: Option<usize>,
        event: &str,
        args: Vec<oxbasic_runtime::Variant>,
    ) -> Vec<oxbasic_runtime::Variant> {
        let (object, index) = {
            let h = self.h.borrow();
            match control.and_then(|i| h.control_spec(form, i)) {
                Some(c) => (c.name.clone(), c.index),
                None if control.is_some() => return args,
                None => ("Form".to_string(), None),
            }
        };
        let handler = format!("{}_{}", object, event);
        let mut full: Vec<oxbasic_runtime::Variant> = index
            .map(|n| vec![oxbasic_runtime::Variant::Integer(n as i16)])
            .unwrap_or_default();
        let offset = full.len();
        full.extend(args.iter().cloned());
        match self.call_named(form, &handler, full) {
            Some(out) => out[offset..].to_vec(),
            None => args,
        }
    }

    /// Run form `form`'s handler `handler` (`Command1_Click`) with `args`
    /// if the program has it; returns the arguments as it left them, or
    /// `None` when it has no such handler or the handler failed. A form
    /// that was unloaded gets no events.
    fn call_named(
        &mut self,
        form: usize,
        handler: &str,
        args: Vec<oxbasic_runtime::Variant>,
    ) -> Option<Vec<oxbasic_runtime::Variant>> {
        let form_name = self.h.borrow().form_name(form)?.to_string();
        if !self.h.borrow().is_loaded(form) {
            return None;
        }
        let name = self.names.find(&form_name, handler)?;
        self.h.borrow_mut().set_context_index(form);
        let result = self
            .rt
            .call_event(&name, &args)
            .and_then(|out| self.rt.run_raised_events().map(|_| out));
        let out = match result {
            Ok(out) => Some(out),
            Err(e) if e.number == oxbasic_runtime::DEBUG_ABORT => {
                self.alive = false;
                None
            }
            Err(e) => {
                let _ = self
                    .tx
                    .send(DebugEvent::Output(format!("error in {}: {}\n", handler, e)));
                None
            }
        };
        // what the handler did to the forms, up to an error
        self.alive = flush_forms(self.host, self.rt, self.names, self.tx) && self.alive;
        out
    }

    /// A key pressed: the form's `KeyDown` (with `KeyPreview`) then the
    /// control's, then `KeyPress` the same way when it types a character.
    /// As in VB, a handler setting `KeyCode` to 0 skips the handlers after
    /// it and cancels a key that types nothing (an arrow, Delete), but not
    /// `KeyPress`: only `KeyAscii` = 0 swallows a character, and another
    /// `KeyAscii` types that character instead.
    fn key(&mut self, form: usize, control: Option<usize>, key: KeyInput) -> (bool, Option<char>) {
        use oxbasic_runtime::Variant;
        let preview = control.is_some() && self.h.borrow().key_preview(form);
        let order: Vec<Option<usize>> = if preview {
            vec![None, control]
        } else {
            vec![control]
        };
        let num = |v: &Variant| v.as_i64().unwrap_or(0) as i16;
        let mut code_kept = true;
        if let Some(code) = key.code {
            let mut code = code;
            for &target in &order {
                let out = self.raise(
                    form,
                    target,
                    "KeyDown",
                    oxbasic_ui::events::key_args(code, key.shift),
                );
                code = num(&out[0]);
                if code == 0 {
                    code_kept = false;
                    break;
                }
            }
        }
        let Some(ascii) = key.ascii else {
            return (code_kept, None);
        };
        let mut ascii = ascii;
        for &target in &order {
            let out = self.raise(form, target, "KeyPress", vec![Variant::Integer(ascii)]);
            ascii = num(&out[0]);
            if ascii == 0 {
                break;
            }
        }
        (ascii != 0, oxbasic_ui::events::typed_char(ascii))
    }

    /// The user asked to close form `form`: unless its handlers cancel, it
    /// is unloaded. The program ends with its last form.
    fn close(&mut self, form: usize) {
        let name = match self.h.borrow().form_name(form) {
            Some(n) if self.h.borrow().is_loaded(form) => n.to_string(),
            _ => return,
        };
        self.h.borrow_mut().set_context_index(form);
        let go = match self.rt.confirm_unload(&name, 0) {
            Ok(go) => go,
            Err(e) if e.number == oxbasic_runtime::DEBUG_ABORT => {
                self.alive = false;
                return;
            }
            Err(e) => {
                let _ = self
                    .tx
                    .send(DebugEvent::Output(format!("error in Form_Unload: {}\n", e)));
                true
            }
        };
        if go {
            self.h.borrow_mut().unload(form);
        } else {
            let _ = self.tx.send(DebugEvent::CloseRefused(form));
        }
        self.alive = flush_forms(self.host, self.rt, self.names, self.tx) && self.alive;
    }
}

/// Answer the windows' events until the program ends, is stopped, or its
/// last form is unloaded.
fn serve(
    rt: &mut Interpreter,
    h: &Host,
    host: &Option<Host>,
    names: &Names,
    tx: &Sender<DebugEvent>,
    cmd_rx: &std::sync::mpsc::Receiver<DebugCommand>,
    alive: &mut bool,
) {
    let mut s = Server {
        rt,
        h,
        host,
        names,
        tx,
        alive: *alive,
    };
    while s.alive {
        // the program is over once its last form is unloaded
        if !h.borrow().any_loaded() {
            break;
        }
        match cmd_rx.recv() {
            Ok(DebugCommand::Call(form, handler, args)) => {
                // a handler named in full (`Command1_Click`, a timer's
                // `Timer1_Timer`) with its whole-number arguments
                let args = args
                    .iter()
                    .map(|&n| oxbasic_runtime::Variant::Integer(n as i16))
                    .collect();
                s.call_named(form, &handler, args);
            }
            Ok(DebugCommand::Raise {
                form,
                control,
                event,
                args,
            }) => {
                let args = args.into_iter().map(EventArg::to_variant).collect();
                s.raise(form, control, &event, args);
            }
            Ok(DebugCommand::Key {
                form,
                control,
                key,
                serial,
            }) => {
                let (keep, typed) = s.key(form, control, key);
                let _ = tx.send(DebugEvent::KeyResult {
                    serial,
                    keep,
                    typed,
                });
            }
            Ok(DebugCommand::KeyUp {
                form,
                control,
                code,
                shift,
            }) => {
                let preview = control.is_some() && h.borrow().key_preview(form);
                let mut code = code;
                for target in [None, control].into_iter().skip(!preview as usize) {
                    let out = s.raise(
                        form,
                        target,
                        "KeyUp",
                        oxbasic_ui::events::key_args(code, shift),
                    );
                    code = out[0].as_i64().unwrap_or(0) as i16;
                    if code == 0 {
                        break;
                    }
                }
            }
            Ok(DebugCommand::Edit(form, e)) => {
                // the control the edit is for, so its event is raised on it
                let control = match &e {
                    UserEdit::Scroll { control, .. }
                    | UserEdit::Value { control, .. }
                    | UserEdit::Tab { control, .. } => Some(*control),
                    _ => None,
                };
                apply_edit(host, form, e);
                // a user's edit can raise events (a scrollbar's Change):
                // run their handlers now, as a key's are run
                for ev in take_edit_raised(host) {
                    let event = ev.handler.rsplit('_').next().unwrap_or("").to_string();
                    let _ = s.raise(form, control, &event, ev.args);
                }
            }
            Ok(DebugCommand::Close(form)) => s.close(form),
            // the window is already gone: the form is unloaded without
            // asking (its handlers can't keep a window that isn't there)
            Ok(DebugCommand::Closed(form)) => {
                h.borrow_mut().unload(form);
                let _ = h.borrow_mut().take_commands();
            }
            Ok(DebugCommand::Stop) | Err(_) => s.alive = false,
            Ok(_) => {}
        }
    }
    *alive = s.alive;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn live_session_serves_click_events() {
        let (evt_tx, evt_rx) = channel();
        let bps = Arc::new(Mutex::new(HashSet::new()));
        let src = "Dim Clicks As Long\n\
                   Sub Form_Load()\n  MsgBox \"loaded\"\nEnd Sub\n\
                   Sub Command1_Click()\n  Clicks = Clicks + 1\n  MsgBox \"clicks=\" & CStr(Clicks)\nEnd Sub\n"
            .to_string();
        let session = DebugSession::start(src, bps, evt_tx);
        session.send(DebugCommand::Run);

        let recv = || {
            evt_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
        };
        let mut out = String::new();
        // Live announcement, then Form_Load output
        loop {
            match recv() {
                DebugEvent::Live | DebugEvent::FormShown(_) | DebugEvent::Handlers(_) => {}
                DebugEvent::Output(s) => {
                    out.push_str(&s);
                    if out.contains("loaded") {
                        break;
                    }
                }
                other => panic!("unexpected {}", other.kind_name()),
            }
        }
        // two clicks mutate persistent module state
        session.send(DebugCommand::Call(0, "Command1_Click".into(), vec![]));
        session.send(DebugCommand::Call(0, "Command1_Click".into(), vec![]));
        while !out.contains("clicks=2") {
            if let DebugEvent::Output(s) = recv() {
                out.push_str(&s);
            }
        }
        assert!(out.contains("clicks=1"));
        session.send(DebugCommand::Stop);
        loop {
            if let DebugEvent::Finished(_) = recv() {
                break;
            }
        }
    }

    #[test]
    fn optional_event_handlers_are_silent_but_handler_errors_are_reported() {
        let (evt_tx, evt_rx) = channel();
        let src = "Sub Command1_Click()\n  Call MissingRoutine()\nEnd Sub\n\
                   Sub Command2_Click()\n  Print \"still alive\"\nEnd Sub\n";
        let session = DebugSession::start(
            src.to_string(),
            Arc::new(Mutex::new(HashSet::new())),
            evt_tx,
        );
        session.send(DebugCommand::Run);
        assert!(matches!(
            evt_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            DebugEvent::Live
        ));

        // No Form_Load, Text1_Click, Label2_Click, or Timer1_Timer is defined.
        // Generated events must not emit errors or terminate the session.
        for name in ["Form_Load", "Text1_Click", "Label2_Click", "Timer1_Timer"] {
            session.send(DebugCommand::Call(0, name.into(), vec![]));
        }
        // Handler lookup remains case-insensitive. An undefined ordinary call
        // *inside* a defined handler must still report runtime error 35.
        session.send(DebugCommand::Call(0, "cOmMaNd1_cLiCk".into(), vec![]));
        session.send(DebugCommand::Call(0, "COMMAND2_CLICK".into(), vec![]));
        session.send(DebugCommand::Stop);

        let mut output = String::new();
        loop {
            match evt_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
            {
                DebugEvent::Output(s) => output.push_str(&s),
                DebugEvent::Finished(_) => break,
                DebugEvent::FormShown(_) | DebugEvent::Handlers(_) => {}
                other => panic!("unexpected {}", other.kind_name()),
            }
        }
        assert_eq!(output.matches("error in ").count(), 1, "{}", output);
        assert!(output.contains("runtime error 35"), "{}", output);
        assert!(
            output.to_lowercase().contains("missingroutine"),
            "{}",
            output
        );
        assert!(output.contains("still alive"), "{}", output);
        for name in ["Form_Load", "Text1_Click", "Label2_Click", "Timer1_Timer"] {
            assert!(!output.contains(name), "{}", output);
        }
    }

    #[test]
    fn run_session_streams_output_and_finishes() {
        let (evt_tx, evt_rx) = channel();
        let bps = Arc::new(Mutex::new(HashSet::new()));
        let src = "For i = 1 To 3\n  Print i;\nNext\nPrint \"\"\n".to_string();
        let session = DebugSession::start(src, bps, evt_tx);
        session.send(DebugCommand::Run);

        let mut output = String::new();
        let mut finished = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !finished && std::time::Instant::now() < deadline {
            match evt_rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(DebugEvent::Output(s)) => output.push_str(&s),
                Ok(DebugEvent::Finished(_)) => finished = true,
                Ok(DebugEvent::Failed(e)) => panic!("parse failed: {}", e),
                Ok(DebugEvent::Paused(_)) => session.send(DebugCommand::Continue),
                Ok(DebugEvent::Live) => {}
                Ok(DebugEvent::Form(_))
                | Ok(DebugEvent::FormShown(_))
                | Ok(DebugEvent::FormHidden(_))
                | Ok(DebugEvent::FormClosed(_))
                | Ok(DebugEvent::CloseRefused(_))
                | Ok(DebugEvent::KeyResult { .. })
                | Ok(DebugEvent::Handlers(_))
                | Ok(DebugEvent::ChooseFile { .. }) => {}
                Err(_) => {}
            }
        }
        assert!(finished, "session did not finish");
        assert_eq!(output, " 1  2  3 \n");
    }

    #[test]
    fn step_session_pauses_at_each_statement() {
        let (evt_tx, evt_rx) = channel();
        let bps = Arc::new(Mutex::new(HashSet::new()));
        let src = "x = 1\nx = 2\n".to_string();
        let session = DebugSession::start(src, bps, evt_tx);
        session.send(DebugCommand::Step);

        // first pause: line 1
        match evt_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        {
            DebugEvent::Paused(info) => assert_eq!(info.line, 1),
            other => panic!("expected pause, got {:?}", other.kind_name()),
        }
        session.send(DebugCommand::StepOver);
        // second pause: line 2 — with the updated variable
        match evt_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        {
            DebugEvent::Paused(info) => {
                assert_eq!(info.line, 2);
                assert!(info.globals.iter().any(|(k, v)| k == "x" && v == "1"));
            }
            other => panic!("expected pause, got {:?}", other.kind_name()),
        }
        session.send(DebugCommand::Continue);
        // program terminates
        match evt_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
        {
            DebugEvent::Finished(_) => {}
            other => panic!("expected finish, got {:?}", other.kind_name()),
        }
    }

    /// The next `DebugEvent::Form`, skipping everything else; also gives
    /// back the program's output seen on the way.
    fn next_form(
        rx: &std::sync::mpsc::Receiver<DebugEvent>,
        out: &mut String,
    ) -> oxbasic_ui::host::FormSnapshot {
        loop {
            match rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("a form change")
            {
                DebugEvent::Form(snap) => return snap,
                DebugEvent::Output(s) => out.push_str(&s),
                DebugEvent::Failed(e) => panic!("{}", e),
                _ => {}
            }
        }
    }

    #[test]
    fn program_writes_to_the_form_and_reads_what_the_user_typed() {
        use oxbasic_ui::host::UserEdit;
        let (evt_tx, evt_rx) = channel();
        let mut form = FormSpec::new("Form1", "Counter", 300.0, 200.0);
        form.label(
            "Label1",
            "Clicks: 0",
            oxbasic_ui::Rect::new(0.0, 0.0, 100.0, 20.0),
        );
        form.text_box("Text1", "", oxbasic_ui::Rect::new(0.0, 30.0, 100.0, 24.0));
        let src = "Dim Clicks As Long\n\
                   Sub Form_Load()\n  Me.Caption = \"Loaded\"\nEnd Sub\n\
                   Sub Command1_Click()\n\
                     Clicks = Clicks + 1\n\
                     Label1.Caption = \"Clicks: \" & CStr(Clicks) & \" \" & Text1.Text\n\
                   End Sub\n";
        let session = DebugSession::start_with_form(
            src.to_string(),
            Some(form),
            Arc::new(Mutex::new(HashSet::new())),
            evt_tx,
        );
        session.send(DebugCommand::Run);
        let mut out = String::new();

        // Form_Load changed the caption
        let snap = next_form(&evt_rx, &mut out);
        assert_eq!(snap.form.caption, "Loaded");

        // the user types into Text1, then clicks: the handler sees the text
        session.send(DebugCommand::Edit(
            0,
            UserEdit::Text {
                control: 1,
                text: "abc".into(),
            },
        ));
        session.send(DebugCommand::Call(0, "Command1_Click".into(), vec![]));
        let snap = next_form(&evt_rx, &mut out);
        assert_eq!(snap.form.controls[0].caption, "Clicks: 1 abc");
        assert!(snap.touched[0], "Label1 was written");
        assert!(!snap.touched[1], "Text1 was only read");

        session.send(DebugCommand::Stop);
        assert_eq!(out, "", "no errors reported: {}", out);
    }

    #[test]
    fn a_form_error_in_a_handler_is_reported_and_the_session_lives() {
        let (evt_tx, evt_rx) = channel();
        let mut form = FormSpec::new("Form1", "F", 300.0, 200.0);
        form.label("Label1", "x", oxbasic_ui::Rect::new(0.0, 0.0, 100.0, 20.0));
        let src = "Sub Command1_Click()\n  Label1.Caption = \"ok\"\n  Label1.Nope = 1\nEnd Sub\n\
                   Sub Command2_Click()\n  Label1.Caption = \"still alive\"\nEnd Sub\n";
        let session = DebugSession::start_with_form(
            src.to_string(),
            Some(form),
            Arc::new(Mutex::new(HashSet::new())),
            evt_tx,
        );
        session.send(DebugCommand::Run);
        let mut out = String::new();
        session.send(DebugCommand::Call(0, "Command1_Click".into(), vec![]));
        // the write before the error is still shown
        assert_eq!(next_form(&evt_rx, &mut out).form.controls[0].caption, "ok");
        session.send(DebugCommand::Call(0, "Command2_Click".into(), vec![]));
        assert_eq!(
            next_form(&evt_rx, &mut out).form.controls[0].caption,
            "still alive"
        );
        assert!(out.contains("error in Command1_Click"), "{}", out);
        assert!(out.contains("438"), "{}", out);
        session.send(DebugCommand::Stop);
    }

    impl DebugEvent {
        fn kind_name(&self) -> &'static str {
            match self {
                DebugEvent::Paused(_) => "Paused",
                DebugEvent::Output(_) => "Output",
                DebugEvent::ChooseFile { .. } => "ChooseFile",
                DebugEvent::Form(_) => "Form",
                DebugEvent::FormShown(_) => "FormShown",
                DebugEvent::FormHidden(_) => "FormHidden",
                DebugEvent::FormClosed(_) => "FormClosed",
                DebugEvent::CloseRefused(_) => "CloseRefused",
                DebugEvent::KeyResult { .. } => "KeyResult",
                DebugEvent::Handlers(_) => "Handlers",
                DebugEvent::Live => "Live",
                DebugEvent::Finished(_) => "Finished",
                DebugEvent::Failed(_) => "Failed",
            }
        }
    }
}
