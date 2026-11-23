//! Interpreter, Variant system, and standard library for OxidizedBASIC.
//!
//! Phase 1 execution engine: a tree-walking interpreter over the
//! `oxbasic_ast` with a VB-style `Variant` tagged enum, procedure scopes
//! with ByRef parameter sharing (`Rc<RefCell<...>>`), arrays, and
//! `On Error GoTo` / `On Error Resume Next` trapping.

mod fastmap;
mod files;
use files::Field;
mod stdlib;
mod vbbinary;
mod vbtypes;

pub use stdlib::{is_builtin, pattern_matches};
pub use vbbinary::{decode_value, encode_value, FileType};
pub use vbtypes::{encode_record, file_type_of, fixed_size};

use fastmap::{Lowered, NameMap};
use oxbasic_ast::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Variant system
// ---------------------------------------------------------------------------

/// A classic-BASIC dynamic value. Dates are stored as days since
/// 1899-12-30 (the VB `DateSerial` epoch), with the fraction as time of day.
#[derive(Clone, Debug)]
pub enum Variant {
    Empty,
    Null,
    Nothing,
    Boolean(bool),
    Integer(i16),
    Long(i32),
    Single(f32),
    Double(f64),
    Currency(f64),
    Date(f64),
    String(String),
    Array(Rc<RefCell<VbArray>>),
    /// A user-defined `Type` value. Its fields are held inline, so assigning
    /// a record copies it, as VB does.
    Record(Record),
}

/// A field of a user-defined `Type`, resolved when the type is declared.
#[derive(Clone, Debug)]
pub(crate) struct TypeField {
    pub name: String,
    /// `None` for a scalar field; the element count per dimension otherwise.
    pub dims: Option<Vec<usize>>,
    pub ty: Option<String>,
}

/// A record of a user-defined `Type`: its type name and its fields in order.
#[derive(Clone, Debug)]
pub struct Record {
    pub ty: String,
    pub fields: Vec<(String, Variant)>,
}

/// A BASIC array. `dims` is empty for dynamic (auto-growing) arrays.
/// `lower` is the first valid index: 0, or 1 under `Option Base 1`.
#[derive(Clone, Debug)]
pub struct VbArray {
    pub dims: Vec<usize>,
    pub data: Vec<Variant>,
    pub lower: i64,
}

impl Variant {
    /// The value as VB's `CStr` would give it.
    pub fn as_string(&self) -> ExecResult<String> {
        to_string(self)
    }

    /// The value as a number (`CDbl`).
    pub fn as_f64(&self) -> ExecResult<f64> {
        to_f64(self)
    }

    /// The value as a whole number (`CLng`, rounded).
    pub fn as_i64(&self) -> ExecResult<i64> {
        to_i64(self)
    }

    /// The value as a Boolean (`CBool`).
    pub fn as_bool(&self) -> ExecResult<bool> {
        to_bool(self)
    }
}

impl VbArray {
    fn flat_index(&self, idxs: &[i64]) -> ExecResult<usize> {
        if self.dims.is_empty() {
            return err(9, "subscript out of range (dynamic arrays take 1 index)");
        }
        if idxs.len() != self.dims.len() {
            return err(9, "wrong number of array subscripts");
        }
        let mut flat = 0usize;
        for (i, &ix) in idxs.iter().enumerate() {
            let zero_based = ix - self.lower;
            if zero_based < 0 || zero_based as usize >= self.dims[i] {
                return err(9, "subscript out of range");
            }
            flat = flat * self.dims[i] + zero_based as usize;
        }
        Ok(flat)
    }

    /// The position in `data` of a 1-D index (a dynamic array or a vector).
    fn index_1d(&self, ix: i64) -> ExecResult<usize> {
        let zero_based = ix - self.lower;
        if zero_based < 0 || zero_based as usize >= self.data.len() {
            return err(9, "subscript out of range");
        }
        Ok(zero_based as usize)
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A trappable BASIC runtime error (VB-compatible numbers where possible).
#[derive(Clone, Debug)]
pub struct VbError {
    pub number: i32,
    pub description: String,
    /// The source line the error happened on, when the interpreter knows it
    /// (a host's own error has none until the statement that raised it is
    /// reached).
    pub line: Option<usize>,
}

// ---------------------------------------------------------------------------
// Output abstraction (console / test buffers / future GPUI bridge)
// ---------------------------------------------------------------------------

/// Where `Print` / `MsgBox` output goes and where `InputBox` reads from.
pub trait Output {
    fn write(&mut self, s: &str);
    fn read_line(&mut self) -> Option<String> {
        None
    }
    /// The native file dialog behind `OpenFileDialog` / `SaveFileDialog`:
    /// the path chosen, or `None` when cancelled (or no dialog is shown).
    /// `filter` is a semicolon list of patterns such as `*.bas;*.oxproj`.
    fn choose_file(&mut self, _filter: &str, _save: bool) -> Option<String> {
        None
    }
}

/// Standard console implementation.
pub struct ConsoleOutput;

impl Output for ConsoleOutput {
    fn write(&mut self, s: &str) {
        print!("{}", s);
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
    fn read_line(&mut self) -> Option<String> {
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => Some(line.trim_end_matches(['\r', '\n']).to_string()),
            Err(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Interpreter state
// ---------------------------------------------------------------------------

pub(crate) enum Flow {
    Normal,
    ExitDo,
    ExitFor,
    ExitProc,
    Goto(String),
    /// `Resume Next` — continue after the statement that raised the error.
    ResumeNext,
    /// `Resume` — retry the statement that raised the error.
    ResumeRetry,
    /// `Resume <label>`
    ResumeLabel(String),
    /// `End` statement: stop the program.
    Halt,
    /// `Return`: ends the subroutine (a `GoSub`) that is running it.
    Return(usize),
}

pub(crate) enum ErrMode {
    None,
    ResumeNext,
    Goto(String),
}

pub(crate) struct Frame {
    #[allow(dead_code)]
    name: String,
    is_main: bool,
    locals: NameMap<Rc<RefCell<Variant>>>,
    local_types: NameMap<String>,
    ret_name: Option<String>,
    ret: Variant,
    err_mode: ErrMode,
    /// Index (in the block that trapped the error) of the failing statement.
    err_pos: usize,
    err_number: i32,
    err_desc: String,
    /// Source line of the statement about to execute (debugger support).
    current_line: usize,
    /// The statements `GoSub` finds its labels in: the module's, or the
    /// procedure's own body.
    body: Rc<Vec<Stmt>>,
    /// How many `GoSub`s are running in this frame (a `Return` needs one).
    gosub_depth: usize,
}

impl Frame {
    fn main(body: Rc<Vec<Stmt>>) -> Frame {
        Frame {
            name: String::new(),
            is_main: true,
            locals: NameMap::default(),
            local_types: NameMap::default(),
            ret_name: None,
            ret: Variant::Empty,
            err_mode: ErrMode::None,
            err_pos: 0,
            err_number: 0,
            err_desc: String::new(),
            current_line: 0,
            body,
            gosub_depth: 0,
        }
    }

    fn proc(decl: &ProcDecl) -> Frame {
        Frame {
            name: decl.name.to_lowercase(),
            is_main: false,
            locals: NameMap::default(),
            local_types: NameMap::default(),
            ret_name: match decl.kind {
                ProcKind::Function => Some(decl.name.to_lowercase()),
                ProcKind::Sub => None,
            },
            ret: Variant::Empty,
            err_mode: ErrMode::None,
            err_pos: 0,
            err_number: 0,
            err_desc: String::new(),
            current_line: 0,
            body: decl.body.clone(),
            gosub_depth: 0,
        }
    }
}




/// A host object as the program names it: `Label1`, or one element of a
/// control array, `Command1(2)`.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectRef {
    pub name: String,
    pub index: Option<i32>,
}

/// Objects the program can reach by name — a form's controls, so
/// `Label1.Caption = "x"`, `n = List1.ListCount` and `List1.AddItem "a"`
/// read and change the live form. The interpreter asks the host first
/// for every `Name.Member` whose `Name` is not a variable, and keeps its
/// own errors for the names the host doesn't know. A sub-object's member
/// arrives as a dotted property: `Label1.Font.Bold` is `Font.Bold`.
pub trait ObjectHost {
    /// Whether `name` (case-insensitive) is an object of this host.
    fn has_object(&self, name: &str) -> bool;
    /// Read `object.property`; `args` are a parameterised property's
    /// indexes (`List1.List(0)`).
    fn get(&mut self, object: &ObjectRef, property: &str, args: &[Variant]) -> ExecResult<Variant>;
    /// Write `object.property = value`.
    fn set(
        &mut self,
        object: &ObjectRef,
        property: &str,
        args: &[Variant],
        value: Variant,
    ) -> ExecResult<()>;
    /// Call `object.method args` (`List1.AddItem "a"`, `Text1.SetFocus`).
    fn call(&mut self, object: &ObjectRef, method: &str, args: &[Variant]) -> ExecResult<Variant>;
    /// The form whose code is running (so a bare `Label1` means *its*
    /// control). Empty when no form's code is.
    fn context(&self) -> String {
        String::new()
    }
    /// The interpreter entered code that belongs to form `ctx`.
    fn set_context(&mut self, _ctx: &str) {}
    /// The events the program's own statements raised since the last call
    /// (`Text1.Text = "x"` raises `Text1_Change`, as in VB), oldest first.
    /// The interpreter runs their handlers straight after the statement.
    fn take_raised(&mut self) -> Vec<RaisedEvent> {
        Vec::new()
    }
    /// `Unload object` is about to run: the name of the form it unloads, if
    /// that form is loaded, so its `Form_QueryUnload` and `Form_Unload`
    /// handlers can run first (and cancel it). Hosts without forms: `None`.
    fn unloading_form(&self, _object: &ObjectRef) -> Option<String> {
        None
    }
    /// Draw `shape` on `object` (`None`: the form whose code runs), its
    /// coordinates already worked out (a `Step` added to `CurrentX/Y`).
    /// Hosts that can't draw raise 438.
    fn draw(&mut self, _object: Option<&ObjectRef>, _shape: DrawCmd) -> ExecResult<()> {
        Err(VbError {
            number: 438,
            description: "object doesn't support this property or method".into(),
            line: None,
        })
    }
    /// `CurrentX` / `CurrentY` of `object` (`None`: the form), where a
    /// `Step` counts from.
    fn current_point(&self, _object: Option<&ObjectRef>) -> (f32, f32) {
        (0.0, 0.0)
    }
}

/// A shape a graphics statement draws, in the drawing's own coordinates
/// (pixels from its top-left). `color` is `None` for the drawing's
/// `ForeColor`.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCmd {
    Line {
        from: (f32, f32),
        to: (f32, f32),
        color: Option<u32>,
        /// 0 a line, 1 a box, 2 a filled box.
        boxed: u8,
    },
    PSet {
        at: (f32, f32),
        color: Option<u32>,
    },
    Circle {
        center: (f32, f32),
        radius: f32,
        color: Option<u32>,
        /// An arc from `start` to `end` (radians); negative values also
        /// draw the radius to that end (a pie slice), as in VB.
        start: Option<f32>,
        end: Option<f32>,
        /// Height over width (1 a circle).
        aspect: f32,
    },
}

/// An event a host object raised by itself (a property change made by the
/// program), for the interpreter to run.
#[derive(Debug, Clone)]
pub struct RaisedEvent {
    /// The form the control is on (handlers of a project's forms are
    /// called `Form2__Text1_Change` in the combined program).
    pub form: String,
    /// The handler as the form's code names it, e.g. `Text1_Change`.
    pub handler: String,
    pub args: Vec<Variant>,
}

/// The OxidizedBASIC interpreter.
pub struct Interpreter<'out> {
    /// The folder relative file names (`LoadPicture("logo.png")`) are in:
    /// the project's, when run from the IDE.
    pub base_dir: Option<std::path::PathBuf>,
    pub(crate) decls: Vec<Rc<ProcDecl>>,
    pub(crate) proc_index: NameMap<usize>,
    top_level: Vec<Stmt>,
    pub(crate) globals: NameMap<Rc<RefCell<Variant>>>,
    pub(crate) global_types: NameMap<String>,
    pub(crate) constants: NameMap<Variant>,
    /// User-defined `Type`s: each field's name, its declared type, and (for an
    /// array field) its element counts, resolved when the type is declared.
    pub(crate) user_types: NameMap<Vec<TypeField>>,
    pub(crate) out: &'out mut dyn Output,
    pub(crate) option_explicit: bool,
    /// `Option Compare Text`: string comparisons ignore case.
    pub(crate) option_compare_text: bool,
    /// The type `DefInt`/`DefStr`/... gives an undeclared name by its first
    /// letter (index 0 = `a`), or `None` for the default Variant.
    pub(crate) def_types: [Option<String>; 26],
    /// `Option Base 0|1`: the lowest index of an array declared from now on.
    pub(crate) option_base: i64,
    pub(crate) rng: Cell<u64>,
    pub(crate) depth: usize,
    /// Debugger: source lines with breakpoints (shared with the IDE).
    breakpoints: Option<std::sync::Arc<std::sync::Mutex<std::collections::HashSet<usize>>>>,
    /// Debugger: pause again when execution returns to this call depth.
    step_depth: Option<usize>,
    /// Debugger: called when a breakpoint or step lands.
    debug_hook: Option<Box<dyn FnMut(&PausedInfo) -> DebugAction>>,
    /// The objects `Name.Member` resolves against (a form's controls).
    objects: Option<Rc<RefCell<dyn ObjectHost>>>,
    /// `End` ran: the program is over, wherever it was.
    ended: bool,
    /// `DoEvents`: lets the host show what the program drew so far and
    /// hear a Stop; returns `false` to abort the program.
    doevents_hook: Option<Box<dyn FnMut() -> bool>>,
    /// The objects of the `With` blocks being run, innermost last.
    with_stack: Vec<Expr>,
    /// The files the program has open, by number (`Open ... As #n`).
    pub(crate) files: files::FileTable,
}

/// Debugger pause notification: where execution stopped and what the
/// variables hold (values rendered as display strings).
#[derive(Clone, Debug)]
pub struct PausedInfo {
    /// 1-based source line of the statement about to execute.
    pub line: usize,
    /// Procedure name ("" for module-level code).
    pub proc: String,
    /// Current call depth (0 = module level).
    pub depth: usize,
    /// Local variables of the current frame: (name, display value).
    pub locals: Vec<(String, String)>,
    /// Module-level (global) variables: (name, display value).
    pub globals: Vec<(String, String)>,
}

/// What the debugger tells the interpreter to do after a pause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugAction {
    /// Run until the next breakpoint.
    Continue,
    /// Execute the current statement, then pause at the next statement
    /// at the same (or shallower) call depth.
    StepOver,
    /// Stop the program.
    Abort,
}

/// Trappable "execution aborted by the debugger" error number.
pub const DEBUG_ABORT: i32 = 9998;

/// Parse and run a whole program against an output sink.
pub fn run_program(prog: &Program, out: &mut dyn Output) -> ExecResult<()> {
    let mut it = Interpreter::new(prog, out);
    it.run()
}

/// Convenience: parse source and run it.
pub fn run_source(src: &str, out: &mut dyn Output) -> Result<(), String> {
    let prog = oxbasic_parser::parse(src)?;
    run_program(&prog, out).map_err(|e| e.to_string())
}

impl std::fmt::Display for VbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(
                f,
                "runtime error {}: {} on line {}",
                self.number, self.description, line
            ),
            None => write!(f, "runtime error {}: {}", self.number, self.description),
        }
    }
}

