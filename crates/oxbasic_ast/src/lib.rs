//! AST data structures & symbol definitions for OxidizedBASIC (`oxbasic`).
//!
//! These types are produced by `oxbasic_parser` and consumed by
//! `oxbasic_runtime`. They model the classic Visual Basic 6.0 / VBA
//! statement and expression surface that Phase 1 of the roadmap covers.

use std::rc::Rc;

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
    /// Shared, so a call (and each GoSub frame) takes a pointer, not a copy.
    pub body: Rc<Vec<Stmt>>,
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
    Assign {
        target: Expr,
        value: Expr,
    },
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
    /// `With obj ... End With`: inside, `.Member` is a member of `obj`.
    With {
        obj: Expr,
        body: Vec<Stmt>,
    },
    /// `Type Name ... End Type`: a record with these fields.
    TypeDecl {
        name: String,
        fields: Vec<VarDecl>,
    },
    /// `Enum Name ... End Enum`: named Long constants, `(name, value)`.
    EnumDecl {
        name: String,
        members: Vec<(String, Expr)>,
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
    /// `[obj.]Line`, `PSet`, `Circle`: VB's drawing statements, whose
    /// coordinate pairs aren't ordinary arguments.
    Graphics {
        /// What is drawn on (`Picture1`); `None` is the form whose code runs.
        target: Option<Expr>,
        shape: Graphic,
    },
    Exit(ExitKind),
    /// `Stop`: pause in the debugger (no effect outside the IDE).
    Stop,
    /// `Erase a, b`: each array is reset to its elements' defaults (a dynamic
    /// one is emptied).
    Erase(Vec<Expr>),
    /// `ReDim [Preserve] a(n), b(n, m)`: resizes dynamic arrays.
    ReDim {
        preserve: bool,
        vars: Vec<VarDecl>,
    },
    /// `Mid(s, start [, length]) = text`: overwrites characters of a string.
    MidAssign {
        target: Expr,
        start: Expr,
        length: Option<Expr>,
        value: Expr,
    },
    /// `GoSub label`: continue at `label`, and come back here on `Return`.
    GoSub(String),
    /// `Return`: back to the statement after the last `GoSub`.
    Return,
    GoTo(String),
    OnError(OnErrorMode),
    Resume(ResumeKind),
    End,
    OptionExplicit,
    /// `Option Base 0|1`: the default lower bound of arrays.
    OptionBase(i64),
    /// `Option Compare Text|Binary`: `true` for Text (case-insensitive).
    OptionCompareText(bool),
    /// `DefInt`, `DefStr`, ...: the type given to undeclared names whose first
    /// letter falls in each range, as `(type name, first letter, last letter)`.
    DefType(Vec<(String, char, char)>),
}

#[derive(Debug, Clone)]
pub struct IfArm {
    pub cond: Expr,
    pub body: Vec<Stmt>,
}

/// A point of a graphics statement: `(x, y)`, or `Step(x, y)` relative to
/// the last point drawn (`CurrentX`, `CurrentY`).
#[derive(Debug, Clone)]
pub struct GPoint {
    pub step: bool,
    pub x: Expr,
    pub y: Expr,
}

#[derive(Debug, Clone)]
pub enum Graphic {
    /// `Line [from]-to [, color] [, B | BF]`: no `from` starts at the last
    /// point drawn.
    Line {
        from: Option<GPoint>,
        to: GPoint,
        color: Option<Expr>,
        /// 0 a line, 1 a box (`B`), 2 a filled box (`BF`).
        boxed: u8,
    },
    PSet {
        at: GPoint,
        color: Option<Expr>,
    },
    /// `Circle (x, y), radius [, color [, start [, end [, aspect]]]]`.
    Circle {
        center: GPoint,
        radius: Expr,
        color: Option<Expr>,
        start: Option<Expr>,
        end: Option<Expr>,
        aspect: Option<Expr>,
    },
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
    /// `.Member` inside a `With` block: a member of the With object.
    WithRef(String),
    /// `Name(arg1, arg2)` — also used for array indexing; the runtime
    /// decides whether `name` is an array element or a procedure call.
    Call {
        name: String,
        args: Vec<Expr>,
    },
    /// `obj.Member` property access (e.g. `Err.Number`).
    Member {
        obj: Box<Expr>,
        name: String,
    },
    /// `obj.Method(args)` (e.g. `Err.Clear`).
    MethodCall {
        obj: Box<Expr>,
        name: String,
        args: Vec<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
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
