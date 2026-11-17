//! Interpreter, Variant system, and standard library for OxidizedBASIC.
//!
//! Phase 1 execution engine: a tree-walking interpreter over the
//! `oxbasic_ast` with a VB-style `Variant` tagged enum, procedure scopes
//! with ByRef parameter sharing (`Rc<RefCell<...>>`), arrays, and
//! `On Error GoTo` / `On Error Resume Next` trapping.

mod stdlib;

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
}

/// A BASIC array. `dims` is empty for dynamic (auto-growing) arrays.
#[derive(Clone, Debug)]
pub struct VbArray {
    pub dims: Vec<usize>,
    pub data: Vec<Variant>,
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
            if ix < 0 || ix as usize >= self.dims[i] {
                return err(9, "subscript out of range");
            }
            flat = flat * self.dims[i] + ix as usize;
        }
        Ok(flat)
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
    locals: HashMap<String, Rc<RefCell<Variant>>>,
    local_types: HashMap<String, String>,
    ret_name: Option<String>,
    ret: Variant,
    err_mode: ErrMode,
    /// Index (in the block that trapped the error) of the failing statement.
    err_pos: usize,
    err_number: i32,
    err_desc: String,
}

impl Frame {
    fn main() -> Frame {
        Frame {
            name: String::new(),
            is_main: true,
            locals: HashMap::new(),
            local_types: HashMap::new(),
            ret_name: None,
            ret: Variant::Empty,
            err_mode: ErrMode::None,
            err_pos: 0,
            err_number: 0,
            err_desc: String::new(),
        }
    }

    fn proc(decl: &ProcDecl) -> Frame {
        Frame {
            name: decl.name.to_lowercase(),
            is_main: false,
            locals: HashMap::new(),
            local_types: HashMap::new(),
            ret_name: match decl.kind {
                ProcKind::Function => Some(decl.name.to_lowercase()),
                ProcKind::Sub => None,
            },
            ret: Variant::Empty,
            err_mode: ErrMode::None,
            err_pos: 0,
            err_number: 0,
            err_desc: String::new(),
        }
    }
}

/// The OxidizedBASIC interpreter.
pub struct Interpreter<'out> {
    pub(crate) decls: Vec<Rc<ProcDecl>>,
    pub(crate) proc_index: HashMap<String, usize>,
    top_level: Vec<Stmt>,
    pub(crate) globals: HashMap<String, Rc<RefCell<Variant>>>,
    pub(crate) global_types: HashMap<String, String>,
    pub(crate) constants: HashMap<String, Variant>,
    pub(crate) out: &'out mut dyn Output,
    pub(crate) option_explicit: bool,
    pub(crate) rng: Cell<u64>,
    pub(crate) depth: usize,
}

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
        write!(f, "runtime error {}: {}", self.number, self.description)
    }
}

pub type ExecResult<T> = Result<T, VbError>;

pub(crate) fn err<T>(number: i32, description: &str) -> ExecResult<T> {
    Err(VbError {
        number,
        description: description.to_string(),
    })
}

