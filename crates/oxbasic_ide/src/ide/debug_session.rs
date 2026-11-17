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