pub type ExecResult<T> = Result<T, VbError>;

pub(crate) fn err<T>(number: i32, description: &str) -> ExecResult<T> {
    Err(VbError {
        number,
        description: description.to_string(),
        line: None,
    })
}

impl<'out> Interpreter<'out> {
    pub fn new(prog: &Program, out: &'out mut dyn Output) -> Interpreter<'out> {
        let mut it = Interpreter {
            decls: Vec::new(),
            proc_index: NameMap::default(),
            top_level: Vec::new(),
            globals: NameMap::default(),
            global_types: NameMap::default(),
            constants: builtin_constants(),
            user_types: NameMap::default(),
            out,
            option_explicit: false,
            option_compare_text: false,
            def_types: Default::default(),
            option_base: 0,
            rng: Cell::new(0x9E3779B97F4A7C15),
            depth: 0,
            breakpoints: None,
            step_depth: None,
            debug_hook: None,
            objects: None,
            ended: false,
            doevents_hook: None,
            with_stack: Vec::new(),
            files: files::FileTable::default(),
            base_dir: None,
        };
        it.add_program(prog);
        it
    }

    /// Install the host whose objects (a form's controls) the program's
    /// `Name.Member` reads, writes and calls go to. The caller keeps its
    /// own `Rc` to see what the program changed.
    pub fn set_object_host(&mut self, host: Rc<RefCell<dyn ObjectHost>>) {
        self.objects = Some(host);
    }

    /// Register declarations (also used by the REPL).
    pub fn add_program(&mut self, prog: &Program) {
        for item in &prog.items {
            match item {
                Item::Proc(p) => {
                    self.proc_index
                        .insert(p.name.to_lowercase(), self.decls.len());
                    self.decls.push(Rc::new(p.clone()));
                }
                Item::Stmt(s) => {
                    self.register_declaration(s);
                    self.top_level.push(s.clone());
                }
            }
        }
    }

    /// A `Type` or `Enum` is known from the moment the program is loaded, so
    /// procedures and code above it can use it: the type's fields are kept,
    /// the enum's members become named constants.
    fn register_declaration(&mut self, s: &Stmt) {
        match s {
            Stmt::TypeDecl { name, fields } => {
                // a field's bounds are constant expressions: resolve them now
                let mut resolved = Vec::with_capacity(fields.len());
                for fd in fields {
                    let dims = match &fd.dims {
                        None => None,
                        Some(exprs) => {
                            let mut ds = Vec::new();
                            for e in exprs {
                                let mut f = Frame::main(Rc::new(Vec::new()));
                                let upper = self
                                    .eval(e, &mut f)
                                    .and_then(|v| to_i64(&v))
                                    .unwrap_or(0);
                                ds.push((upper - self.option_base + 1).max(0) as usize);
                            }
                            Some(ds)
                        }
                    };
                    resolved.push(TypeField {
                        name: fd.name.to_lowercase(),
                        dims,
                        ty: fd.ty.clone(),
                    });
                }
                self.user_types.insert(name.to_lowercase(), resolved);
            }
            Stmt::EnumDecl { members, .. } => {
                for (member, value) in members {
                    // the values are constant expressions: evaluate them now
                    let mut f = Frame::main(Rc::new(Vec::new()));
                    if let Ok(v) = self.eval(value, &mut f) {
                        self.constants.insert(member.to_lowercase(), v);
                    }
                }
            }
            Stmt::DefType(ranges) => self.register_def_types(ranges),
            _ => {}
        }
    }

    pub fn run(&mut self) -> ExecResult<()> {
        let stmts = self.top_level.clone();
        let mut f = Frame::main(Rc::new(stmts.clone()));
        let result = self.exec_block(&stmts, &mut f);
        // files are closed when the program ends, error or not, so output
        // that was buffered is written
        let closed = self.files.close_all();
        result?;
        closed?;
        Ok(())
    }

    /// Whether the program ran `End`. A caller serving events checks this
    /// after each call: `End` inside a nested `Sub` only unwinds that call,
    /// but the program as a whole is over.
    pub fn has_ended(&self) -> bool {
        self.ended
    }

    /// Execute one source snippet in the global (main) scope (REPL).
    pub fn repl_line(&mut self, src: &str) -> Result<(), String> {
        let prog = oxbasic_parser::parse(src)?;
        let stmts: Vec<Stmt> = prog
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Proc(p) => {
                    self.proc_index
                        .insert(p.name.to_lowercase(), self.decls.len());
                    self.decls.push(Rc::new(p.clone()));
                    None
                }
                Item::Stmt(s) => Some(s.clone()),
            })
            .collect();
        let mut f = Frame::main(Rc::new(stmts.clone()));
        self.exec_block(&stmts, &mut f)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Names of every user-defined `Sub`/`Function` (for event wiring).
    pub fn proc_names(&self) -> Vec<String> {
        self.decls.iter().map(|d| d.name.clone()).collect()
    }

    /// Share the breakpoint set with a debugger (e.g. the IDE). The set
    /// holds 1-based source line numbers and may be mutated while running.
    pub fn set_breakpoints(
        &mut self,
        breakpoints: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<usize>>>,
    ) {
        self.breakpoints = Some(breakpoints);
    }

    /// Install the debugger pause hook. It is called whenever a
    /// breakpoint or a pending step lands, and decides how to proceed.
    /// Installing the hook also breaks at the first statement
    /// ("break on start").
    pub fn set_debug_hook(&mut self, hook: Box<dyn FnMut(&PausedInfo) -> DebugAction>) {
        self.debug_hook = Some(hook);
        self.step_depth = Some(0);
    }

    /// Install what `DoEvents` does: the host shows what the program has
    /// drawn so far and answers whether to go on (`false` aborts, as a
    /// Stop does). Without one, `DoEvents` does nothing.
    pub fn set_doevents_hook(&mut self, hook: Box<dyn FnMut() -> bool>) {
        self.doevents_hook = Some(hook);
    }

    /// `DoEvents`: run the host's hook, if any.
    pub(crate) fn do_events(&mut self) -> ExecResult<()> {
        if let Some(hook) = self.doevents_hook.as_mut() {
            if !hook() {
                return err(DEBUG_ABORT, "execution aborted by the debugger");
            }
        }
        Ok(())
    }

