//! Launching the project's program: combining every form's and module's
//! code, translating breakpoints to the combined program's line numbers,
//! and mapping a pause back to the document and line it is in.

use super::debug_session::ProgramSpec;
use super::program::{combine, Origin, Program};
use super::workspace::DocKind;
use super::*;

impl Ide {
    /// The program for the open project, built from the working copy: every
    /// form's code (its handlers prefixed with the form's name) and every
    /// module's.
    pub(super) fn build_program(&mut self) -> (ProgramSpec, Program) {
        self.store_active();
        let program = combine(&self.workspace.docs);
        let forms: Vec<FormSpec> = self
            .workspace
            .docs
            .iter()
            .filter(|d| d.kind == DocKind::Form)
            .map(|d| d.form.clone())
            .collect();
        // the project's forms are the form documents, in order; the startup
        // form is found among them by name
        let startup = self
            .workspace
            .docs
            .iter()
            .filter(|d| d.kind == DocKind::Form)
            .position(|d| d.name.eq_ignore_ascii_case(&self.workspace.startup))
            .unwrap_or(0);
        let spec = ProgramSpec {
            source: program.source.clone(),
            forms,
            startup,
            bare: false,
            base_dir: self
                .workspace
                .file
                .as_deref()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf()),
        };
        (spec, program)
    }

    /// The breakpoints of every document as lines of the combined program.
    fn combined_breakpoints(&self, program: &Program) -> HashSet<usize> {
        let mut out = HashSet::new();
        for (i, doc) in self.workspace.docs.iter().enumerate() {
            for line in &doc.breakpoints {
                if let Some(n) = program.combined_line(i, *line) {
                    out.insert(n);
                }
            }
        }
        out
    }

    /// Start the program, running or stepping from the first statement.
    pub(super) fn launch(&mut self, step: bool) {
        let (spec, program) = self.build_program();
        // the debugger reads this set while the program runs, so it holds
        // the *combined* line numbers for the whole run
        let combined = Arc::new(Mutex::new(self.combined_breakpoints(&program)));
        self.run_lines = Some(program);
        self.output.clear();
        self.output.push_str(if step {
            "--- step ---\n"
        } else {
            "--- run ---\n"
        });
        let session = DebugSession::start_program(spec, combined, self.evt_tx.clone());
        session.send(if step {
            DebugCommand::Step
        } else {
            DebugCommand::Run
        });
        self.session = Some(session);
        self.paused = None;
        self.status = if step { "Stepping" } else { "Running" }.into();
    }

    /// Where the combined program's `line` came from.
    pub(super) fn origin_of(&self, line: usize) -> Option<Origin> {
        self.run_lines.as_ref()?.origin(line)
    }
}
