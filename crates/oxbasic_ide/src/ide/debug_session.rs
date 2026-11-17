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
}

/// Events the running program sends to the IDE.
pub enum DebugEvent {
    Paused(PausedInfo),
    Output(String),
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
            let run_mode = matches!(cmd_rx.recv(), Ok(DebugCommand::Run));
            let first_pause = Rc::new(Cell::new(run_mode));
            let evt_tx_hook = evt_tx.clone();
            {
                let first_pause = first_pause.clone();
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
                        }
                    }
                }));
            }
            let result = rt.run();
            let msg = match result {
                Ok(()) => "Program finished".to_string(),
                Err(e) => e.to_string(),
            };
            let _ = evt_tx.send(DebugEvent::Finished(msg));
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
        match evt_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            DebugEvent::Paused(info) => assert_eq!(info.line, 1),
            other => panic!("expected pause, got {:?}", other.kind_name()),
        }
        session.send(DebugCommand::StepOver);
        // second pause: line 2 — with the updated variable
        match evt_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            DebugEvent::Paused(info) => {
                assert_eq!(info.line, 2);
                assert!(info.globals.iter().any(|(k, v)| k == "x" && v == "1"));
            }
            other => panic!("expected pause, got {:?}", other.kind_name()),
        }
        session.send(DebugCommand::Continue);
        // program terminates
        match evt_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
            DebugEvent::Finished(_) => {}
            other => panic!("expected finish, got {:?}", other.kind_name()),
        }
    }

    impl DebugEvent {
        fn kind_name(&self) -> &'static str {
            match self {
                DebugEvent::Paused(_) => "Paused",
                DebugEvent::Output(_) => "Output",
                DebugEvent::Finished(_) => "Finished",
                DebugEvent::Failed(_) => "Failed",
            }
        }
    }
}