    /// Debugger: called before each statement; blocks inside the hook.
    fn debug_check(&mut self, f: &mut Frame) -> ExecResult<()> {
        if self.debug_hook.is_none() {
            return Ok(());
        }
        let at_breakpoint = self
            .breakpoints
            .as_ref()
            .map(|b| {
                b.lock()
                    .map(|b| b.contains(&f.current_line))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        let step_due = match self.step_depth {
            Some(d) => self.depth <= d,
            None => false,
        };
        if !at_breakpoint && !step_due {
            return Ok(());
        }
        let info = PausedInfo {
            line: f.current_line,
            proc: f.name.clone(),
            depth: self.depth,
            locals: f
                .locals
                .iter()
                .map(|(k, v)| (k.clone(), debug_repr(&v.borrow())))
                .collect(),
            globals: self
                .globals
                .iter()
                .map(|(k, v)| (k.clone(), debug_repr(&v.borrow())))
                .collect(),
        };
        let action = (self.debug_hook.as_mut().unwrap())(&info);
        match action {
            DebugAction::Continue => self.step_depth = None,
            DebugAction::StepOver => self.step_depth = Some(self.depth),
            DebugAction::Abort => return err(DEBUG_ABORT, "execution aborted by the debugger"),
        }
        Ok(())
    }

    /// Invoke a BASIC `Sub`/`Function` by name with evaluated argument
    /// values — the Phase 2 UI event bridge (spec §5.1:
    /// `runtime.evaluate_sub("Command1_Click", vec![])`).
    ///
    /// Parameters are bound by value (ByVal semantics). Returns the
    /// function value, or `Empty` for a `Sub`.
    pub fn evaluate_sub(&mut self, name: &str, args: &[Variant]) -> ExecResult<Variant> {
        self.invoke(name, args).map(|(ret, _)| ret)
    }

    /// The parameter count of procedure `name`, if the program has it.
    pub fn param_count(&self, name: &str) -> Option<usize> {
        let idx = *self.proc_index.get(&name.to_lowercase())?;
        Some(self.decls[idx].params.len())
    }

    /// Raise an event: call handler `name` with the event's arguments, as
    /// VB passes them — by reference, so a handler that sets `KeyAscii = 0`
    /// or `Cancel = True` changes the caller's value. Returns the arguments
    /// as the handler left them. A handler declaring fewer parameters than
    /// the event has gets the leading ones (a placeholder written before
    /// `Index` was added still runs).
    pub fn call_event(&mut self, name: &str, args: &[Variant]) -> ExecResult<Vec<Variant>> {
        let n = self.param_count(name).unwrap_or(args.len()).min(args.len());
        let (_, finals) = self.invoke(name, &args[..n])?;
        let mut out = args.to_vec();
        for (slot, v) in out.iter_mut().zip(finals) {
            *slot = v;
        }
        Ok(out)
    }

    /// Run the handlers of the events the host's objects raised by
    /// themselves (a property the program set), straight after the
    /// statement that set it, as VB raises `Text1_Change` inside the
    /// assignment. Handlers the program doesn't have are skipped.
    pub fn run_raised_events(&mut self) -> ExecResult<()> {
        let Some(host) = self.objects.clone() else {
            return Ok(());
        };
        // an event's handler may raise more; VB has no limit but a stack
        // overflow, so guard against a handler that sets its own property
        if self.depth > 400 {
            return err(28, "out of stack space");
        }
        loop {
            let raised = host.borrow_mut().take_raised();
            if raised.is_empty() {
                return Ok(());
            }
            for ev in raised {
                let Some(name) = self.handler_name(&ev.form, &ev.handler) else {
                    continue;
                };
                self.call_event(&name, &ev.args)?;
            }
        }
    }

    /// The program's procedure for `form`'s `handler`: `Form2__Text1_Change`
    /// in a project's combined program, else `Text1_Change`.
    pub fn handler_name(&self, form: &str, handler: &str) -> Option<String> {
        let combined = format!("{}__{}", form, handler).to_lowercase();
        let bare = handler.to_lowercase();
        [combined, bare]
            .into_iter()
            .find(|n| self.proc_index.contains_key(n))
            .map(|n| self.decls[self.proc_index[&n]].name.clone())
    }

    /// Ask form `form` whether it may close, as VB does before unloading
    /// it: `Form_QueryUnload(Cancel, UnloadMode)` then `Form_Unload(Cancel)`;
    /// either handler setting `Cancel` to a non-zero value keeps the form.
    /// `mode` is VB's `UnloadMode` (0 = the user closed the window, 1 =
    /// `Unload` from code). Returns whether the form may go.
    pub fn confirm_unload(&mut self, form: &str, mode: i16) -> ExecResult<bool> {
        let cancelled = |v: &Variant| v.as_f64().map(|n| n != 0.0).unwrap_or(false);
        if let Some(h) = self.handler_name(form, "Form_QueryUnload") {
            let out = self.call_event(&h, &[Variant::Integer(0), Variant::Integer(mode)])?;
            if cancelled(&out[0]) {
                return Ok(false);
            }
        }
        if let Some(h) = self.handler_name(form, "Form_Unload") {
            let out = self.call_event(&h, &[Variant::Integer(0)])?;
            if cancelled(&out[0]) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `Unload object` from code: its form's unload handlers decide.
    fn query_unload(&mut self, object: &ObjectRef) -> ExecResult<bool> {
        let form = match &self.objects {
            Some(host) => host.borrow().unloading_form(object),
            None => None,
        };
        match form {
            Some(form) => self.confirm_unload(&form, 1),
            None => Ok(true),
        }
    }

    /// Call procedure `name` with evaluated `args`; returns its value and
    /// each parameter's final value (for ByRef write-back).
    fn invoke(&mut self, name: &str, args: &[Variant]) -> ExecResult<(Variant, Vec<Variant>)> {
        let ln = name.to_lowercase();
        let idx = match self.proc_index.get(&ln) {
            Some(&i) => i,
            None => return err(35, &format!("Sub or Function not defined: {}", name)),
        };
        if self.depth > 400 {
            return err(28, "out of stack space");
        }
        let decl = self.decls[idx].clone();
        let mut callee = Frame::proc(&decl);
        let nparams = decl.params.len();
        let has_param_array = decl.params.iter().any(|p| p.param_array);
        if args.len() > nparams && !has_param_array {
            return err(450, &format!("wrong number of arguments for {}", name));
        }
        for (i, p) in decl.params.iter().enumerate() {
            if p.param_array {
                let start = i.min(args.len());
                let items: Vec<Variant> = args[start..].to_vec();
                let n = items.len();
                callee.locals.insert(
                    p.name.to_lowercase(),
                    Rc::new(RefCell::new(Variant::Array(Rc::new(RefCell::new(
                        VbArray {
                            dims: vec![n],
                            data: items,
                            lower: 0,
                        },
                    ))))),
                );
                break;
            }
            if i >= args.len() {
                if !p.optional {
                    return err(449, &format!("argument not optional: {}", p.name));
                }
                let v = match &p.default {
                    Some(d) => {
                        // defaults are constant expressions; evaluate in a
                        // scratch main frame
                        let mut scratch = Frame::main(Rc::new(Vec::new()));
                        self.eval(d, &mut scratch)?
                    }
                    None => Variant::Empty,
                };
                callee
                    .locals
                    .insert(p.name.to_lowercase(), Rc::new(RefCell::new(v)));
                continue;
            }
            let v = match &p.ty {
                Some(t) => coerce_typed(&args[i], t)?,
                None => args[i].clone(),
            };
            callee
                .locals
                .insert(p.name.to_lowercase(), Rc::new(RefCell::new(v)));
        }
        self.depth += 1;
        let body = decl.body.clone();
        let prev = self.enter_context(&decl.name);
        let flow = self.exec_block(&body, &mut callee);
        self.leave_context(prev);
        self.depth -= 1;
        flow?;
        let finals = decl
            .params
            .iter()
            .take(args.len())
            .map(|p| {
                callee
                    .locals
                    .get(&p.name.to_lowercase())
                    .map(|v| v.borrow().clone())
                    .unwrap_or(Variant::Empty)
            })
            .collect();
        Ok((callee.ret, finals))
    }

    /// A procedure named `Form2__Command1_Click` belongs to form `Form2`:
    /// tell the host, so a bare `Label1` in it means that form's control.
    /// Returns the form that was running, to restore on leaving.
    fn enter_context(&mut self, proc_name: &str) -> Option<String> {
        let host = self.objects.as_ref()?;
        let (form, _) = proc_name.split_once("__")?;
        let prev = host.borrow().context();
        host.borrow_mut().set_context(form);
        Some(prev)
    }

    fn leave_context(&mut self, prev: Option<String>) {
        if let (Some(prev), Some(host)) = (prev, &self.objects) {
            host.borrow_mut().set_context(&prev);
        }
    }

    // -- scope helpers ------------------------------------------------------

    fn lookup_rc(&self, f: &Frame, name: &str) -> Option<Rc<RefCell<Variant>>> {
        self.lookup_ref(f, Lowered::new(name).as_str()).cloned()
    }

    /// The variable `ln` (already lowercase): the frame's own, else a
    /// global.
    #[inline]
    fn lookup_ref<'a>(&'a self, f: &'a Frame, ln: &str) -> Option<&'a Rc<RefCell<Variant>>> {
        if !f.is_main {
            if let Some(rc) = f.locals.get(ln) {
                return Some(rc);
            }
        }
        self.globals.get(ln)
    }

    fn declared_type(&self, f: &Frame, name: &str) -> Option<String> {
        self.declared_type_ref(f, Lowered::new(name).as_str())
            .map(|t| t.to_string())
    }

    #[inline]
    fn declared_type_ref<'a>(&'a self, f: &'a Frame, ln: &str) -> Option<&'a str> {
        if !f.is_main {
            if let Some(t) = f.local_types.get(ln) {
                return Some(t);
            }
        }
        self.global_types.get(ln).map(|t| t.as_str())
    }

    fn set_var(&mut self, f: &mut Frame, name: &str, value: Variant) -> ExecResult<()> {
        let lowered = Lowered::new(name);
        let ln = lowered.as_str();
        // The common case, an existing variable, is one lookup: its declared
        // type (if any) is read from the same frame in the same pass.
        if let Some(rc) = self.lookup_ref(f, ln) {
            let value = match self.declared_type_ref(f, ln) {
                Some(t) => coerce_typed(&value, t)?,
                None => value,
            };
            *rc.borrow_mut() = value;
            return Ok(());
        }
        if self.option_explicit {
            return err(500, &format!("Variable not defined: {}", name));
        }
        let value = match self.implicit_type(name) {
            Some(t) => coerce_typed(&value, &t)?,
            None => value,
        };
        let rc = Rc::new(RefCell::new(value));
        if f.is_main {
            self.globals.insert(ln.to_string(), rc);
        } else {
            f.locals.insert(ln.to_string(), rc);
        }
        Ok(())
    }

    /// The `DefInt`-style type of an undeclared `name`, from its first letter.
    fn implicit_type(&self, name: &str) -> Option<String> {
        let first = name.chars().next()?.to_ascii_lowercase();
        if !first.is_ascii_lowercase() {
            return None;
        }
        self.def_types[(first as u8 - b'a') as usize].clone()
    }

    /// `DefInt A-C, X`: records each range's type for undeclared names.
    fn register_def_types(&mut self, ranges: &[(String, char, char)]) {
        for (ty, first, last) in ranges {
            for c in *first..=*last {
                if c.is_ascii_lowercase() {
                    self.def_types[(c as u8 - b'a') as usize] = Some(ty.clone());
                }
            }
        }
    }

    /// The starting value of a variable declared `As ty`: a record for a
    /// user-defined `Type`, else the scalar default for the built-in type.
    /// The VB binary bytes of a user `Type` value, its fields in order. A
    /// field that is itself a user `Type` is laid out the same way, recursively.
    fn record_bytes(&self, r: &Record) -> ExecResult<Vec<u8>> {
        let Some(fields) = self.user_types.get(&r.ty.to_lowercase()) else {
            return err(5, "Invalid procedure call");
        };
        let mut out = Vec::new();
        for fd in fields {
            let Some((_, v)) = r.fields.iter().find(|(n, _)| n.eq_ignore_ascii_case(&fd.name)) else {
                return err(9, "record field missing");
            };
            let ty_name = fd.ty.clone().unwrap_or_default();
            match v {
                Variant::Record(inner) if self.user_types.contains_key(&inner.ty.to_lowercase()) => {
                    out.extend(self.record_bytes(inner)?);
                }
                _ => {
                    let Some(ty) = crate::vbtypes::file_type_of(&ty_name) else {
                        return err(5, "Type of a record field is not supported in a file");
                    };
                    out.extend(encode_value(&ty, v)?);
                }
            }
        }
        Ok(out)
    }

    /// Decode a user `Type` value of `ty_name`'s layout from `bytes` at `*at`,
    /// keeping the record's own type. Nested `Type` fields decode recursively.
    fn record_from_bytes(&self, ty_name: &str, bytes: &[u8], at: &mut usize) -> ExecResult<Record> {
        let Some(fields) = self.user_types.get(&ty_name.to_lowercase()) else {
            return err(5, "Invalid procedure call");
        };
        let mut out = Vec::with_capacity(fields.len());
        for fd in fields {
            let ty_name_field = fd.ty.clone().unwrap_or_default();
            let v = if self.user_types.contains_key(&ty_name_field.to_lowercase()) {
                Variant::Record(self.record_from_bytes(&ty_name_field, bytes, at)?)
            } else {
                let Some(ty) = crate::vbtypes::file_type_of(&ty_name_field) else {
                    return err(5, "Type of a record field is not supported in a file");
                };
                decode_value(&ty, bytes, at)?
            };
            out.push((fd.name.clone(), v));
        }
        Ok(Record {
            ty: ty_name.to_string(),
            fields: out,
        })
    }

    fn default_value(&self, ty: Option<&str>) -> ExecResult<Variant> {
        let Some(t) = ty else {
            return Ok(Variant::Empty);
        };
        let Some(fields) = self.user_types.get(&t.to_lowercase()) else {
            return Ok(default_for_type(Some(t)));
        };
        let mut out = Vec::with_capacity(fields.len());
        for fd in fields {
            let value = match &fd.dims {
                // an array field: a fixed array of the field's type
                Some(ds) => {
                    let n: usize = ds.iter().product();
                    let def = self.default_value(fd.ty.as_deref())?;
                    Variant::Array(Rc::new(RefCell::new(VbArray {
                        dims: ds.clone(),
                        data: (0..n).map(|_| def.clone()).collect(),
                        lower: self.option_base,
                    })))
                }
                None => self.default_value(fd.ty.as_deref())?,
            };
            out.push((fd.name.clone(), value));
        }
        Ok(Variant::Record(Record {
            ty: t.to_lowercase(),
            fields: out,
        }))
    }

    /// `obj` names a variable (or array element) holding a user-defined record.
    fn is_record_place(&mut self, obj: &Expr, f: &mut Frame) -> bool {
        self.with_record(obj, f, |_| ()).is_ok()
    }

    /// Run `op` on the record `obj` names, a variable or an array element
    /// (`pts(2)`), changing it in place. Errors if `obj` names no record.
    fn with_record<T>(
        &mut self,
        obj: &Expr,
        f: &mut Frame,
        op: impl FnOnce(&mut Record) -> T,
    ) -> ExecResult<T> {
        let not_record = || err(13, "type mismatch: not a user-defined record");
        match obj {
            Expr::Var(name) => {
                let Some(rc) = self.lookup_rc(f, name) else {
                    return not_record();
                };
                let mut borrowed = rc.borrow_mut();
                match &mut *borrowed {
                    Variant::Record(r) => Ok(op(r)),
                    _ => not_record(),
                }
            }
            Expr::Call { name, args } if args.len() == 1 => {
                let Some(rc) = self.lookup_rc(f, name) else {
                    return not_record();
                };
                let ix = to_i64(&self.eval(&args[0], f)?)?;
                let mut borrowed = rc.borrow_mut();
                let Variant::Array(arr) = &mut *borrowed else {
                    return not_record();
                };
                let mut arr = arr.borrow_mut();
                let at = arr.index_1d(ix)?;
                match &mut arr.data[at] {
                    Variant::Record(r) => Ok(op(r)),
                    _ => not_record(),
                }
            }
            // a record inside a record: `o.I` in `o.I.N`. The outer record is
            // reached from its variable, then its fields are walked in turn.
            Expr::Member { obj: inner, name } => {
                let mut chain = vec![name.clone()];
                let mut base = inner.as_ref();
                while let Expr::Member { obj, name } = base {
                    chain.push(name.clone());
                    base = obj.as_ref();
                }
                chain.reverse();
                let Expr::Var(var) = base else {
                    return not_record();
                };
                let Some(rc) = self.lookup_rc(f, var) else {
                    return not_record();
                };
                let mut borrowed = rc.borrow_mut();
                let Variant::Record(root) = &mut *borrowed else {
                    return not_record();
                };
                let mut cur: &mut Record = root;
                for step in &chain {
                    match cur.fields.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(step)) {
                        Some((_, Variant::Record(r))) => cur = r,
                        _ => return not_record(),
                    }
                }
                Ok(op(cur))
            }
            _ => not_record(),
        }
    }

    /// `obj.name` read from a record: `None` unless `obj` names a record.
    fn record_field_value(
        &mut self,
        obj: &Expr,
        name: &str,
        f: &mut Frame,
    ) -> ExecResult<Option<Variant>> {
        if !self.is_record_place(obj, f) {
            return Ok(None);
        }
        self.with_record(obj, f, |r| {
            match r.fields.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
                Some((_, v)) => Ok(Some(v.clone())),
                None => err(438, &format!("object doesn't support this property: {}", name)),
            }
        })?
    }

    /// `obj.name = v` into a record, changed in place.
    fn record_field_store(
        &mut self,
        obj: &Expr,
        name: &str,
        v: Variant,
        f: &mut Frame,
    ) -> ExecResult<()> {
        self.with_record(obj, f, |r| {
            match r.fields.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
                Some((_, slot)) => {
                    *slot = v;
                    Ok(())
                }
                None => err(438, &format!("object doesn't support this property: {}", name)),
            }
        })?
    }

    fn declare_var(
        &mut self,
        f: &mut Frame,
        name: &str,
        dims: Option<Vec<usize>>,
        ty: &Option<String>,
    ) -> ExecResult<()> {
        let ln = name.to_lowercase();
        let value = match dims {
            Some(ds) => {
                let n: usize = ds.iter().product();
                // an array of records holds one record per element
                let def = self.default_value(ty.as_deref())?;
                let data = (0..n).map(|_| def.clone()).collect();
                Variant::Array(Rc::new(RefCell::new(VbArray {
                    dims: ds,
                    data,
                    lower: self.option_base,
                })))
            }
            None => self.default_value(ty.as_deref())?,
        };
        let rc = Rc::new(RefCell::new(value));
        if f.is_main {
            self.globals.insert(ln.clone(), rc);
            if let Some(t) = ty {
                self.global_types.insert(ln, t.to_lowercase());
            }
        } else {
            f.locals.insert(ln.clone(), rc);
            if let Some(t) = ty {
                f.local_types.insert(ln, t.to_lowercase());
            }
        }
        Ok(())
    }
}

fn builtin_constants() -> NameMap<Variant> {
    let mut c = NameMap::default();
    let mut ins = |k: &str, v: Variant| {
        c.insert(k.to_string(), v);
    };
    ins("vbcrlf", Variant::String("\r\n".into()));
    ins("vbcr", Variant::String("\r".into()));
    ins("vblf", Variant::String("\n".into()));
    ins("vbnewline", Variant::String("\r\n".into()));
    ins("vbtab", Variant::String("\t".into()));
    ins("vbnullstring", Variant::String(String::new()));
    ins("vbnullchar", Variant::String("\0".into()));
    ins("vbokonly", Variant::Long(0));
    ins("vbokcancel", Variant::Long(1));
    ins("vbyesno", Variant::Long(4));
    ins("vbyesnocancel", Variant::Long(3));
    ins("vbquestion", Variant::Long(32));
    ins("vbinformation", Variant::Long(64));
    ins("vbexclamation", Variant::Long(48));
    ins("vbcritical", Variant::Long(16));
    ins("vbtrue", Variant::Boolean(true));
    ins("vbfalse", Variant::Boolean(false));
    // CheckBox.Value
    ins("vbunchecked", Variant::Long(0));
    ins("vbchecked", Variant::Long(1));
    ins("vbgrayed", Variant::Long(2));
    // colours, as RGB() makes them (&H00BBGGRR)
    ins("vbblack", Variant::Long(0x000000));
    ins("vbred", Variant::Long(0x0000FF));
    ins("vbgreen", Variant::Long(0x00FF00));
    ins("vbyellow", Variant::Long(0x00FFFF));
    ins("vbblue", Variant::Long(0xFF0000));
    ins("vbmagenta", Variant::Long(0xFF00FF));
    ins("vbcyan", Variant::Long(0xFFFF00));
    ins("vbwhite", Variant::Long(0xFFFFFF));
    // system colours: &H80000000 + the Windows COLOR_* index
    for (i, name) in [
        "vbscrollbars",
        "vbdesktop",
        "vbactivetitlebar",
        "vbinactivetitlebar",
        "vbmenubar",
        "vbwindowbackground",
        "vbwindowframe",
        "vbmenutext",
        "vbwindowtext",
        "vbtitlebartext",
        "vbactiveborder",
        "vbinactiveborder",
        "vbapplicationworkspace",
        "vbhighlight",
        "vbhighlighttext",
        "vbbuttonface",
        "vbbuttonshadow",
        "vbgraytext",
        "vbbuttontext",
        "vbinactivecaptiontext",
        "vb3dhighlight",
        "vb3ddkshadow",
        "vb3dlight",
        "vbinfotext",
        "vbinfobackground",
    ]
    .iter()
    .enumerate()
    {
        ins(name, Variant::Long((0x8000_0000u32 + i as u32) as i32));
    }
    ins("vb3dface", Variant::Long(0x8000_000Fu32 as i32));
    ins("vb3dshadow", Variant::Long(0x8000_0010u32 as i32));
    // MousePointer
    for (n, name) in [
        (0, "vbdefault"),
        (1, "vbarrow"),
        (2, "vbcrosshair"),
        (3, "vbibeam"),
        (5, "vbsizepointer"),
        (6, "vbsizenesw"),
        (7, "vbsizens"),
        (8, "vbsizenwse"),
        (9, "vbsizewe"),
        (10, "vbuparrow"),
        (11, "vbhourglass"),
        (12, "vbnodrop"),
        (13, "vbarrowhourglass"),
        (14, "vbarrowquestion"),
        (15, "vbsizeall"),
        (99, "vbcustom"),
    ] {
        ins(name, Variant::Integer(n));
    }
    // ZOrder, ScrollBars, MultiSelect
    ins("vbbringtofront", Variant::Integer(0));
    ins("vbsendtoback", Variant::Integer(1));
    ins("vbsbnone", Variant::Integer(0));
    ins("vbhorizontal", Variant::Integer(1));
    ins("vbvertical", Variant::Integer(2));
    ins("vbboth", Variant::Integer(3));
    ins("vbmultiselectnone", Variant::Integer(0));
    ins("vbmultiselectsimple", Variant::Integer(1));
    ins("vbmultiselectextended", Variant::Integer(2));
    c
}

// ---------------------------------------------------------------------------
// Statement execution
// ---------------------------------------------------------------------------

impl<'out> Interpreter<'out> {
    /// Execute a statement block. Handles labels, `GoTo`, error trapping
    /// (`On Error GoTo <label>` / `Resume Next`) and `Resume`.
    pub(crate) fn exec_block(&mut self, stmts: &[Stmt], f: &mut Frame) -> ExecResult<Flow> {
        // The label table is built only when a GoTo or error handler needs it,
        // not on every run of the block (a loop body runs many times).
        let mut labels: Option<HashMap<String, usize>> = None;
        let find_label = |labels: &mut Option<HashMap<String, usize>>, name: &str| -> Option<usize> {
            let map = labels.get_or_insert_with(|| {
                let mut m = HashMap::new();
                for (i, s) in stmts.iter().enumerate() {
                    if let Stmt::Label(l) = s {
                        m.entry(l.to_lowercase()).or_insert(i);
                    }
                }
                m
            });
            map.get(&name.to_lowercase()).copied()
        };
        let mut i = 0usize;
        while i < stmts.len() {
            if let Stmt::Label(_) = &stmts[i] {
                i += 1;
                continue;
            }
            if let Stmt::LineMarker(l) = &stmts[i] {
                f.current_line = *l;
                i += 1;
                continue;
            }
            // Debugger: pause on breakpoints / pending steps.
            self.debug_check(f)?;
            match self.exec_stmt(&stmts[i], f) {
                Ok(flow) => match flow {
                    Flow::Normal => {
                        // `End` inside a called Sub unwinds every caller too
                        if self.ended {
                            return Ok(Flow::Halt);
                        }
                    }
                    Flow::ExitDo | Flow::ExitFor | Flow::ExitProc | Flow::Halt => return Ok(flow),
                    Flow::Return(at) => return Ok(Flow::Return(at)),
                    Flow::Goto(l) => match find_label(&mut labels, &l) {
                        Some(j) => {
                            i = j;
                            continue;
                        }
                        None => return Ok(Flow::Goto(l)),
                    },
                    Flow::ResumeNext => {
                        i = f.err_pos + 1;
                        f.err_number = 0;
                        f.err_desc.clear();
                        continue;
                    }
                    Flow::ResumeRetry => {
                        i = f.err_pos;
                        f.err_number = 0;
                        f.err_desc.clear();
                        continue;
                    }
                    Flow::ResumeLabel(l) => return Ok(Flow::Goto(l)),
                },
                Err(e) => {
                    f.err_pos = i;
                    f.err_number = e.number;
                    f.err_desc = e.description.clone();
                    match &f.err_mode {
                        ErrMode::ResumeNext => {
                            i += 1;
                            continue;
                        }
                        ErrMode::Goto(l) => match find_label(&mut labels, &l) {
                            Some(j) => {
                                i = j;
                                continue;
                            }
                            None => return Ok(Flow::Goto(l.clone())),
                        },
                        ErrMode::None => {
                            // the line of the statement that failed: the innermost
                            // one, so an error from a called Sub keeps its own line
                            let mut e = e;
                            if e.line.is_none() {
                                e.line = Some(f.current_line);
                            }
                            return Err(e);
                        }
                    }
                }
            }
            i += 1;
        }
        Ok(Flow::Normal)
    }

