//! The debug session: runs the BASIC interpreter on its own thread (the
//! interpreter is single-threaded `Rc`-based) and communicates with the
//! IDE over channels — pause/step/continue commands in, output and
//! pause notifications out.

use oxbasic_runtime::{DebugAction, Interpreter, Output, PausedInfo};
use std::cell::Cell;
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
    /// Invoke a BASIC event handler (e.g. `Command1_Click`) on the live
    /// program.
    Call(String),
}

/// Events the running program sends to the IDE.
pub enum DebugEvent {
    Paused(PausedInfo),
    Output(String),
    /// Module-level code finished and the program stays alive to serve
    /// form events (clicks, timers).
    Live,
    Finished(String),
    Failed(String),
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

/// A live program under the IDE's control.
#[derive(Clone)]
pub struct DebugSession {
    cmd_tx: Sender<DebugCommand>,
}

impl DebugSession {
    /// Spawn the interpreter thread. Events flow to `evt_tx`; commands
    /// come back through the returned handle.
    pub fn start(
        src: String,
        breakpoints: Arc<Mutex<HashSet<usize>>>,
        evt_tx: Sender<DebugEvent>,
    ) -> DebugSession {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<DebugCommand>();
        std::thread::spawn(move || {
            let prog = match oxbasic_parser::parse(&src) {
                Ok(p) => p,
                Err(e) => {
                    let _ = evt_tx.send(DebugEvent::Failed(e));
                    return;
                }
            };
            let mut out = ChannelOutput { tx: evt_tx.clone() };
            let mut rt = Interpreter::new(&prog, &mut out);
            rt.set_breakpoints(breakpoints);

            // First command decides whether the initial break-on-start is
            // skipped (Run) or honored (Step).
            let cmd_rx = Rc::new(cmd_rx);
            let run_mode = matches!(cmd_rx.recv(), Ok(DebugCommand::Run));
            let first_pause = Rc::new(Cell::new(run_mode));
            let evt_tx_hook = evt_tx.clone();
            {
                let first_pause = first_pause.clone();
                let cmd_rx = cmd_rx.clone();
                rt.set_debug_hook(Box::new(move |info: &PausedInfo| {
                    if first_pause.get() {
                        first_pause.set(false);
                        return DebugAction::Continue; // skip break-on-start
                    }
                    let _ = evt_tx_hook.send(DebugEvent::Paused(info.clone()));
                    loop {
                        match cmd_rx.recv() {
                            Ok(DebugCommand::Continue) | Ok(DebugCommand::Run) => {
                                return DebugAction::Continue
                            }
                            Ok(DebugCommand::Step) | Ok(DebugCommand::StepOver) => {
                                return DebugAction::StepOver
                            }
                            Ok(DebugCommand::Stop) | Err(_) => return DebugAction::Abort,
                            // events arriving while paused are dropped
                            Ok(DebugCommand::Call(_)) => continue,
                        }
                    }
                }));
            }

            // 1. module-level code
            if let Err(e) = rt.run() {
                let _ = evt_tx.send(DebugEvent::Finished(e.to_string()));
                return;
            }

            // 2. if the program defines form event handlers, stay alive
            //    (VB semantics): fire Form_Load, then serve Call commands.
            let names: Vec<String> = rt.proc_names().iter().map(|n| n.to_lowercase()).collect();
            let has_events = names.iter().any(|n| {
                n.ends_with("_click")
                    || n.ends_with("_load")
                    || n.ends_with("_timer")
                    || n.ends_with("_change")
            });
            if !has_events {
                let _ = evt_tx.send(DebugEvent::Finished("Program finished".to_string()));
                return;
            }
            let _ = evt_tx.send(DebugEvent::Live);
            let call = |rt: &mut Interpreter, name: &str| -> bool {
                // UI handlers are optional. Leave the control's normal action
                // alone when no handler exists, but do not suppress errors
                // raised while executing a handler that *is* defined.
                if !names
                    .iter()
                    .any(|defined| defined.eq_ignore_ascii_case(name))
                {
                    return true;
                }
                match rt.evaluate_sub(name, &[]) {
                    Ok(_) => true,
                    Err(e) if e.number == oxbasic_runtime::DEBUG_ABORT => false,
                    Err(e) => {
                        let _ =
                            evt_tx.send(DebugEvent::Output(format!("error in {}: {}\n", name, e)));
                        true
                    }
                }
            };
            let mut alive = true;
            if names.iter().any(|n| n == "form_load") {
                alive = call(&mut rt, "Form_Load");
            }
            while alive {
                match cmd_rx.recv() {
                    Ok(DebugCommand::Call(name)) => alive = call(&mut rt, &name),
                    Ok(DebugCommand::Stop) | Err(_) => alive = false,
                    Ok(_) => {}
                }
            }
            let _ = evt_tx.send(DebugEvent::Finished("Stopped".to_string()));
        });
        DebugSession { cmd_tx }
    }

    pub fn send(&self, cmd: DebugCommand) {
        let _ = self.cmd_tx.send(cmd);
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
                DebugEvent::Live => {}
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
        session.send(DebugCommand::Call("Command1_Click".into()));
        session.send(DebugCommand::Call("Command1_Click".into()));
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
            session.send(DebugCommand::Call(name.into()));
        }
        // Handler lookup remains case-insensitive. An undefined ordinary call
        // *inside* a defined handler must still report runtime error 35.
        session.send(DebugCommand::Call("cOmMaNd1_cLiCk".into()));
        session.send(DebugCommand::Call("COMMAND2_CLICK".into()));
        session.send(DebugCommand::Stop);

        let mut output = String::new();
        loop {
            match evt_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
            {
                DebugEvent::Output(s) => output.push_str(&s),
                DebugEvent::Finished(_) => break,
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

    impl DebugEvent {
        fn kind_name(&self) -> &'static str {
            match self {
                DebugEvent::Paused(_) => "Paused",
                DebugEvent::Output(_) => "Output",
                DebugEvent::Live => "Live",
                DebugEvent::Finished(_) => "Finished",
                DebugEvent::Failed(_) => "Failed",
            }
        }
    }
}
