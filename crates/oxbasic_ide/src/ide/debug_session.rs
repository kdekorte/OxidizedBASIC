//! The debug session: runs the BASIC interpreter on its own thread (the
//! interpreter is single-threaded `Rc`-based) and communicates with the
//! IDE over channels — pause/step/continue commands in, output and
//! pause notifications out.

use oxbasic_runtime::{DebugAction, Interpreter, Output, PausedInfo};
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
    /// The user typed into or picked from a control of form `form`: the
    /// program's `Text1.Text` / `List1.ListIndex` must show it. Sent before
    /// the event it causes, so the handler already sees the new value.
    Edit(usize, UserEdit),
}

/// Events the running program sends to the IDE.
pub enum DebugEvent {
    Paused(PausedInfo),
    Output(String),
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
    /// Module-level code finished and the program stays alive to serve
    /// form events (clicks, timers).
    Live,
    Finished(String),
    Failed(String),
}

/// What a session runs: the program, the forms' layouts in project order,
/// and which form starts.
pub struct ProgramSpec {
    pub source: String,
    pub forms: Vec<FormSpec>,
    pub startup: usize,
    /// Handlers are named without a form prefix (`Command1_Click`, not
    /// `Form1__Command1_Click`): a single-form program that wasn't built
    /// from a project.
    pub bare: bool,
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
    /// alive to serve them).
    fn has_events(&self) -> bool {
        self.declared.iter().any(|n| {
            let l = n.to_lowercase();
            l.ends_with("_click")
                || l.ends_with("_load")
                || l.ends_with("_timer")
                || l.ends_with("_change")
        })
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

/// A live program under the IDE's control.
#[derive(Clone)]
pub struct DebugSession {
    cmd_tx: Sender<DebugCommand>,
}
impl DebugSession {
    /// A program with one form (or none): the old entry point, kept for
    /// callers that only have a layout and its code.
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
    // The program's forms; the interpreter reaches their controls through
    // it, and this thread keeps a handle to publish changes and to apply
    // what the user did in the windows. A program with no form still gets
    // one empty form, so its event handlers can be called.
    let forms = if spec.forms.is_empty() {
        vec![oxbasic_ui::FormSpec::new("Form1", "Form1", 0.0, 0.0)]
    } else {
        spec.forms.clone()
    };
    let host: Option<Host> = Some(Rc::new(RefCell::new(ProjectHost::new(forms))));
    if let Some(h) = &host {
        rt.set_object_host(h.clone());
    }

    // First command decides whether the initial break-on-start is skipped
    // (Run) or honored (Step).
    let cmd_rx = Rc::new(cmd_rx);
    let run_mode = matches!(cmd_rx.recv(), Ok(DebugCommand::Run));
    let first_pause = Rc::new(Cell::new(run_mode));
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
                    // events arriving while paused are dropped
                    Ok(DebugCommand::Call(..)) => continue,
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
    if spec.bare && !names.has_events() {
        let _ = evt_tx.send(DebugEvent::Finished("Program finished".to_string()));
        return;
    }
    let _ = evt_tx.send(DebugEvent::Live);
    let startup = spec.startup.min(h.borrow().form_count() - 1);
    h.borrow_mut().show(startup);
    let mut alive = flush_forms(&host, &mut rt, &names, &evt_tx);
    serve(&mut rt, &h, &host, &names, &evt_tx, &cmd_rx, &mut alive);
    let ended = rt.has_ended();
    let _ = evt_tx.send(DebugEvent::Finished(
        if ended { "Program ended" } else { "Stopped" }.to_string(),
    ));
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
    while *alive {
        // the program is over once its last form is unloaded
        if !h.borrow().any_loaded() {
            break;
        }
        match cmd_rx.recv() {
            Ok(DebugCommand::Call(form, handler, args)) => {
                let Some(form_name) = h.borrow().form_name(form).map(|s| s.to_string()) else {
                    continue;
                };
                // a form that was unloaded gets no events
                if !h.borrow().is_loaded(form) {
                    continue;
                }
                // handlers are optional: no handler leaves the control's
                // own action alone, but errors inside one are reported
                let Some(name) = names.find(&form_name, &handler) else {
                    continue;
                };
                h.borrow_mut().set_context_index(form);
                // an event's `Index As Integer`
                let args: Vec<oxbasic_runtime::Variant> = args
                    .iter()
                    .map(|&n| oxbasic_runtime::Variant::Integer(n as i16))
                    .collect();
                match rt.evaluate_sub(&name, &args) {
                    Ok(_) => {}
                    Err(e) if e.number == oxbasic_runtime::DEBUG_ABORT => *alive = false,
                    Err(e) => {
                        let _ =
                            tx.send(DebugEvent::Output(format!("error in {}: {}\n", handler, e)));
                    }
                }
                // what the handler did to the forms, up to an error
                *alive = flush_forms(host, rt, names, tx) && *alive;
            }
            Ok(DebugCommand::Edit(form, e)) => apply_edit(host, form, e),
            Ok(DebugCommand::Stop) | Err(_) => *alive = false,
            Ok(_) => {}
        }
    }
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
                DebugEvent::Live | DebugEvent::FormShown(_) => {}
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
                DebugEvent::FormShown(_) => {}
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
                | Ok(DebugEvent::FormClosed(_)) => {}
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
                DebugEvent::Form(_) => "Form",
                DebugEvent::FormShown(_) => "FormShown",
                DebugEvent::FormHidden(_) => "FormHidden",
                DebugEvent::FormClosed(_) => "FormClosed",
                DebugEvent::Live => "Live",
                DebugEvent::Finished(_) => "Finished",
                DebugEvent::Failed(_) => "Failed",
            }
        }
    }
}