impl<'out> Interpreter<'out> {
    pub fn new(prog: &Program, out: &'out mut dyn Output) -> Interpreter<'out> {
        let mut it = Interpreter {
            decls: Vec::new(),
            proc_index: HashMap::new(),
            top_level: Vec::new(),
            globals: HashMap::new(),
            global_types: HashMap::new(),
            constants: builtin_constants(),
            out,
            option_explicit: false,
            rng: Cell::new(0x9E3779B97F4A7C15),
            depth: 0,
        };
        it.add_program(prog);
        it
    }

    /// Register declarations (also used by the REPL).
    pub fn add_program(&mut self, prog: &Program) {
        for item in &prog.items {
            match item {
                Item::Proc(p) => {
                    self.proc_index.insert(p.name.to_lowercase(), self.decls.len());
                    self.decls.push(Rc::new(p.clone()));
                }
                Item::Stmt(s) => self.top_level.push(s.clone()),
            }
        }
    }

    pub fn run(&mut self) -> ExecResult<()> {
        let stmts = self.top_level.clone();
        let mut f = Frame::main();
        self.exec_block(&stmts, &mut f)?;
        Ok(())
    }

    /// Execute one source snippet in the global (main) scope (REPL).
    pub fn repl_line(&mut self, src: &str) -> Result<(), String> {
        let prog = oxbasic_parser::parse(src)?;
        let stmts: Vec<Stmt> = prog
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Proc(p) => {
                    self.proc_index.insert(p.name.to_lowercase(), self.decls.len());
                    self.decls.push(Rc::new(p.clone()));
                    None
                }
                Item::Stmt(s) => Some(s.clone()),
            })
            .collect();
        let mut f = Frame::main();
        self.exec_block(&stmts, &mut f).map(|_| ()).map_err(|e| e.to_string())
    }

    /// Names of every user-defined `Sub`/`Function` (for event wiring).
    pub fn proc_names(&self) -> Vec<String> {
        self.decls.iter().map(|d| d.name.clone()).collect()
    }

    /// Invoke a BASIC `Sub`/`Function` by name with evaluated argument
    /// values — the Phase 2 UI event bridge (spec §5.1:
    /// `runtime.evaluate_sub("Command1_Click", vec![])`).
    ///
    /// Parameters are bound by value (ByVal semantics). Returns the
    /// function value, or `Empty` for a `Sub`.
    pub fn evaluate_sub(&mut self, name: &str, args: &[Variant]) -> ExecResult<Variant> {
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
                        VbArray { dims: vec![n], data: items },
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
                        let mut scratch = Frame::main();
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
        let flow = self.exec_block(&body, &mut callee);
        self.depth -= 1;
        flow?;
        Ok(callee.ret)
    }

    // -- scope helpers ------------------------------------------------------

    fn lookup_rc(&self, f: &Frame, name: &str) -> Option<Rc<RefCell<Variant>>> {
        let ln = name.to_lowercase();
        if !f.is_main {
            if let Some(rc) = f.locals.get(&ln) {
                return Some(rc.clone());
            }
        }
        self.globals.get(&ln).cloned()
    }

    fn declared_type(&self, f: &Frame, name: &str) -> Option<String> {
        let ln = name.to_lowercase();
        if !f.is_main {
            if let Some(t) = f.local_types.get(&ln) {
                return Some(t.clone());
            }
        }
        self.global_types.get(&ln).cloned()
    }

    fn set_var(&mut self, f: &mut Frame, name: &str, value: Variant) -> ExecResult<()> {
        let ln = name.to_lowercase();
        let ty = self.declared_type(f, name);
        let value = match &ty {
            Some(t) => coerce_typed(&value, t)?,
            None => value,
        };
        if let Some(rc) = self.lookup_rc(f, name) {
            *rc.borrow_mut() = value;
            return Ok(());
        }
        if self.option_explicit {
            return err(500, &format!("Variable not defined: {}", name));
        }
        let rc = Rc::new(RefCell::new(value));
        if f.is_main {
            self.globals.insert(ln, rc);
        } else {
            f.locals.insert(ln, rc);
        }
        Ok(())
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
                let def = default_for_type(ty.as_deref());
                Variant::Array(Rc::new(RefCell::new(VbArray {
                    dims: ds,
                    data: vec![def; n],
                })))
            }
            None => default_for_type(ty.as_deref()),
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

fn builtin_constants() -> HashMap<String, Variant> {
    let mut c = HashMap::new();
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
    c
}


// ---------------------------------------------------------------------------
// Statement execution
// ---------------------------------------------------------------------------