    fn exec_stmt(&mut self, s: &Stmt, f: &mut Frame) -> ExecResult<Flow> {
        match s {
            Stmt::LineMarker(_) => Ok(Flow::Normal),
            // `Type` and `Enum` were registered when the program was loaded
            Stmt::TypeDecl { .. } | Stmt::EnumDecl { .. } => Ok(Flow::Normal),
            Stmt::Label(_) => Ok(Flow::Normal),
            Stmt::OptionExplicit => {
                self.option_explicit = true;
                Ok(Flow::Normal)
            }
            Stmt::OptionBase(base) => {
                self.option_base = *base;
                Ok(Flow::Normal)
            }
            Stmt::OptionCompareText(text) => {
                self.option_compare_text = *text;
                Ok(Flow::Normal)
            }
            // `DefInt A-Z` etc.: undeclared names starting with those letters
            Stmt::DefType(ranges) => {
                self.register_def_types(ranges);
                Ok(Flow::Normal)
            }
            // `Erase a`: a fixed array keeps its size with each element reset to
            // its default; a dynamic array is emptied (back to no elements).
            Stmt::Erase(targets) => {
                for t in targets {
                    let (Expr::Var(name) | Expr::Call { name, .. }) = t else {
                        return err(13, "type mismatch: Erase needs an array");
                    };
                    let Some(rc) = self.lookup_rc(f, name) else {
                        return err(9, "subscript out of range");
                    };
                    let mut borrowed = rc.borrow_mut();
                    let Variant::Array(arr) = &mut *borrowed else {
                        return err(13, "type mismatch: Erase needs an array");
                    };
                    let mut arr = arr.borrow_mut();
                    if arr.dims.is_empty() {
                        arr.data.clear();
                    } else {
                        for v in arr.data.iter_mut() {
                            *v = match v {
                                Variant::String(_) => Variant::String(String::new()),
                                Variant::Boolean(_) => Variant::Boolean(false),
                                Variant::Empty | Variant::Null | Variant::Nothing => Variant::Empty,
                                Variant::Array(_) => Variant::Empty,
                                _ => Variant::Long(0),
                            };
                        }
                    }
                }
                Ok(Flow::Normal)
            }
            // `ReDim a(n)`: a dynamic array is resized to n+1 elements. With
            // `Preserve` the existing elements are kept (up to the new size).
            Stmt::ReDim { preserve, vars } => {
                for d in vars {
                    let Some(exprs) = &d.dims else {
                        return err(9, "ReDim needs bounds");
                    };
                    let mut ds = Vec::new();
                    for e in exprs {
                        let upper = to_i64(&self.eval(e, f)?)?;
                        if upper < self.option_base {
                            return err(9, "array dimension must be >= the lower bound");
                        }
                        ds.push((upper - self.option_base + 1) as usize);
                    }
                    if ds.len() != 1 {
                        return err(9, "ReDim supports one dimension");
                    }
                    let Some(rc) = self.lookup_rc(f, &d.name) else {
                        // not yet declared: ReDim creates it
                        let arr = VbArray {
                            dims: vec![],
                            data: vec![Variant::Empty; ds[0]],
                            lower: self.option_base,
                        };
                        let rc = Rc::new(RefCell::new(Variant::Array(Rc::new(RefCell::new(arr)))));
                        let ln = d.name.to_lowercase();
                        if f.is_main {
                            self.globals.insert(ln, rc);
                        } else {
                            f.locals.insert(ln, rc);
                        }
                        continue;
                    };
                    let mut borrowed = rc.borrow_mut();
                    let Variant::Array(arr) = &mut *borrowed else {
                        return err(13, "type mismatch: ReDim needs an array");
                    };
                    let mut arr = arr.borrow_mut();
                    if !arr.dims.is_empty() {
                        return err(10, "This array is fixed or temporarily locked");
                    }
                    let keep = if *preserve { arr.data.len().min(ds[0]) } else { 0 };
                    let mut data: Vec<Variant> = arr.data.drain(..keep).collect();
                    data.resize(ds[0], Variant::Empty);
                    arr.data = data;
                }
                Ok(Flow::Normal)
            }
            // `Mid(s, start [, length]) = text`: overwrites characters of a string
            // in place, from `start` (1-based), for at most `length` characters
            // (and never past the end of the string).
            Stmt::MidAssign {
                target,
                start,
                length,
                value,
            } => {
                let s = self.eval(target, f)?.as_string()?;
                let start = to_i64(&self.eval(start, f)?)?;
                if start < 1 {
                    return err(5, "invalid procedure call: Mid start < 1");
                }
                let text = self.eval(value, f)?.as_string()?;
                let mut chars: Vec<char> = s.chars().collect();
                let from = (start - 1) as usize;
                if from >= chars.len() {
                    return Ok(Flow::Normal);
                }
                let max = match length {
                    Some(l) => to_i64(&self.eval(l, f)?)?.max(0) as usize,
                    None => usize::MAX,
                };
                let count = max.min(text.chars().count()).min(chars.len() - from);
                for (i, c) in text.chars().take(count).enumerate() {
                    chars[from + i] = c;
                }
                let result: String = chars.into_iter().collect();
                self.assign(target, Variant::String(result), f)?;
                Ok(Flow::Normal)
            }
            Stmt::End => {
                self.ended = true;
                Ok(Flow::Halt)
            }
            Stmt::Dim(decls) => {
                for d in decls {
                    let dims = match &d.dims {
                        None => None,
                        Some(exprs) => {
                            let mut ds = Vec::new();
                            for e in exprs {
                                let v = self.eval(e, f)?;
                                let upper = to_i64(&v)?;
                                // `Dim a(10)` spans 0..10 (1..10 under Option Base 1)
                                if upper < self.option_base {
                                    return err(9, "array dimension must be >= the lower bound");
                                }
                                ds.push((upper - self.option_base + 1) as usize);
                            }
                            Some(ds)
                        }
                    };
                    self.declare_var(f, &d.name, dims, &d.ty)?;
                }
                Ok(Flow::Normal)
            }
            Stmt::Const(decls) => {
                for d in decls {
                    let v = self.eval(&d.value, f)?;
                    self.declare_var(f, &d.name, None, &None)?;
                    self.set_var(f, &d.name, v)?;
                }
                Ok(Flow::Normal)
            }
            Stmt::Assign { target, value } => {
                // `Set Option1.Container = Frame1`: the value is an object of
                // the host, passed as its name (`Frame1`, `Frame1(2)`)
                let v = match self.object_value(target, value, f)? {
                    Some(v) => v,
                    None => self.eval(value, f)?,
                };
                self.assign(target, v, f)?;
                Ok(Flow::Normal)
            }
            Stmt::If { arms, else_body } => {
                for arm in arms {
                    if to_bool(&self.eval(&arm.cond, f)?)? {
                        return self.exec_block(&arm.body, f);
                    }
                }
                match else_body {
                    Some(b) => self.exec_block(b, f),
                    None => Ok(Flow::Normal),
                }
            }
            Stmt::Select {
                expr,
                arms,
                else_body,
            } => {
                let subject = self.eval(expr, f)?;
                for arm in arms {
                    let mut hit = false;
                    for item in &arm.items {
                        hit = self.case_matches(&subject, item, f)?;
                        if hit {
                            break;
                        }
                    }
                    if hit {
                        return self.exec_block(&arm.body, f);
                    }
                }
                match else_body {
                    Some(b) => self.exec_block(b, f),
                    None => Ok(Flow::Normal),
                }
            }
            Stmt::Exit(kind) => Ok(match kind {
                ExitKind::Do => Flow::ExitDo,
                ExitKind::For => Flow::ExitFor,
                ExitKind::Sub | ExitKind::Function | ExitKind::Property => Flow::ExitProc,
            }),
            Stmt::GoTo(l) => Ok(Flow::Goto(l.clone())),
            // `GoSub label` runs the code from `label` in the frame's body, up
            // to the matching `Return`, then continues after the GoSub. It is a
            // call, so it works from inside a loop or an If, as in VB.
            Stmt::GoSub(l) => {
                let body = f.body.clone();
                let start = body.iter().position(|s| {
                    matches!(s, Stmt::Label(x) if x.eq_ignore_ascii_case(l))
                });
                let Some(start) = start else {
                    return err(35, &format!("Label {} not defined", l));
                };
                f.gosub_depth += 1;
                let flow = self.exec_block(&body[start..], f);
                f.gosub_depth -= 1;
                match flow? {
                    // a Return ends the subroutine and resumes the caller
                    Flow::Return(_) | Flow::Normal => Ok(Flow::Normal),
                    // anything else (End, an error goto, a GoTo out) carries on
                    // to the caller unchanged
                    other => Ok(other),
                }
            }
            // `Return` ends the subroutine it is in: the GoSub that called it
            // sees Flow::Return and carries on after itself. With no GoSub
            // running in this frame there is nothing to return to.
            Stmt::Return => {
                if f.gosub_depth == 0 {
                    return err(3, "Return without GoSub");
                }
                Ok(Flow::Return(0))
            }


            // Stop: a debugger pause at the next statement. Outside the IDE
            // there is no debugger and nothing happens, as in VB.
            Stmt::Stop => {
                if self.debug_hook.is_some() {
                    self.step_depth = Some(self.depth);
                }
                Ok(Flow::Normal)
            }
            Stmt::OnError(mode) => {
                f.err_mode = match mode {
                    OnErrorMode::GoTo(l) => ErrMode::Goto(l.clone()),
                    OnErrorMode::GoToZero => ErrMode::None,
                    OnErrorMode::ResumeNext => ErrMode::ResumeNext,
                };
                Ok(Flow::Normal)
            }
            Stmt::Resume(kind) => Ok(match kind {
                ResumeKind::Retry => Flow::ResumeRetry,
                ResumeKind::Next => Flow::ResumeNext,
                ResumeKind::Label(l) => Flow::ResumeLabel(l.clone()),
            }),
            Stmt::Print {
                items,
                trailing_sep,
            } => {
                self.exec_print(items, *trailing_sep, f)?;
                Ok(Flow::Normal)
            }
            Stmt::OpenFile {
                path,
                mode,
                number,
                record_len,
            } => {
                let name = self.eval(path, f)?.as_string()?;
                let n = to_i64(&self.eval(number, f)?)?;
                let len = match record_len {
                    Some(e) => Some(to_i64(&self.eval(e, f)?)?),
                    None => None,
                };
                let full = match &self.base_dir {
                    Some(dir) => dir.join(&name),
                    None => std::path::PathBuf::from(&name),
                };
                self.files.open(n, &name, &full, *mode, len)?;
                Ok(Flow::Normal)
            }
            Stmt::FileSeek { number, pos } => {
                let n = to_i64(&self.eval(number, f)?)?;
                let p = to_i64(&self.eval(pos, f)?)?;
                self.files.seek(n, p)?;
                Ok(Flow::Normal)
            }
            Stmt::FileGet {
                number,
                pos,
                target,
            } => {
                let n = to_i64(&self.eval(number, f)?)?;
                let recnum = match pos {
                    Some(e) => Some(to_i64(&self.eval(e, f)?)?),
                    None => None,
                };
                let current = self.eval(target, f)?;
                // a binary Get reads the bytes the variable takes; a random one
                // reads a whole record
                let size = match &current {
                    Variant::Record(r) => Some(self.record_bytes(r)?.len()),
                    Variant::String(s) => match target {
                        // a fixed-length string reads its declared size
                        Expr::Var(name) => match self.declared_type(f, name).as_deref().and_then(crate::vbtypes::file_type_of) {
                            Some(FileType::FixedString(n)) => Some(n),
                            _ => Some(s.len()),
                        },
                        _ => Some(s.len()),
                    },
                    Variant::Integer(_) => Some(2),
                    Variant::Long(_) | Variant::Single(_) => Some(4),
                    Variant::Double(_) | Variant::Currency(_) | Variant::Date(_) => Some(8),
                    Variant::Boolean(_) => Some(2),
                    _ => None,
                };
                let bytes = self.files.get_record(n, recnum, size)?;
                let value = match current {
                    // a user Type: decode each field in order into the record
                    Variant::Record(r) => {
                        let mut at = 0usize;
                        Variant::Record(self.record_from_bytes(&r.ty, &bytes, &mut at)?)
                    }
                    // a scalar: decoded as the variable's own type
                    other => {
                        let ty = match &other {
                            Variant::Integer(_) => FileType::Integer,
                            Variant::Long(_) => FileType::Long,
                            Variant::Single(_) => FileType::Single,
                            Variant::Double(_) => FileType::Double,
                            Variant::Currency(_) => FileType::Currency,
                            Variant::Date(_) => FileType::Date,
                            Variant::Boolean(_) => FileType::Boolean,
                            // a fixed-length string is its declared size, not length-prefixed
                            Variant::String(_) => match size {
                                Some(n) if matches!(target, Expr::Var(nm) if matches!(self.declared_type(f, nm).as_deref().and_then(crate::vbtypes::file_type_of), Some(FileType::FixedString(_)))) => {
                                    FileType::FixedString(n)
                                }
                                _ => FileType::String,
                            },
                            _ => FileType::String,
                        };
                        let mut at = 0usize;
                        decode_value(&ty, &bytes, &mut at)?
                    }
                };
                self.assign(target, value, f)?;
                Ok(Flow::Normal)
            }
            Stmt::FilePut {
                number,
                pos,
                value,
            } => {
                let n = to_i64(&self.eval(number, f)?)?;
                let recnum = match pos {
                    Some(e) => Some(to_i64(&self.eval(e, f)?)?),
                    None => None,
                };
                let bytes = match self.eval(value, f)? {
                    // a user Type: its fields in order, in VB's layout
                    Variant::Record(r) => self.record_bytes(&r)?,
                    // a scalar: its own VB type; a fixed-length string variable
                    // writes its declared size, with no length prefix
                    other => {
                        let ty = match &other {
                            Variant::Integer(_) => FileType::Integer,
                            Variant::Long(_) => FileType::Long,
                            Variant::Single(_) => FileType::Single,
                            Variant::Double(_) => FileType::Double,
                            Variant::Currency(_) => FileType::Currency,
                            Variant::Date(_) => FileType::Date,
                            Variant::Boolean(_) => FileType::Boolean,
                            Variant::String(_) => match value {
                                Expr::Var(nm) => match self.declared_type(f, nm).as_deref().and_then(crate::vbtypes::file_type_of) {
                                    Some(FileType::FixedString(n)) => FileType::FixedString(n),
                                    _ => FileType::String,
                                },
                                _ => FileType::String,
                            },
                            _ => FileType::String,
                        };
                        encode_value(&ty, &other)?
                    }
                };
                self.files.put_record(n, recnum, &bytes)?;
                Ok(Flow::Normal)
            }

            Stmt::CloseFile(numbers) => {
                if numbers.is_empty() {
                    self.files.close_all()?;
                } else {
                    for e in numbers {
                        let n = to_i64(&self.eval(e, f)?)?;
                        self.files.close(n)?;
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::LineInputFile { number, target } => {
                let n = to_i64(&self.eval(number, f)?)?;
                let line = self.files.read_line(n)?;
                self.assign(target, Variant::String(line), f)?;
                Ok(Flow::Normal)
            }
            Stmt::InputFile { number, targets } => {
                let n = to_i64(&self.eval(number, f)?)?;
                // VB's Input # reads comma-separated fields; a line may hold
                // several values, and a value may be quoted
                for target in targets {
                    let v = self.next_input_value(n)?;
                    self.assign(target, v, f)?;
                }
                Ok(Flow::Normal)
            }
            Stmt::PrintFile {
                number,
                items,
                trailing_sep,
                write,
            } => {
                let n = to_i64(&self.eval(number, f)?)?;
                let text = self.file_text(items, *trailing_sep, *write, f)?;
                self.files.write_line(n, &text)?;
                Ok(Flow::Normal)
            }
            Stmt::Graphics { target, shape } => {
                self.exec_graphics(target.as_ref(), shape, f)?;
                Ok(Flow::Normal)
            }
            Stmt::Call { target, args, .. } => {
                self.exec_call(target, args, f)?;
                Ok(Flow::Normal)
            }
            _ => self.exec_loop_stmt(s, f),
        }
    }

    fn case_matches(
        &mut self,
        subject: &Variant,
        item: &CaseItem,
        f: &mut Frame,
    ) -> ExecResult<bool> {
        match item {
            CaseItem::Value(e) => {
                let v = self.eval(e, f)?;
                values_eq(subject, &v, self.option_compare_text)
            }
            CaseItem::Range(lo, hi) => {
                let l = self.eval(lo, f)?;
                let h = self.eval(hi, f)?;
                let t = self.option_compare_text;
                Ok(compare(subject, &l, BinaryOp::Ge, t)? && compare(subject, &h, BinaryOp::Le, t)?)
            }
            CaseItem::Is(op, e) => {
                let v = self.eval(e, f)?;
                compare(subject, &v, *op, self.option_compare_text)
            }
        }
    }
}

/// Loop statements, split out to keep `exec_stmt` manageable.
impl<'out> Interpreter<'out> {
    fn exec_loop_stmt(&mut self, s: &Stmt, f: &mut Frame) -> ExecResult<Flow> {
        match s {
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
            } => {
                let sv = self.eval(start, f)?;
                let ev = self.eval(end, f)?;
                let stv = match step {
                    Some(e) => self.eval(e, f)?,
                    None => Variant::Long(1),
                };
                let s0 = to_f64(&sv)?;
                let e0 = to_f64(&ev)?;
                let st = to_f64(&stv)?;
                if st == 0.0 {
                    return err(92, "For loop step cannot be zero");
                }
                let integral = is_integral(&sv) && is_integral(&ev) && is_integral(&stv);
                let mut i = s0;
                while (st >= 0.0 && i <= e0) || (st < 0.0 && i >= e0) {
                    let iv = if integral {
                        Variant::Long(i as i32)
                    } else {
                        Variant::Double(i)
                    };
                    self.set_var(f, var, iv)?;
                    match self.exec_block(body, f)? {
                        Flow::Normal => {}
                        Flow::ExitFor => break,
                        flow => return Ok(flow),
                    }
                    i += st;
                }
                Ok(Flow::Normal)
            }
            Stmt::ForEach { var, coll, body } => {
                let cv = self.eval(coll, f)?;
                let items: Vec<Variant> = match &cv {
                    Variant::Array(a) => a.borrow().data.clone(),
                    _ => return err(92, "For Each requires an array or collection"),
                };
                for item in items {
                    self.set_var(f, var, item)?;
                    match self.exec_block(body, f)? {
                        Flow::Normal => {}
                        Flow::ExitFor => break,
                        flow => return Ok(flow),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::DoWhile { cond, body, until } => {
                loop {
                    let c = to_bool(&self.eval(cond, f)?)?;
                    if *until == c {
                        break;
                    }
                    match self.exec_block(body, f)? {
                        Flow::Normal => {}
                        Flow::ExitDo => break,
                        flow => return Ok(flow),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::DoLoop { body, cond } => {
                loop {
                    match self.exec_block(body, f)? {
                        Flow::Normal => {}
                        Flow::ExitDo => break,
                        flow => return Ok(flow),
                    }
                    if let Some((c, until)) = cond {
                        let v = to_bool(&self.eval(c, f)?)?;
                        if *until == v {
                            break;
                        }
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::While { cond, body } => {
                while to_bool(&self.eval(cond, f)?)? {
                    match self.exec_block(body, f)? {
                        Flow::Normal => {}
                        Flow::ExitDo => break,
                        flow => return Ok(flow),
                    }
                }
                Ok(Flow::Normal)
            }
            // `With obj ... End With`: `.Member` inside means a member of obj.
            // The object is pushed for the body and popped however it ends.
            Stmt::With { obj, body } => {
                self.with_stack.push(obj.clone());
                let result = self.exec_block(body, f);
                self.with_stack.pop();
                result
            }
            _ => err(5, "internal: unhandled statement"),
        }
    }
}

// ---------------------------------------------------------------------------
// Assignment, Print, Calls
// ---------------------------------------------------------------------------

impl<'out> Interpreter<'out> {
    fn assign(&mut self, target: &Expr, v: Variant, f: &mut Frame) -> ExecResult<()> {
        // `.Caption = x` inside a With block: a member of the With object
        if let Expr::WithRef(name) = target {
            let member = self.resolve_with(name)?;
            return self.assign(&member, v, f);
        }
        match target {
            Expr::Var(name) => {
                let lowered = Lowered::new(name);
                let ln = lowered.as_str();
                if let Some(rn) = &f.ret_name {
                    if rn == ln {
                        f.ret = v;
                        return Ok(());
                    }
                }
                self.set_var(f, name, v)
            }
            Expr::Call { name, args } => {
                // Array element store: a(i) = v
                let mut idxs = Vec::new();
                for a in args {
                    idxs.push(to_i64(&self.eval(a, f)?)?);
                }
                let rc = match self.lookup_rc(f, name) {
                    Some(rc) => rc,
                    None => {
                        if self.option_explicit {
                            return err(500, &format!("Variable not defined: {}", name));
                        }
                        // auto-create a dynamic array
                        let rc = Rc::new(RefCell::new(Variant::Array(Rc::new(RefCell::new(
                            VbArray {
                                dims: vec![],
                                data: vec![],
                                lower: self.option_base,
                            },
                        )))));
                        let ln = name.to_lowercase();
                        if f.is_main {
                            self.globals.insert(ln, rc.clone());
                        } else {
                            f.locals.insert(ln, rc.clone());
                        }
                        rc
                    }
                };
                let ty = self.declared_type(f, name);
                let v = match &ty {
                    Some(t) => coerce_typed(&v, t)?,
                    None => v,
                };
                let mut borrowed = rc.borrow_mut();
                if let Variant::Array(arr) = &mut *borrowed {
                    let mut arr = arr.borrow_mut();
                    if arr.dims.is_empty() {
                        // dynamic auto-grow (1-D)
                        if idxs.len() != 1 {
                            return err(9, "dynamic arrays take exactly one subscript");
                        }
                        let ix = idxs[0] - arr.lower;
                        if ix < 0 {
                            return err(9, "subscript out of range");
                        }
                        while arr.data.len() <= ix as usize {
                            arr.data.push(Variant::Empty);
                        }
                        arr.data[ix as usize] = v;
                        Ok(())
                    } else {
                        let flat = arr.flat_index(&idxs)?;
                        arr.data[flat] = v;
                        Ok(())
                    }
                } else {
                    err(13, "type mismatch: not an array")
                }
            }
            Expr::Member { obj, name } if self.is_record_place(obj, f) => {
                // `p.X = v` on a user-defined Type: store into the record
                self.record_field_store(obj, name, v, f)
            }
            Expr::Member { obj, name } => match self.object_ref(obj, f)? {
                Some((object, prefix)) => {
                    let host = self.objects.clone().expect("object_ref needs a host");
                    let property = format!("{}{}", prefix, name);
                    let result = host.borrow_mut().set(&object, &property, &[], v);
                    result?;
                    self.run_raised_events()
                }
                None => err(13, "type mismatch: cannot assign to this expression"),
            },
            // `b.Vals(1) = v` on a record whose field is an array
            Expr::MethodCall { obj, name, args } if self.is_record_place(obj, f) => {
                let mut idxs = Vec::with_capacity(args.len());
                for a in args {
                    idxs.push(to_i64(&self.eval(a, f)?)?);
                }
                self.with_record(obj, f, |r| {
                    let Some((_, Variant::Array(arr))) =
                        r.fields.iter().find(|(n, _)| n.eq_ignore_ascii_case(name))
                    else {
                        return err(438, &format!("object doesn't support this property: {}", name));
                    };
                    let mut arr = arr.borrow_mut();
                    let flat = arr.flat_index(&idxs)?;
                    arr.data[flat] = v;
                    Ok(())
                })?
            }
            // `List1.List(0) = "x"`: a parameterised property
            Expr::MethodCall { obj, name, args } => match self.object_ref(obj, f)? {
                Some((object, prefix)) => {
                    let mut idx = Vec::with_capacity(args.len());
                    for a in args {
                        idx.push(self.eval(a, f)?);
                    }
                    let host = self.objects.clone().expect("object_ref needs a host");
                    let property = format!("{}{}", prefix, name);
                    let result = host.borrow_mut().set(&object, &property, &idx, v);
                    result?;
                    self.run_raised_events()
                }
                None => err(13, "type mismatch: cannot assign to this expression"),
            },
            _ => err(13, "type mismatch: cannot assign to this expression"),
        }
    }

    /// For an assignment to a host property that holds an object
    /// (`Container`), a value naming a host object (`Frame1`, `Frame1(2)`)
    /// is passed as that object's name: VB's `Set Text1.Container = Frame1`.
    /// `None` for every other assignment, which evaluates its value.
    fn object_value(
        &mut self,
        target: &Expr,
        value: &Expr,
        f: &mut Frame,
    ) -> ExecResult<Option<Variant>> {
        let to_host = matches!(target, Expr::Member { obj, name }
            if name.eq_ignore_ascii_case("container") && self.object_ref(obj, f)?.is_some());
        if !to_host {
            return Ok(None);
        }
        Ok(match self.object_ref_expr(value, f)? {
            Some(ObjectRef {
                name,
                index: Some(i),
            }) => Some(Variant::String(format!("{}({})", name, i))),
            Some(ObjectRef { name, index: None }) => Some(Variant::String(name)),
            None => None,
        })
    }

    /// `value` itself as a host object (`Frame1`, `Cmd(2)`), if it is one.
    fn object_ref_expr(&mut self, value: &Expr, f: &mut Frame) -> ExecResult<Option<ObjectRef>> {
        Ok(match value {
            Expr::Var(_) | Expr::Call { .. } => self.object_ref(value, f)?.map(|(o, _)| o),
            _ => None,
        })
    }

    /// Whether `name` is a host object the program can reach: the host
    /// knows it, and no variable of that name hides it.
    fn is_host_object(&self, name: &str, f: &Frame) -> bool {
        match &self.objects {
            Some(host) => self.lookup_rc(f, name).is_none() && host.borrow().has_object(name),
            None => false,
        }
    }

    /// Resolve `obj`, the part before a `.Member`, to a host object and the
    /// property prefix of a sub-object: `Label1` -> (`Label1`, ""),
    /// `Command1(i)` -> (`Command1` element `i`, ""), `Label1.Font` ->
    /// (`Label1`, "Font."). `None` when `obj` is not a host object.
    /// `.name` inside a `With` block: the member `name` of the innermost With
    /// object. Outside any `With` block it is an error, as in VB.
    fn resolve_with(&self, name: &str) -> ExecResult<Expr> {
        match self.with_stack.last() {
            Some(obj) => Ok(Expr::Member {
                obj: Box::new(obj.clone()),
                name: name.to_string(),
            }),
            None => err(
                91,
                "a leading '.' needs a With block (Object variable or With block variable not set)",
            ),
        }
    }

    fn object_ref(&mut self, obj: &Expr, f: &mut Frame) -> ExecResult<Option<(ObjectRef, String)>> {
        match obj {
            // `.Label1` with a With block is `Label1.Caption`'s object part
            Expr::WithRef(base) => {
                let resolved = self.resolve_with(base)?;
                self.object_ref(&resolved, f)
            }
            Expr::Var(base) if self.is_host_object(base, f) => Ok(Some((
                ObjectRef {
                    name: base.clone(),
                    index: None,
                },
                String::new(),
            ))),
            // a control array element (a procedure of that name wins)
            Expr::Call { name, args }
                if args.len() == 1
                    && !self.proc_index.contains_key(&name.to_lowercase())
                    && self.is_host_object(name, f) =>
            {
                let index = to_i32(&self.eval(&args[0], f)?)?;
                Ok(Some((
                    ObjectRef {
                        name: name.clone(),
                        index: Some(index),
                    },
                    String::new(),
                )))
            }
            Expr::Member { obj: inner, name } => Ok(self
                .object_ref(inner, f)?
                .map(|(object, prefix)| (object, format!("{}{}.", prefix, name)))),
            _ => Ok(None),
        }
    }

    /// `Line`, `PSet` or `Circle` on `target` (the form whose code runs when
    /// `None`): its coordinates and colour worked out, then handed to the
    /// host. A `Step` point counts from `CurrentX`/`CurrentY`.
    fn exec_graphics(
        &mut self,
        target: Option<&Expr>,
        shape: &Graphic,
        f: &mut Frame,
    ) -> ExecResult<()> {
        let Some(host) = self.objects.clone() else {
            return err(438, "object doesn't support this property or method");
        };
        let object = match target {
            Some(t) => match self.object_ref(t, f)? {
                Some((o, prefix)) if prefix.is_empty() => Some(o),
                // `Form2.Picture1`: the form's control, named by its path
                Some((o, prefix)) => Some(ObjectRef {
                    name: format!("{}.{}", o.name, prefix.trim_end_matches('.')),
                    index: o.index,
                }),
                None => return err(424, "object required"),
            },
            None => None,
        };
        let here = host.borrow().current_point(object.as_ref());
        let num = |me: &mut Self, e: &Expr, f: &mut Frame| -> ExecResult<f32> {
            Ok(to_f64(&me.eval(e, f)?)? as f32)
        };
        let point = |me: &mut Self,
                     p: &GPoint,
                     from: (f32, f32),
                     f: &mut Frame|
         -> ExecResult<(f32, f32)> {
            let (x, y) = (num(me, &p.x, f)?, num(me, &p.y, f)?);
            Ok(if p.step {
                (from.0 + x, from.1 + y)
            } else {
                (x, y)
            })
        };
        let color = |me: &mut Self, c: &Option<Expr>, f: &mut Frame| -> ExecResult<Option<i64>> {
            match c {
                Some(e) => Ok(Some(to_i64(&me.eval(e, f)?)?)),
                None => Ok(None),
            }
        };
        // colours arrive as VB Longs; the host makes them its own
        let cmd = match shape {
            Graphic::Line {
                from,
                to,
                color: c,
                boxed,
            } => {
                let a = match from {
                    Some(p) => point(self, p, here, f)?,
                    None => here,
                };
                let b = point(self, to, a, f)?;
                DrawCmd::Line {
                    from: a,
                    to: b,
                    color: color(self, c, f)?.map(|v| v as u32),
                    boxed: *boxed,
                }
            }
            Graphic::PSet { at, color: c } => DrawCmd::PSet {
                at: point(self, at, here, f)?,
                color: color(self, c, f)?.map(|v| v as u32),
            },
            Graphic::Circle {
                center,
                radius,
                color: c,
                start,
                end,
                aspect,
            } => {
                let center = point(self, center, here, f)?;
                let radius = num(self, radius, f)?;
                let opt =
                    |me: &mut Self, e: &Option<Expr>, f: &mut Frame| -> ExecResult<Option<f32>> {
                        match e {
                            Some(e) => Ok(Some(num(me, e, f)?)),
                            None => Ok(None),
                        }
                    };
                DrawCmd::Circle {
                    center,
                    radius,
                    color: color(self, c, f)?.map(|v| v as u32),
                    start: opt(self, start, f)?,
                    end: opt(self, end, f)?,
                    aspect: opt(self, aspect, f)?.unwrap_or(1.0),
                }
            }
        };
        let result = host.borrow_mut().draw(object.as_ref(), cmd);
        result
    }

    /// The next value of an `Input #n` read: fields are comma-separated on a
    /// line, a quoted field has its quotes removed, and a line's fields are
    /// used up before the next line is read (error 62 at the end).
    fn next_input_value(&mut self, n: i64) -> ExecResult<Variant> {
        let field = self.files.next_field(n)?;
        Ok(match field {
            Field::Text(s) => Variant::String(s),
            Field::Number(s) => match s.parse::<f64>() {
                Ok(x) => Variant::Double(x),
                Err(_) => Variant::String(s),
            },
        })
    }

    /// The text `Print #n` or `Write #n` writes for `items`: as `Print`
    /// lays them out, or, for `Write`, strings quoted and items comma-separated.
    fn file_text(
        &mut self,
        items: &[PrintItem],
        trailing_sep: bool,
        write: bool,
        f: &mut Frame,
    ) -> ExecResult<String> {
        if !write {
            // the same layout as `Print`, minus the screen's column stops
            let mut line = String::new();
            for item in items {
                let v = self.eval(&item.expr, f)?;
                line.push_str(&to_string(&v)?);
                if item.sep == Some(',') {
                    line.push(',');
                }
            }
            return Ok(line);
        }
        let mut parts: Vec<String> = Vec::new();
        for item in items {
            let v = self.eval(&item.expr, f)?;
            parts.push(match v {
                Variant::String(s) => format!("\"{}\"", s),
                other => to_string(&other)?,
            });
        }
        let _ = trailing_sep;
        Ok(parts.join(","))
    }

    fn exec_print(
        &mut self,
        items: &[PrintItem],
        trailing_sep: bool,
        f: &mut Frame,
    ) -> ExecResult<()> {
        let mut line = String::new();
        for item in items {
            let v = self.eval(&item.expr, f)?;
            line.push_str(&print_repr(&v)?);
            match item.sep {
                Some(';') => {}
                Some(',') => {
                    while line.len() % 14 != 0 {
                        line.push(' ');
                    }
                }
                _ => {}
            }
        }
        if !trailing_sep {
            line.push('\n');
        }
        self.out.write(&line);
        Ok(())
    }

    fn exec_call(&mut self, target: &Expr, args: &[Expr], f: &mut Frame) -> ExecResult<()> {
        match target {
            Expr::Member { obj, name } => {
                self.call_member(obj, name, args, f)?;
            }
            Expr::Var(name) => {
                // `Unload Me` / `Load Form2`: the argument names an object,
                // it isn't a value
                let lower = name.to_ascii_lowercase();
                if (lower == "unload" || lower == "load") && args.len() == 1 {
                    // `Load Cmd(3)` / `Unload Cmd(3)`: a new or removed
                    // element of control array `Cmd`
                    if let Expr::Call {
                        name: array,
                        args: idx,
                    } = &args[0]
                    {
                        if idx.len() == 1
                            && !self.proc_index.contains_key(&array.to_lowercase())
                            && self.is_host_object(array, f)
                        {
                            let n = to_i32(&self.eval(&idx[0], f)?)?;
                            let host = self.objects.clone().expect("is_host_object needs a host");
                            let object = ObjectRef {
                                name: array.clone(),
                                index: Some(n),
                            };
                            let method = if lower == "unload" { "Unload" } else { "Load" };
                            let result = host.borrow_mut().call(&object, method, &[]);
                            result?;
                            return self.run_raised_events();
                        }
                    }
                    if let Expr::Var(target) = &args[0] {
                        if self.is_host_object(target, f) {
                            let host = self.objects.clone().expect("is_host_object needs a host");
                            let object = ObjectRef {
                                name: target.clone(),
                                index: None,
                            };
                            if lower == "unload" && !self.query_unload(&object)? {
                                return Ok(()); // a handler set Cancel
                            }
                            let method = if lower == "unload" { "Unload" } else { "Load" };
                            let result = host.borrow_mut().call(&object, method, &[]);
                            result?;
                            return self.run_raised_events();
                        }
                    }
                }
                self.call_proc(name, args, f)?;
            }
            other => {
                self.eval(other, f)?;
            }
        }
        Ok(())
    }

    /// `obj.Member` / `obj.Method(args)`: a host object's property or
    /// method (a form control), or the `Err` object (`Err.Number`,
    /// `Err.Description`, `Err.Clear`, `Err.Raise`).
    fn call_member(
        &mut self,
        obj: &Expr,
        name: &str,
        args: &[Expr],
        f: &mut Frame,
    ) -> ExecResult<Variant> {
        // `b.Vals(1)` on a record whose field is an array: read the element
        if !args.is_empty() && self.is_record_place(obj, f) {
            let mut idxs = Vec::with_capacity(args.len());
            for a in args {
                idxs.push(to_i64(&self.eval(a, f)?)?);
            }
            return self.with_record(obj, f, |r| {
                match r.fields.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
                    Some((_, Variant::Array(arr))) => {
                        let arr = arr.borrow();
                        let flat = arr.flat_index(&idxs)?;
                        Ok(arr.data[flat].clone())
                    }
                    _ => err(438, &format!("object doesn't support this property: {}", name)),
                }
            })?;
        }
        // `.Refresh` inside a With block: a call on the With object
        if let Expr::WithRef(base) = obj {
            let with_obj = self.resolve_with(base)?;
            return self.call_member(&with_obj, name, args, f);
        }
        if let Some((object, prefix)) = self.object_ref(obj, f)? {
            let mut values = Vec::with_capacity(args.len());
            for a in args {
                values.push(self.eval(a, f)?);
            }
            let host = self.objects.clone().expect("object_ref needs a host");
            // a method (`AddItem`, `SetFocus`) or a parameterised property
            // (`List(0)`) — the host decides which
            let method = format!("{}{}", prefix, name);
            let result = host.borrow_mut().call(&object, &method, &values);
            let value = result?;
            self.run_raised_events()?;
            return Ok(value);
        }
        if let Expr::Var(base) = obj {
            if base.eq_ignore_ascii_case("err") {
                match name.to_ascii_lowercase().as_str() {
                    "clear" => {
                        f.err_number = 0;
                        f.err_desc.clear();
                        return Ok(Variant::Empty);
                    }
                    "raise" => {
                        let n = match args.first() {
                            Some(e) => to_i32(&self.eval(e, f)?)?,
                            None => 5,
                        };
                        let d = match args.get(1) {
                            Some(e) => to_string(&self.eval(e, f)?)?,
                            None => format!("application-defined error {}", n),
                        };
                        return err(n, &d);
                    }
                    "number" => return Ok(Variant::Long(f.err_number)),
                    "description" => return Ok(Variant::String(f.err_desc.clone())),
                    "source" => return Ok(Variant::String("OxidizedBASIC".into())),
                    _ => return err(438, "object doesn't support this property or method"),
                }
            }
        }
        err(
            424,
            "object required (COM/class objects arrive in a later phase)",
        )
    }
}

/// Call a user procedure or builtin. Returns the function value.
impl<'out> Interpreter<'out> {
    pub(crate) fn call_proc(
        &mut self,
        name: &str,
        arg_exprs: &[Expr],
        f: &mut Frame,
    ) -> ExecResult<Variant> {
        let ln = name.to_lowercase();
        if let Some(&idx) = self.proc_index.get(&ln) {
            if self.depth > 400 {
                return err(28, "out of stack space");
            }
            let decl = self.decls[idx].clone();
            let mut callee = Frame::proc(&decl);
            let nparams = decl.params.len();
            let nargs = arg_exprs.len();
            if nargs > nparams && !decl.params.iter().any(|p| p.param_array) {
                return err(450, &format!("wrong number of arguments for {}", name));
            }
            for (i, p) in decl.params.iter().enumerate() {
                if p.param_array {
                    let mut items = Vec::new();
                    for e in &arg_exprs[i.min(nargs)..] {
                        items.push(self.eval(e, f)?);
                    }
                    callee.locals.insert(
                        p.name.to_lowercase(),
                        Rc::new(RefCell::new(Variant::Array(Rc::new(RefCell::new(
                            VbArray {
                                dims: vec![items.len()],
                                data: items,
                                lower: 0,
                            },
                        ))))),
                    );
                    break;
                }
                if i >= nargs {
                    if !p.optional {
                        return err(449, &format!("argument not optional: {}", p.name));
                    }
                    let v = match &p.default {
                        Some(d) => self.eval(d, f)?,
                        None => Variant::Empty,
                    };
                    callee
                        .locals
                        .insert(p.name.to_lowercase(), Rc::new(RefCell::new(v)));
                    if let Some(t) = &p.ty {
                        callee
                            .local_types
                            .insert(p.name.to_lowercase(), t.to_lowercase());
                    }
                    continue;
                }
                let arg = &arg_exprs[i];
                let shared = if p.by_ref {
                    if let Expr::Var(vn) = arg {
                        match self.lookup_rc(f, vn) {
                            Some(rc) => Some(rc),
                            None => {
                                if self.option_explicit {
                                    return err(500, &format!("Variable not defined: {}", vn));
                                }
                                let rc = Rc::new(RefCell::new(Variant::Empty));
                                let l = vn.to_lowercase();
                                if f.is_main {
                                    self.globals.insert(l, rc.clone());
                                } else {
                                    f.locals.insert(l, rc.clone());
                                }
                                Some(rc)
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                match shared {
                    Some(rc) => {
                        callee.locals.insert(p.name.to_lowercase(), rc);
                    }
                    None => {
                        let v = self.eval(arg, f)?;
                        let v = match &p.ty {
                            Some(t) => coerce_typed(&v, t)?,
                            None => v,
                        };
                        callee
                            .locals
                            .insert(p.name.to_lowercase(), Rc::new(RefCell::new(v)));
                    }
                }
                if let Some(t) = &p.ty {
                    callee
                        .local_types
                        .insert(p.name.to_lowercase(), t.to_lowercase());
                }
            }
            self.depth += 1;
            let body = decl.body.clone();
            let prev = self.enter_context(&decl.name);
            let flow = self.exec_block(&body, &mut callee);
            self.leave_context(prev);
            self.depth -= 1;
            flow?;
            Ok(callee.ret)
        } else {
            // builtin function
            let mut vals = Vec::new();
            for e in arg_exprs {
                vals.push(self.eval(e, f)?);
            }
            self.builtin(&ln, &vals, f)
        }
    }
}

// ---------------------------------------------------------------------------
// Expression evaluation
// ---------------------------------------------------------------------------

impl<'out> Interpreter<'out> {
    pub(crate) fn eval(&mut self, e: &Expr, f: &mut Frame) -> ExecResult<Variant> {
        match e {
            Expr::Lit(l) => Ok(match l {
                Lit::Int(v) => Variant::Long(*v),
                Lit::Double(v) => Variant::Double(*v),
                Lit::Str(s) => Variant::String(s.clone()),
                Lit::Bool(b) => Variant::Boolean(*b),
                Lit::Empty => Variant::Empty,
                Lit::Null => Variant::Null,
                Lit::Nothing => Variant::Nothing,
            }),
            Expr::Var(name) => self.eval_var(name, f),
            // `.Caption` inside a With block: read the member of the With object
            Expr::WithRef(name) => {
                let member = self.resolve_with(name)?;
                self.eval(&member, f)
            }
            Expr::Call { name, args } => {
                // Array element read or procedure call.
                let ln = name.to_lowercase();
                if !self.proc_index.contains_key(&ln) && !stdlib::is_builtin(&ln) && args.len() == 1
                {
                    if let Some(rc) = self.lookup_rc(f, name) {
                        let borrowed = rc.borrow();
                        if let Variant::Array(arr) = &*borrowed {
                            let ix = to_i64(&self.eval(&args[0], f)?)?;
                            let arr = arr.borrow();
                            if arr.dims.is_empty() {
                                return Ok(arr.data[arr.index_1d(ix)?].clone());
                            }
                            let flat = arr.flat_index(&[ix])?;
                            return Ok(arr.data[flat].clone());
                        }
                    }
                }
                self.call_proc(name, args, f)
            }
            Expr::Member { obj, name } => {
                // a field of a user-defined Type (`p.X`), read from its record
                if let Some(v) = self.record_field_value(obj, name, f)? {
                    return Ok(v);
                }
                // a host object's property read (`x = Label1.Caption`)
                if let Some((object, prefix)) = self.object_ref(obj, f)? {
                    let host = self.objects.clone().expect("object_ref needs a host");
                    let property = format!("{}{}", prefix, name);
                    let result = host.borrow_mut().get(&object, &property, &[]);
                    return result;
                }
                self.call_member(obj, name, &[], f)
            }
            Expr::MethodCall { obj, name, args } => self.call_member(obj, name, args, f),
            Expr::Unary { op, expr } => {
                let v = self.eval(expr, f)?;
                match op {
                    UnaryOp::Neg => match &v {
                        Variant::Integer(x) => Ok(Variant::Integer(-x)),
                        Variant::Long(x) => Ok(Variant::Long(-x)),
                        Variant::Single(x) => Ok(Variant::Single(-x)),
                        Variant::Double(x) => Ok(Variant::Double(-x)),
                        Variant::Currency(x) => Ok(Variant::Currency(-x)),
                        other => Ok(Variant::Double(-to_f64(other)?)),
                    },
                    UnaryOp::Not => match &v {
                        Variant::Boolean(b) => Ok(Variant::Boolean(!b)),
                        other => Ok(Variant::Long((!to_i64(other)?) as i32)),
                    },
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let l = self.eval(lhs, f)?;
                let r = self.eval(rhs, f)?;
                binary_op(op, &l, &r, self.option_compare_text)
            }
        }
    }

    fn eval_var(&mut self, name: &str, f: &mut Frame) -> ExecResult<Variant> {
        let lowered = Lowered::new(name);
        let ln = lowered.as_str();
        if let Some(rc) = self.lookup_ref(f, ln) {
            return Ok(rc.borrow().clone());
        }
        if let Some(c) = self.constants.get(ln) {
            return Ok(c.clone());
        }
        if let Some(rn) = &f.ret_name {
            if *rn == ln {
                return Ok(f.ret.clone());
            }
        }
        // `Timer` with no parentheses is the function (a variable of that
        // name, looked up above, takes precedence)
        if ln == "timer" {
            return self.builtin("timer", &[], f);
        }
        if self.option_explicit {
            return err(500, &format!("Variable not defined: {}", name));
        }
        Ok(Variant::Empty)
    }
}

// ---------------------------------------------------------------------------
// Variant coercions
// ---------------------------------------------------------------------------

pub(crate) fn to_f64(v: &Variant) -> ExecResult<f64> {
    match v {
        Variant::Boolean(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Variant::Integer(x) => Ok(*x as f64),
        Variant::Long(x) => Ok(*x as f64),
        Variant::Single(x) => Ok(*x as f64),
        Variant::Double(x) => Ok(*x),
        Variant::Currency(x) => Ok(*x),
        Variant::Date(x) => Ok(*x),
        Variant::Empty => Ok(0.0),
        Variant::String(s) => match s.trim().parse::<f64>() {
            Ok(x) => Ok(x),
            Err(_) => err(13, &format!("type mismatch: \"{}\" is not numeric", s)),
        },
        Variant::Null => err(94, "invalid use of Null"),
        Variant::Nothing | Variant::Array(_) | Variant::Record(_) => err(13, "type mismatch"),
    }
}

pub(crate) fn to_i64(v: &Variant) -> ExecResult<i64> {
    match v {
        Variant::Integer(x) => Ok(*x as i64),
        Variant::Long(x) => Ok(*x as i64),
        Variant::Boolean(b) => Ok(if *b { 1 } else { 0 }),
        Variant::Empty => Ok(0),
        other => Ok(to_f64(other)?.round() as i64),
    }
}

pub(crate) fn to_i32(v: &Variant) -> ExecResult<i32> {
    let n = to_i64(v)?;
    Ok(n.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
}

pub(crate) fn to_bool(v: &Variant) -> ExecResult<bool> {
    match v {
        Variant::Boolean(b) => Ok(*b),
        Variant::Empty => Ok(false),
        Variant::Null => err(94, "invalid use of Null"),
        Variant::String(s) => {
            let t = s.trim();
            if t.eq_ignore_ascii_case("true") {
                Ok(true)
            } else if t.eq_ignore_ascii_case("false") {
                Ok(false)
            } else {
                Ok(to_f64(v)? != 0.0)
            }
        }
        other => Ok(to_f64(other)? != 0.0),
    }
}

/// Format a number the way VB `CStr` does (1.0 -> "1").
pub(crate) fn fmt_num(x: f64) -> String {
    if x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{}", x)
    }
}

pub(crate) fn to_string(v: &Variant) -> ExecResult<String> {
    match v {
        Variant::String(s) => Ok(s.clone()),
        Variant::Boolean(b) => Ok(if *b { "True".into() } else { "False".into() }),
        Variant::Empty => Ok(String::new()),
        Variant::Null => err(94, "invalid use of Null"),
        Variant::Integer(x) => Ok(x.to_string()),
        Variant::Long(x) => Ok(x.to_string()),
        Variant::Single(x) => Ok(fmt_num(*x as f64)),
        Variant::Double(x) => Ok(fmt_num(*x)),
        Variant::Currency(x) => Ok(fmt_num(*x)),
        Variant::Date(d) => Ok(format_date(*d)),
        Variant::Nothing => Ok("Nothing".into()),
        Variant::Array(_) => err(13, "type mismatch: cannot convert array to string"),
        Variant::Record(_) => err(13, "type mismatch: cannot convert a user type to string"),
    }
}

/// `Print` representation (VB console spacing for numbers).
fn print_repr(v: &Variant) -> ExecResult<String> {
    match v {
        Variant::String(s) => Ok(s.clone()),
        Variant::Null => Ok("Null".into()),
        Variant::Integer(x) => Ok(format!(" {} ", x)),
        Variant::Long(x) => Ok(format!(" {} ", x)),
        Variant::Single(x) => Ok(format!(" {} ", fmt_num(*x as f64))),
        Variant::Double(x) => Ok(format!(" {} ", fmt_num(*x))),
        Variant::Currency(x) => Ok(format!(" {} ", fmt_num(*x))),
        Variant::Date(d) => Ok(format!(" {} ", format_date(*d))),
        Variant::Boolean(b) => Ok(if *b { "True".into() } else { "False".into() }),
        Variant::Empty => Ok(String::new()),
        Variant::Nothing => Ok("Nothing".into()),
        Variant::Array(_) => err(13, "type mismatch: cannot print array"),
        Variant::Record(_) => err(13, "type mismatch: cannot print a user type"),
    }
}

fn is_integral(v: &Variant) -> bool {
    matches!(
        v,
        Variant::Integer(_) | Variant::Long(_) | Variant::Boolean(_) | Variant::Empty
    )
}

/// Compact debugger display of a value (for watch panes).
pub(crate) fn debug_repr(v: &Variant) -> String {
    match v {
        Variant::Empty => "Empty".into(),
        Variant::Null => "Null".into(),
        Variant::Nothing => "Nothing".into(),
        Variant::Boolean(b) => if *b { "True" } else { "False" }.into(),
        Variant::Integer(x) => x.to_string(),
        Variant::Long(x) => x.to_string(),
        Variant::Single(x) => fmt_num(*x as f64),
        Variant::Double(x) => fmt_num(*x),
        Variant::Currency(x) => fmt_num(*x),
        Variant::Date(d) => format_date(*d),
        Variant::String(s) => format!("\"{}\"", s),
        Variant::Array(a) => format!("[{} items]", a.borrow().data.len()),
        Variant::Record(r) => format!("{{{}}}", r.ty),
    }
}

fn default_for_type(ty: Option<&str>) -> Variant {
    match ty {
        Some(t) if t.to_ascii_lowercase().contains("string") => Variant::String(String::new()),
        Some(t) if t.to_ascii_lowercase().contains("boolean") => Variant::Boolean(false),
        Some(t) if t.to_ascii_lowercase().contains("date") => Variant::Date(0.0),
        // a numeric variable starts at zero, as VB's does (not Empty, which prints blank)
        Some(t) if t.to_ascii_lowercase().contains("integer") => Variant::Integer(0),
        Some(t) if t.to_ascii_lowercase().contains("long") => Variant::Long(0),
        Some(t) if t.to_ascii_lowercase().contains("single") => Variant::Single(0.0),
        Some(t) if t.to_ascii_lowercase().contains("double") => Variant::Double(0.0),
        Some(t) if t.to_ascii_lowercase().contains("currency") => Variant::Currency(0.0),
        _ => Variant::Empty,
    }
}

/// Coerce a value to a declared `Dim x As <Type>` type on assignment.
pub(crate) fn coerce_typed(v: &Variant, ty: &str) -> ExecResult<Variant> {
    // declared types are stored lowercase (`declare_var`); a qualified
    // `Module.Type` counts by its last part
    let t = match ty.rfind('.') {
        Some(i) => &ty[i + 1..],
        None => ty,
    };
    // the common case, a value already of the declared type, is a copy
    match (t, v) {
        ("double", Variant::Double(_))
        | ("long", Variant::Long(_))
        | ("integer", Variant::Integer(_))
        | ("single", Variant::Single(_))
        | ("boolean", Variant::Boolean(_)) => return Ok(v.clone()),
        _ => {}
    }
    let owned;
    let t = if t.bytes().any(|b| b.is_ascii_uppercase()) {
        owned = t.to_ascii_lowercase();
        owned.as_str()
    } else {
        t
    };
    Ok(match t {
        "integer" => Variant::Integer(to_i32(v)? as i16),
        "long" | "byte" => Variant::Long(to_i32(v)?),
        "single" => Variant::Single(to_f64(v)? as f32),
        "double" => Variant::Double(to_f64(v)?),
        "currency" => Variant::Currency(to_f64(v)?),
        "boolean" => Variant::Boolean(to_bool(v)?),
        "string" => Variant::String(to_string(v)?),
        "date" => Variant::Date(to_f64(v)?),
        _ => v.clone(), // variant / object / unknown
    })
}

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

fn values_eq(l: &Variant, r: &Variant, text: bool) -> ExecResult<bool> {
    compare(l, r, BinaryOp::Eq, text)
}

/// Compare two values. With `text` (Option Compare Text), strings compare
/// without regard to case; otherwise they compare by character code.
fn compare(l: &Variant, r: &Variant, op: BinaryOp, text: bool) -> ExecResult<bool> {
    let fold = |s: &str| -> String {
        if text {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    let ord: std::cmp::Ordering = match (l, r) {
        (Variant::String(a), Variant::String(b)) => fold(a).cmp(&fold(b)),
        (Variant::Empty, Variant::String(b)) => "".cmp(fold(b).as_str()),
        (Variant::String(a), Variant::Empty) => fold(a).as_str().cmp(""),
        _ => {
            let a = to_f64(l)?;
            let b = to_f64(r)?;
            a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
        }
    };
    Ok(match op {
        BinaryOp::Eq => ord == std::cmp::Ordering::Equal,
        BinaryOp::Ne => ord != std::cmp::Ordering::Equal,
        BinaryOp::Lt => ord == std::cmp::Ordering::Less,
        BinaryOp::Gt => ord == std::cmp::Ordering::Greater,
        BinaryOp::Le => ord != std::cmp::Ordering::Greater,
        BinaryOp::Ge => ord != std::cmp::Ordering::Less,
        _ => false,
    })
}

/// `a op b` for two numbers when the answer is a plain `Double` or a
/// comparison; `None` for the operators that need the general rules
/// (integer division, `Mod`, `And`, `Pow`, a zero divisor...).
#[inline]
fn fast_f64(op: &BinaryOp, a: f64, b: f64) -> Option<Variant> {
    use BinaryOp::*;
    Some(match op {
        // as the general code: a result that is a whole number small enough
        // for a Long becomes a Long (`numeric`)
        Add => numeric(a + b),
        Sub => numeric(a - b),
        Mul => numeric(a * b),
        Div if b != 0.0 => Variant::Double(a / b),
        Lt => Variant::Boolean(a < b),
        Gt => Variant::Boolean(a > b),
        Le => Variant::Boolean(a <= b),
        Ge => Variant::Boolean(a >= b),
        Eq => Variant::Boolean(a == b),
        Ne => Variant::Boolean(a != b),
        _ => return None,
    })
}

fn numeric(x: f64) -> Variant {
    if x == x.trunc() && x.abs() < i32::MAX as f64 {
        Variant::Long(x as i32)
    } else {
        Variant::Double(x)
    }
}

fn binary_op(op: &BinaryOp, l: &Variant, r: &Variant, text: bool) -> ExecResult<Variant> {
    use BinaryOp::*;
    // The hot cases of a numeric loop, with no coercion to go through:
    // Double with Double (or Long), Long with Long. Anything else, or any
    // overflow, falls through to the general code below, so results are the
    // same as before.
    match (l, r) {
        (Variant::Double(a), Variant::Double(b)) => {
            if let Some(v) = fast_f64(op, *a, *b) {
                return Ok(v);
            }
        }
        (Variant::Double(a), Variant::Long(b)) => {
            if let Some(v) = fast_f64(op, *a, *b as f64) {
                return Ok(v);
            }
        }
        (Variant::Long(a), Variant::Double(b)) => {
            if let Some(v) = fast_f64(op, *a as f64, *b) {
                return Ok(v);
            }
        }
        (Variant::Long(a), Variant::Long(b)) => {
            let (a, b) = (*a as i64, *b as i64);
            let long = |x: i64| i32::try_from(x).ok().map(Variant::Long);
            let fast = match op {
                Add => long(a + b),
                Sub => long(a - b),
                Mul => long(a * b),
                Lt => Some(Variant::Boolean(a < b)),
                Gt => Some(Variant::Boolean(a > b)),
                Le => Some(Variant::Boolean(a <= b)),
                Ge => Some(Variant::Boolean(a >= b)),
                Eq => Some(Variant::Boolean(a == b)),
                Ne => Some(Variant::Boolean(a != b)),
                Mod if b != 0 => long(a % b),
                _ => None,
            };
            if let Some(v) = fast {
                return Ok(v);
            }
        }
        (Variant::Boolean(a), Variant::Boolean(b)) => match op {
            And => return Ok(Variant::Boolean(*a && *b)),
            Or => return Ok(Variant::Boolean(*a || *b)),
            _ => {}
        },
        _ => {}
    }
    match op {
        Eq | Ne | Lt | Gt | Le | Ge => Ok(Variant::Boolean(compare(l, r, *op, text)?)),
        Concat => Ok(Variant::String(format!(
            "{}{}",
            to_string(l)?,
            to_string(r)?
        ))),
        Add => {
            if let (Variant::String(a), Variant::String(b)) = (l, r) {
                return Ok(Variant::String(format!("{}{}", a, b)));
            }
            if matches!(l, Variant::Date(_)) || matches!(r, Variant::Date(_)) {
                return Ok(Variant::Date(to_f64(l)? + to_f64(r)?));
            }
            if is_integral(l) && is_integral(r) {
                let a = to_i64(l)?;
                let b = to_i64(r)?;
                match a.checked_add(b) {
                    Some(x) if x >= i32::MIN as i64 && x <= i32::MAX as i64 => {
                        Ok(Variant::Long(x as i32))
                    }
                    Some(_) => Ok(Variant::Double((a + b) as f64)),
                    None => err(6, "overflow"),
                }
            } else {
                Ok(numeric(to_f64(l)? + to_f64(r)?))
            }
        }
        Sub => {
            if matches!(l, Variant::Date(_)) || matches!(r, Variant::Date(_)) {
                return Ok(Variant::Date(to_f64(l)? - to_f64(r)?));
            }
            if is_integral(l) && is_integral(r) {
                Ok(Variant::Long((to_i64(l)? - to_i64(r)?) as i32))
            } else {
                Ok(numeric(to_f64(l)? - to_f64(r)?))
            }
        }
        Mul => {
            if is_integral(l) && is_integral(r) {
                let a = to_i64(l)?;
                let b = to_i64(r)?;
                match a.checked_mul(b) {
                    Some(x) if x >= i32::MIN as i64 && x <= i32::MAX as i64 => {
                        Ok(Variant::Long(x as i32))
                    }
                    _ => Ok(numeric(a as f64 * b as f64)),
                }
            } else {
                Ok(numeric(to_f64(l)? * to_f64(r)?))
            }
        }
        Div => {
            let d = to_f64(r)?;
            if d == 0.0 {
                return err(11, "division by zero");
            }
            Ok(Variant::Double(to_f64(l)? / d))
        }
        IntDiv => {
            let b = to_i64(r)?;
            if b == 0 {
                return err(11, "division by zero");
            }
            Ok(Variant::Long((to_i64(l)? / b) as i32))
        }
        Mod => {
            let b = to_i64(r)?;
            if b == 0 {
                return err(11, "division by zero");
            }
            Ok(Variant::Long((to_i64(l)? % b) as i32))
        }
        Pow => Ok(numeric(to_f64(l)?.powf(to_f64(r)?))),
        And | Or | Xor | Eqv | Imp => {
            if matches!(l, Variant::Boolean(_)) && matches!(r, Variant::Boolean(_)) {
                let a = to_bool(l)?;
                let b = to_bool(r)?;
                return Ok(Variant::Boolean(match op {
                    And => a && b,
                    Or => a || b,
                    Xor => a ^ b,
                    Eqv => a == b,
                    Imp => !a || b,
                    _ => false,
                }));
            }
            let a = to_i64(l)?;
            let b = to_i64(r)?;
            Ok(Variant::Long(match op {
                And => a & b,
                Or => a | b,
                Xor => a ^ b,
                Eqv => !(a ^ b),
                Imp => !a | b,
                _ => 0,
            } as i32))
        }
    }
}

// ---------------------------------------------------------------------------
// Date helpers (VB serial dates: day 0 = 1899-12-30)
// ---------------------------------------------------------------------------

pub(crate) fn now_serial() -> f64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    secs / 86400.0 + 25569.0
}

/// The VB serial of the Unix day `unix_days` (days since 1970-01-01), 25569 of
/// them after VB's epoch 1899-12-30.
const UNIX_TO_VB_DAYS: i64 = 25_569;

/// Howard Hinnant's civil-from-days algorithm, on a VB serial day (day 0 =
/// 1899-12-30): returns (year, month, day).
pub(crate) fn civil_from_days(serial_days: i64) -> (i64, u32, u32) {
    let z = serial_days - UNIX_TO_VB_DAYS + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub(crate) fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468 + UNIX_TO_VB_DAYS
}

pub(crate) fn format_date(serial: f64) -> String {
    let days = serial.floor() as i64;
    let frac = serial - days as f64;
    let (y, m, d) = civil_from_days(days);
    let secs = (frac * 86400.0).round() as u64;
    let (hh, mm, ss) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if hh == 0 && mm == 0 && ss == 0 {
        format!("{:04}-{:02}-{:02}", y, m, d)
    } else {
        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, hh, mm, ss)
    }
}
