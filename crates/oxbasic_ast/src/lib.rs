//! AST data structures & symbol definitions for OxidizedBASIC (`oxbasic`).
//!
//! These types are produced by `oxbasic_parser` and consumed by
//! `oxbasic_runtime`. They model the classic Visual Basic 6.0 / VBA
//! statement and expression surface that Phase 1 of the roadmap covers.

/// A whole parsed translation unit: procedure declarations plus top-level
/// (module-level) statements, in source order.
#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Proc(ProcDecl),
    Stmt(Stmt),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcKind {
    Sub,
    Function,
}

#[derive(Debug, Clone)]
pub struct ProcDecl {
    pub kind: ProcKind,
    pub name: String,
    pub params: Vec<Param>,
    pub ret_type: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub by_ref: bool,
    pub optional: bool,
    pub param_array: bool,
    pub ty: Option<String>,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    /// `None` = scalar. `Some(vec![])` = dynamic array (`Dim a()`).
    /// `Some(bounds)` = fixed array, one entry per dimension.
    pub dims: Option<Vec<Expr>>,
    pub ty: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    /// Source line marker emitted before each statement (debugger support).
    LineMarker(usize),
    Label(String),
    Dim(Vec<VarDecl>),
    Const(Vec<ConstDecl>),
    Assign { target: Expr, value: Expr },
    If {
        arms: Vec<IfArm>,
        else_body: Option<Vec<Stmt>>,
    },
    Select {
        expr: Expr,
        arms: Vec<CaseArm>,
        else_body: Option<Vec<Stmt>>,
    },
    For {
        var: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
        body: Vec<Stmt>,
    },
    ForEach {
        var: String,
        coll: Expr,
        body: Vec<Stmt>,
    },
    DoWhile {
        cond: Expr,
        body: Vec<Stmt>,
        until: bool,
    },
    DoLoop {
        body: Vec<Stmt>,
        /// Post-test condition: `(cond, until?)`.
        cond: Option<(Expr, bool)>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Call {
        target: Expr,
        args: Vec<Expr>,
        /// `true` when the call used the VB "no parentheses" statement form.
        bare: bool,
    },
    Print {
        items: Vec<PrintItem>,
        trailing_sep: bool,
    },
    Exit(ExitKind),
    GoTo(String),
    OnError(OnErrorMode),
    Resume(ResumeKind),
    End,
    OptionExplicit,
}

#[derive(Debug, Clone)]
pub struct IfArm {
    pub cond: Expr,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct CaseArm {
    pub items: Vec<CaseItem>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum CaseItem {
    /// `Case 1 To 5`
    Range(Expr, Expr),
    /// `Case Is > 3`
    Is(BinaryOp, Expr),
    /// `Case 7`
    Value(Expr),
}

#[derive(Debug, Clone)]
pub struct PrintItem {
    pub expr: Expr,
    /// Separator character printed after this item (`;` or `,`).
    pub sep: Option<char>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitKind {
    Do,
    For,
    Sub,
    Function,
    Property,
}

#[derive(Debug, Clone)]
pub enum OnErrorMode {
    /// `On Error GoTo <label>`
    GoTo(String),
    /// `On Error GoTo 0` (disable handler)
    GoToZero,
    /// `On Error Resume Next`
    ResumeNext,
}

#[derive(Debug, Clone)]
pub enum ResumeKind {
    /// `Resume` (retry the failing statement)
    Retry,
    /// `Resume Next`
    Next,
    /// `Resume <label>`
    Label(String),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Lit(Lit),
    Var(String),
    /// `Name(arg1, arg2)` — also used for array indexing; the runtime
    /// decides whether `name` is an array element or a procedure call.
    Call { name: String, args: Vec<Expr> },
    /// `obj.Member` property access (e.g. `Err.Number`).
    Member { obj: Box<Expr>, name: String },
    /// `obj.Method(args)` (e.g. `Err.Clear`).
    MethodCall { obj: Box<Expr>, name: String, args: Vec<Expr> },
    Unary { op: UnaryOp, expr: Box<Expr> },
    Binary { op: BinaryOp, lhs: Box<Expr>, rhs: Box<Expr> },
}

#[derive(Debug, Clone)]
pub enum Lit {
    Int(i32),
    Double(f64),
    Str(String),
    Bool(bool),
    Empty,
    Null,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    Xor,
    Eqv,
    Imp,
}