impl<'out> Interpreter<'out> {
    /// Execute a statement block. Handles labels, `GoTo`, error trapping
    /// (`On Error GoTo <label>` / `Resume Next`) and `Resume`.
    pub(crate) fn exec_block(&mut self, stmts: &[Stmt], f: &mut Frame) -> ExecResult<Flow> {
        let mut labels: HashMap<String, usize> = HashMap::new();
        for (i, s) in stmts.iter().enumerate() {
            if let Stmt::Label(l) = s {
                labels.entry(l.to_lowercase()).or_insert(i);
            }
        }
        let mut i = 0usize;
        while i < stmts.len() {
            if let Stmt::Label(_) = &stmts[i] {
                i += 1;
                continue;
            }
            match self.exec_stmt(&stmts[i], f) {
                Ok(flow) => match flow {
                    Flow::Normal => {}
                    Flow::ExitDo | Flow::ExitFor | Flow::ExitProc | Flow::Halt => {
                        return Ok(flow)
                    }
                    Flow::Goto(l) => match labels.get(&l.to_lowercase()) {
                        Some(&j) => {
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
                        ErrMode::Goto(l) => match labels.get(&l.to_lowercase()) {
                            Some(&j) => {
                                i = j;
                                continue;
                            }
                            None => return Ok(Flow::Goto(l.clone())),
                        },
                        ErrMode::None => return Err(e),
                    }
                }
            }
            i += 1;
        }
        Ok(Flow::Normal)
    }


    fn exec_stmt(&mut self, s: &Stmt, f: &mut Frame) -> ExecResult<Flow> {
        match s {
            Stmt::Label(_) => Ok(Flow::Normal),
            Stmt::OptionExplicit => {
                self.option_explicit = true;
                Ok(Flow::Normal)
            }
            Stmt::End => Ok(Flow::Halt),
            Stmt::Dim(decls) => {
                for d in decls {
                    let dims = match &d.dims {
                        None => None,
                        Some(exprs) => {
                            let mut ds = Vec::new();
                            for e in exprs {
                                let v = self.eval(e, f)?;
                                let n = to_i64(&v)?;
                                if n < 0 {
                                    return err(9, "array dimension must be >= 0");
                                }
                                ds.push(n as usize + 1); // Dim a(10) spans 0..10
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
                let v = self.eval(value, f)?;
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
            Stmt::Select { expr, arms, else_body } => {
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
            Stmt::Print { items, trailing_sep } => {
                self.exec_print(items, *trailing_sep, f)?;
                Ok(Flow::Normal)
            }
            Stmt::Call { target, args, .. } => {
                self.exec_call(target, args, f)?;
                Ok(Flow::Normal)
            }
            _ => self.exec_loop_stmt(s, f),
        }
    }

    fn case_matches(&mut self, subject: &Variant, item: &CaseItem, f: &mut Frame) -> ExecResult<bool> {
        match item {
            CaseItem::Value(e) => {
                let v = self.eval(e, f)?;
                values_eq(subject, &v)
            }
            CaseItem::Range(lo, hi) => {
                let l = self.eval(lo, f)?;
                let h = self.eval(hi, f)?;
                Ok(compare(subject, &l, BinaryOp::Ge)? && compare(subject, &h, BinaryOp::Le)?)
            }
            CaseItem::Is(op, e) => {
                let v = self.eval(e, f)?;
                compare(subject, &v, *op)
            }
        }
    }
}


/// Loop statements, split out to keep `exec_stmt` manageable.
impl<'out> Interpreter<'out> {
    fn exec_loop_stmt(&mut self, s: &Stmt, f: &mut Frame) -> ExecResult<Flow> {
        match s {
            Stmt::For { var, start, end, step, body } => {
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
            _ => err(5, "internal: unhandled statement"),
        }
    }
}


// ---------------------------------------------------------------------------
// Assignment, Print, Calls
// ---------------------------------------------------------------------------

impl<'out> Interpreter<'out> {
    fn assign(&mut self, target: &Expr, v: Variant, f: &mut Frame) -> ExecResult<()> {
        match target {
            Expr::Var(name) => {
                let ln = name.to_lowercase();
                if let Some(rn) = &f.ret_name {
                    if *rn == ln {
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
                            VbArray { dims: vec![], data: vec![] },
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
                        let ix = idxs[0];
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
            _ => err(13, "type mismatch: cannot assign to this expression"),
        }
    }

    fn exec_print(&mut self, items: &[PrintItem], trailing_sep: bool, f: &mut Frame) -> ExecResult<()> {
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
                self.call_proc(name, args, f)?;
            }
            other => {
                self.eval(other, f)?;
            }
        }
        Ok(())
    }

    /// `obj.Member` / `obj.Method(args)` — currently supports the `Err`
    /// object (`Err.Number`, `Err.Description`, `Err.Clear`, `Err.Raise`).
    fn call_member(
        &mut self,
        obj: &Expr,
        name: &str,
        args: &[Expr],
        f: &mut Frame,
    ) -> ExecResult<Variant> {
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
        err(424, "object required (COM/class objects arrive in a later phase)")
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
                            VbArray { dims: vec![items.len()], data: items },
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
                    callee.locals
                        .insert(p.name.to_lowercase(), Rc::new(RefCell::new(v)));
                    if let Some(t) = &p.ty {
                        callee.local_types.insert(p.name.to_lowercase(), t.to_lowercase());
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
                        callee.locals
                            .insert(p.name.to_lowercase(), Rc::new(RefCell::new(v)));
                    }
                }
                if let Some(t) = &p.ty {
                    callee.local_types.insert(p.name.to_lowercase(), t.to_lowercase());
                }
            }
            self.depth += 1;
            let body = decl.body.clone();
            let flow = self.exec_block(&body, &mut callee);
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
            Expr::Call { name, args } => {
                // Array element read or procedure call.
                let ln = name.to_lowercase();
                if !self.proc_index.contains_key(&ln)
                    && !stdlib::is_builtin(&ln)
                    && args.len() == 1
                {
                    if let Some(rc) = self.lookup_rc(f, name) {
                        let borrowed = rc.borrow();
                        if let Variant::Array(arr) = &*borrowed {
                            let ix = to_i64(&self.eval(&args[0], f)?)?;
                            let arr = arr.borrow();
                            if arr.dims.is_empty() {
                                if ix >= 0 && (ix as usize) < arr.data.len() {
                                    return Ok(arr.data[ix as usize].clone());
                                }
                                return err(9, "subscript out of range");
                            }
                            let flat = arr.flat_index(&[ix])?;
                            return Ok(arr.data[flat].clone());
                        }
                    }
                }
                self.call_proc(name, args, f)
            }
            Expr::Member { obj, name } => self.call_member(obj, name, &[], f),
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
                binary_op(op, &l, &r)
            }
        }
    }

    fn eval_var(&mut self, name: &str, f: &mut Frame) -> ExecResult<Variant> {
        let ln = name.to_lowercase();
        if let Some(rc) = self.lookup_rc(f, name) {
            return Ok(rc.borrow().clone());
        }
        if let Some(c) = self.constants.get(&ln) {
            return Ok(c.clone());
        }
        if let Some(rn) = &f.ret_name {
            if *rn == ln {
                return Ok(f.ret.clone());
            }
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
        Variant::Nothing | Variant::Array(_) => err(13, "type mismatch"),
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
    }
}

fn is_integral(v: &Variant) -> bool {
    matches!(
        v,
        Variant::Integer(_) | Variant::Long(_) | Variant::Boolean(_) | Variant::Empty
    )
}

fn default_for_type(ty: Option<&str>) -> Variant {
    match ty {
        Some(t) if t.to_ascii_lowercase().contains("string") => {
            Variant::String(String::new())
        }
        Some(t) if t.to_ascii_lowercase().contains("boolean") => Variant::Boolean(false),
        Some(t) if t.to_ascii_lowercase().contains("date") => Variant::Date(0.0),
        _ => Variant::Empty,
    }
}

/// Coerce a value to a declared `Dim x As <Type>` type on assignment.
pub(crate) fn coerce_typed(v: &Variant, ty: &str) -> ExecResult<Variant> {
    let t = ty.to_ascii_lowercase();
    let t = t.rsplit('.').next().unwrap_or(&t).to_string();
    Ok(match t.as_str() {
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

fn values_eq(l: &Variant, r: &Variant) -> ExecResult<bool> {
    compare(l, r, BinaryOp::Eq)
}

fn compare(l: &Variant, r: &Variant, op: BinaryOp) -> ExecResult<bool> {
    let ord: std::cmp::Ordering = match (l, r) {
        (Variant::String(a), Variant::String(b)) => a.cmp(b),
        (Variant::Empty, Variant::String(b)) => "".cmp(b.as_str()),
        (Variant::String(a), Variant::Empty) => a.as_str().cmp(""),
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

fn numeric(x: f64) -> Variant {
    if x == x.trunc() && x.abs() < i32::MAX as f64 {
        Variant::Long(x as i32)
    } else {
        Variant::Double(x)
    }
}

fn binary_op(op: &BinaryOp, l: &Variant, r: &Variant) -> ExecResult<Variant> {
    use BinaryOp::*;
    match op {
        Eq | Ne | Lt | Gt | Le | Ge => Ok(Variant::Boolean(compare(l, r, *op)?)),
        Concat => Ok(Variant::String(format!("{}{}", to_string(l)?, to_string(r)?))),
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

/// Howard Hinnant's civil-from-days algorithm.
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
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
    era * 146_097 + doe as i64 - 719_468
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

