//! Grammar, lexer, and parser for OxidizedBASIC.
//!
//! Parses a VB6/VBA-compatible statement subset into `oxbasic_ast`
//! using a `pest` grammar (see `src/oxbasic.pest`).

use oxbasic_ast::*;
use std::rc::Rc;
use pest::iterators::{Pair, Pairs};
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "oxbasic.pest"]
pub struct OxbasicParser;

/// True for the atomic keyword-helper rules (`kw_if`, ...).
fn is_kw(rule: Rule) -> bool {
    format!("{:?}", rule).starts_with("kw_")
}

/// Skip keyword-helper pairs, keeping only meaningful children.
fn filter_kw<'a>(pairs: Pairs<'a, Rule>) -> impl Iterator<Item = Pair<'a, Rule>> {
    pairs.filter(move |p| !is_kw(p.as_rule()))
}

/// A syntax error with its position: `line` and `column` are 1-based, and
/// `message` is what the parser expected, without the formatted snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

/// Where the source fails to parse, if it does. The IDE uses this to
/// underline the bad text as the user types.
pub fn check(src: &str) -> Option<SyntaxError> {
    match OxbasicParser::parse(Rule::Program, src) {
        Ok(_) => None,
        Err(e) => {
            let (line, column) = match e.line_col {
                pest::error::LineColLocation::Pos((l, c)) => (l, c),
                pest::error::LineColLocation::Span((l, c), _) => (l, c),
            };
            let message = match &e.variant {
                pest::error::ErrorVariant::ParsingError { positives, negatives } => {
                    let mut parts: Vec<String> = Vec::new();
                    if !positives.is_empty() {
                        parts.push(format!(
                            "expected {}",
                            positives.iter().map(|r| format!("{:?}", r)).collect::<Vec<_>>().join(", ")
                        ));
                    }
                    if !negatives.is_empty() {
                        parts.push(format!(
                            "unexpected {}",
                            negatives.iter().map(|r| format!("{:?}", r)).collect::<Vec<_>>().join(", ")
                        ));
                    }
                    parts.join("; ")
                }
                pest::error::ErrorVariant::CustomError { message } => message.clone(),
            };
            Some(SyntaxError {
                line,
                column,
                message: if message.is_empty() {
                    "syntax error".to_string()
                } else {
                    message
                },
            })
        }
    }
}

/// Parse BASIC source text into a `Program`.
///
/// Returns a human-readable error (with position) on failure.
pub fn parse(src: &str) -> Result<Program, String> {
    let mut pairs =
        OxbasicParser::parse(Rule::Program, src).map_err(|e| format!("syntax error: {}", e))?;
    let pair = pairs.next().ok_or_else(|| "empty input".to_string())?;
    let mut items = Vec::new();
    for line in pair.into_inner() {
        // Rule::Line
        for child in line.into_inner() {
            match child.as_rule() {
                Rule::LabelDef => items.push(Item::Stmt(Stmt::Label(ident(child)))),
                Rule::StatementList => {
                    for s in child.into_inner() {
                        match s.as_rule() {
                            Rule::SubDecl => items.push(Item::Proc(proc_decl(s, ProcKind::Sub)?)),
                            Rule::FuncDecl => {
                                items.push(Item::Proc(proc_decl(s, ProcKind::Function)?))
                            }
                            _ => {
                                let line = s.line_col().0;
                                items.push(Item::Stmt(Stmt::LineMarker(line)));
                                items.push(Item::Stmt(stmt(s)?));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(Program { items })
}

fn err<T>(msg: &str) -> Result<T, String> {
    Err(msg.to_string())
}

fn ident(pair: Pair<Rule>) -> String {
    match pair.as_rule() {
        Rule::Ident => pair.as_str().to_string(),
        _ => pair
            .into_inner()
            .find(|p| p.as_rule() == Rule::Ident)
            .map(|p| p.as_str().to_string())
            .unwrap_or_default(),
    }
}

fn type_name(pair: Pair<Rule>) -> String {
    // a fixed-length string's size follows a `*`: `String*10`
    let mut name = pair
        .clone()
        .into_inner()
        .filter(|p| p.as_rule() == Rule::Ident)
        .map(|p| p.as_str().to_string())
        .collect::<Vec<_>>()
        .join(".");
    if let Some(size) = pair.into_inner().find(|p| p.as_rule() == Rule::Number) {
        name.push('*');
        name.push_str(size.as_str().trim());
    }
    name
}

/// Statements of a `Lines` region: children are `Line` pairs.
fn lines_stmts(pair: Pair<Rule>) -> Result<Vec<Stmt>, String> {
    let mut out = Vec::new();
    for line in pair.into_inner().filter(|p| p.as_rule() == Rule::Line) {
        out.extend(lines_stmts_one(line)?);
    }
    Ok(out)
}

/// Statements of a single `Line` pair.
fn lines_stmts_one(line: Pair<Rule>) -> Result<Vec<Stmt>, String> {
    let mut out = Vec::new();
    for child in line.into_inner() {
        match child.as_rule() {
            Rule::LabelDef => out.push(Stmt::Label(ident(child))),
            Rule::StatementList => out.extend(stmt_list_stmts(child)?),
            _ => {}
        }
    }
    Ok(out)
}

/// Statements of a `StatementList` pair.
fn stmt_list_stmts(pair: Pair<Rule>) -> Result<Vec<Stmt>, String> {
    let mut out = Vec::new();
    for s in pair.into_inner() {
        match s.as_rule() {
            Rule::SubDecl | Rule::FuncDecl => return err("procedures cannot be nested"),
            _ => {
                let line = s.line_col().0;
                out.push(Stmt::LineMarker(line));
                out.push(stmt(s)?);
            }
        }
    }
    Ok(out)
}

fn proc_decl(pair: Pair<Rule>, kind: ProcKind) -> Result<ProcDecl, String> {
    let mut name = String::new();
    let mut params = Vec::new();
    let mut ret_type = None;
    let mut body = Vec::new();
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::Ident => {
                if name.is_empty() {
                    name = child.as_str().to_string();
                }
            }
            Rule::Params => {
                for p in child.into_inner() {
                    params.push(param(p)?);
                }
            }
            Rule::TypeName => ret_type = Some(type_name(child)),
            _ => body.extend(lines_stmts_one(child)?),
        }
    }
    Ok(ProcDecl {
        kind,
        name,
        params,
        ret_type,
        body: Rc::new(body),
    })
}

fn param(pair: Pair<Rule>) -> Result<Param, String> {
    let mut p = Param {
        name: String::new(),
        by_ref: true,
        optional: false,
        param_array: false,
        ty: None,
        default: None,
    };
    let mut saw_binding = false;
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::Ident => {
                if p.name.is_empty() {
                    p.name = child.as_str().to_string();
                }
            }
            Rule::TypeName => p.ty = Some(type_name(child)),
            Rule::Expr => p.default = Some(expr(child)?),
            _ => {
                let m = child.as_str().trim().to_ascii_lowercase();
                match m.as_str() {
                    "byval" => {
                        p.by_ref = false;
                        saw_binding = true;
                    }
                    "byref" => {
                        p.by_ref = true;
                        saw_binding = true;
                    }
                    "optional" => p.optional = true,
                    "paramarray" => p.param_array = true,
                    _ => {}
                }
            }
        }
    }
    if !saw_binding {
        p.by_ref = true; // VB default is ByRef
    }
    Ok(p)
}
/// The items of a `Print` argument list, and whether it ends in a separator
/// (which suppresses the line break). `Print` and `Print #n` share this.
fn print_items(args: Option<Pair<Rule>>) -> Result<(Vec<PrintItem>, bool), String> {
    let mut items = Vec::new();
    let mut trailing = false;
    if let Some(args) = args {
        let mut pending: Option<Expr> = None;
        for child in args.into_inner() {
            match child.as_rule() {
                Rule::Expr => {
                    if let Some(e) = pending.take() {
                        items.push(PrintItem { expr: e, sep: None });
                    }
                    pending = Some(expr(child)?);
                    trailing = false;
                }
                Rule::PSep => {
                    let sep = child.as_str().chars().next().unwrap();
                    if let Some(e) = pending.take() {
                        items.push(PrintItem {
                            expr: e,
                            sep: Some(sep),
                        });
                    }
                    // A separator in final position suppresses the newline.
                    trailing = true;
                }
                _ => {}
            }
        }
        if let Some(e) = pending.take() {
            items.push(PrintItem { expr: e, sep: None });
            trailing = false;
        }
    }
    Ok((items, trailing))
}



fn stmt(pair: Pair<Rule>) -> Result<Stmt, String> {
    match pair.as_rule() {
        Rule::OptionStmt => {
            // `Option Explicit`, `Option Base 1` or `Option Compare Text`
            let child = filter_kw(pair.into_inner()).next();
            match child.map(|c| (c.as_rule(), c)) {
                Some((Rule::OptionBase, c)) => {
                    let n = filter_kw(c.into_inner()).next().unwrap();
                    let v = n.as_str().parse::<i64>().map_err(|e| e.to_string())?;
                    if v != 0 && v != 1 {
                        return err("Option Base must be 0 or 1");
                    }
                    Ok(Stmt::OptionBase(v))
                }
                Some((Rule::OptionCompare, c)) => {
                    let mode = c.as_str().to_lowercase();
                    Ok(Stmt::OptionCompareText(mode.ends_with("text")))
                }
                _ => Ok(Stmt::OptionExplicit),
            }
        }
        Rule::DefTypeStmt => {
            // `DefInt A-Z`: the keyword gives the type; each range is one or two
            // letters, the first and last of the range
            let mut children = pair.into_inner();
            let kw = children.next().unwrap().as_str().to_ascii_lowercase();
            let ty = match kw.trim_start_matches("def") {
                "int" => "Integer",
                "lng" => "Long",
                "str" => "String",
                "bool" => "Boolean",
                "dbl" => "Double",
                "sng" => "Single",
                "cur" => "Currency",
                "date" => "Date",
                _ => "Variant",
            };
            let mut ranges = Vec::new();
            for range in children {
                let mut letters = range.into_inner().map(|i| {
                    i.as_str().chars().next().unwrap().to_ascii_lowercase()
                });
                let first = letters.next().unwrap();
                let last = letters.next().unwrap_or(first);
                ranges.push((ty.to_string(), first, last));
            }
            Ok(Stmt::DefType(ranges))
        }
        Rule::FileOpenStmt => {
            let mut children = filter_kw(pair.into_inner());
            let path = expr(children.next().unwrap())?;
            let mode_pair = children.next().unwrap();
            let mode = match mode_pair.as_str().to_ascii_lowercase().as_str() {
                "input" => FileMode::Input,
                "output" => FileMode::Output,
                "random" => FileMode::Random,
                "binary" => FileMode::Binary,
                _ => FileMode::Append,
            };
            let number = expr(children.next().unwrap())?;
            // `Len = n` is optional; its expression is the last child
            let record_len = match children.next() {
                Some(p) => Some(expr(p)?),
                None => None,
            };
            Ok(Stmt::OpenFile { path, mode, number, record_len })
        }
        Rule::FileGetStmt | Rule::FilePutStmt => {
            let is_get = pair.as_rule() == Rule::FileGetStmt;
            let mut number = None;
            let mut pos = None;
            let mut target = None;
            for child in filter_kw(pair.into_inner()) {
                match child.as_rule() {
                    Rule::FileRecord => {
                        let e = child.into_inner().find(|c| c.as_rule() == Rule::Expr).unwrap();
                        pos = Some(expr(e)?);
                    }
                    _ => {
                        let e = expr(child)?;
                        if number.is_none() {
                            number = Some(e);
                        } else {
                            target = Some(e);
                        }
                    }
                }
            }
            let number = number.unwrap();
            let target = target.unwrap();
            Ok(if is_get {
                Stmt::FileGet { number, pos, target }
            } else {
                Stmt::FilePut { number, pos, value: target }
            })
        }
        Rule::FileSeekStmt => {
            let mut children = filter_kw(pair.into_inner());
            let number = expr(children.next().unwrap())?;
            let pos = expr(children.next().unwrap())?;
            Ok(Stmt::FileSeek { number, pos })
        }
        Rule::FileLockStmt | Rule::FileUnlockStmt => {
            let is_lock = pair.as_rule() == Rule::FileLockStmt;
            let mut number: Option<Expr> = None;
            let mut range_from: Option<Expr> = None;
            let mut range_to: Option<Expr> = None;
            for child in filter_kw(pair.into_inner()) {
                match child.as_rule() {
                    Rule::FileNumber => {
                        let n = child.into_inner().find(|c| c.as_rule() == Rule::Expr).unwrap();
                        number = Some(expr(n)?);
                    }
                    _ => {
                        // the range's expressions, in order: from, then to
                        let e = expr(child)?;
                        if range_from.is_none() {
                            range_from = Some(e);
                        } else {
                            range_to = Some(e);
                        }
                    }
                }
            }
            let number = number.ok_or("Lock/Unlock needs a file number")?;
            let range = range_from.map(|from| (from, range_to));
            Ok(if is_lock {
                Stmt::LockFile { number, range }
            } else {
                Stmt::UnlockFile { number, range }
            })
        }
        Rule::FileCloseStmt => {
            let mut numbers = Vec::new();
            for child in filter_kw(pair.into_inner()) {
                if child.as_rule() == Rule::FileNumber {
                    let n = child.into_inner().find(|c| c.as_rule() == Rule::Expr).unwrap();
                    numbers.push(expr(n)?);
                }
            }
            Ok(Stmt::CloseFile(numbers))
        }
        Rule::FileInputStmt => {
            let mut exprs = Vec::new();
            for child in filter_kw(pair.into_inner()) {
                exprs.push(expr(child)?);
            }
            let mut it = exprs.into_iter();
            let number = it.next().unwrap();
            Ok(Stmt::InputFile {
                number,
                targets: it.collect(),
            })
        }
        Rule::FileLineInputStmt => {
            let mut exprs = Vec::new();
            for child in filter_kw(pair.into_inner()) {
                exprs.push(expr(child)?);
            }
            let mut it = exprs.into_iter();
            let number = it.next().unwrap();
            let target = it.next().unwrap();
            Ok(Stmt::LineInputFile { number, target })
        }
        Rule::FilePrintStmt => {
            let write = pair
                .clone()
                .into_inner()
                .any(|p| p.as_rule() == Rule::kw_write);
            let mut number = None;
            let mut args = None;
            for child in filter_kw(pair.into_inner()) {
                match child.as_rule() {
                    Rule::Expr if number.is_none() => number = Some(expr(child)?),
                    Rule::PrintArgs => args = Some(child),
                    _ => {}
                }
            }
            let (items, trailing_sep) = print_items(args)?;
            Ok(Stmt::PrintFile {
                number: number.unwrap(),
                items,
                trailing_sep,
                write,
            })
        }
        Rule::EraseStmt => {
            let mut targets = Vec::new();
            for child in filter_kw(pair.into_inner()) {
                targets.push(expr(child)?);
            }
            Ok(Stmt::Erase(targets))
        }
        Rule::ReDimStmt => {
            // `filter_kw` drops the keyword pairs, so look for `Preserve` first
            let preserve = pair
                .clone()
                .into_inner()
                .any(|p| p.as_rule() == Rule::kw_preserve);
            let mut vars = Vec::new();
            for child in filter_kw(pair.into_inner()) {
                if child.as_rule() == Rule::ReDimVar {
                    let mut name = String::new();
                    let mut dims = None;
                    for c in child.into_inner() {
                        match c.as_rule() {
                            Rule::Ident => name = c.as_str().to_string(),
                            Rule::ArrayDims => {
                                dims = Some(c.into_inner().map(expr).collect::<Result<_, _>>()?);
                            }
                            _ => {}
                        }
                    }
                    vars.push(VarDecl { name, dims, ty: None });
                }
            }
            Ok(Stmt::ReDim { preserve, vars })
        }
        Rule::MidStmt => {
            let mut children = filter_kw(pair.into_inner());
            let target = expr(children.next().unwrap())?;
            let start = expr(children.next().unwrap())?;
            // the children after `start` are the optional length, then the text
            let rest: Vec<_> = children.collect();
            let (value_pair, length_pair) = rest.split_last().unwrap();
            let length = match length_pair.first() {
                Some(l) => Some(expr(l.clone())?),
                None => None,
            };
            let value = expr(value_pair.clone())?;
            Ok(Stmt::MidAssign {
                target,
                start,
                length,
                value,
            })
        }
        Rule::LabelDef => Ok(Stmt::Label(ident(pair))),
        Rule::DimStmt => {
            let mut decls = Vec::new();
            for child in filter_kw(pair.into_inner()) {
                let mut name = String::new();
                let mut dims: Option<Vec<Expr>> = None;
                let mut ty = None;
                for c in child.into_inner() {
                    match c.as_rule() {
                        Rule::Ident => name = c.as_str().to_string(),
                        Rule::ArrayDims => {
                            dims = Some(c.into_inner().map(expr).collect::<Result<_, _>>()?);
                        }
                        Rule::TypeName => ty = Some(type_name(c)),
                        _ => {}
                    }
                }
                decls.push(VarDecl { name, dims, ty });
            }
            Ok(Stmt::Dim(decls))
        }
        Rule::ConstStmt => {
            let mut decls = Vec::new();
            for child in filter_kw(pair.into_inner()) {
                let mut name = String::new();
                let mut value = Expr::Lit(Lit::Empty);
                for c in child.into_inner() {
                    match c.as_rule() {
                        Rule::Ident => name = c.as_str().to_string(),
                        Rule::Expr => value = expr(c)?,
                        _ => {}
                    }
                }
                decls.push(ConstDecl { name, value });
            }
            Ok(Stmt::Const(decls))
        }
        Rule::LetStmt | Rule::SetStmt => {
            let mut children = filter_kw(pair.into_inner());
            let target = expr(children.next().unwrap())?;
            let value = expr(children.next().unwrap())?;
            Ok(Stmt::Assign { target, value })
        }
        Rule::IfStmt => {
            let mut children = filter_kw(pair.into_inner());
            let cond = expr(children.next().unwrap())?;
            let block = children.next().unwrap();
            match block.as_rule() {
                Rule::BlockIf => {
                    let mut arms = vec![IfArm {
                        cond,
                        body: Vec::new(),
                    }];
                    let mut else_body = None;
                    for child in block.into_inner() {
                        match child.as_rule() {
                            Rule::Line => arms
                                .last_mut()
                                .unwrap()
                                .body
                                .extend(lines_stmts_one(child)?),
                            Rule::ElseIfBlock => {
                                let mut ec = filter_kw(child.into_inner());
                                let c = expr(ec.next().unwrap())?;
                                let body = rest_stmts(ec)?;
                                arms.push(IfArm { cond: c, body });
                            }
                            Rule::ElseBlock => {
                                else_body = Some(lines_stmts(child)?);
                            }
                            _ => {}
                        }
                    }
                    Ok(Stmt::If { arms, else_body })
                }
                Rule::LineIf => {
                    let mut children = filter_kw(block.into_inner());
                    let then_body = stmt_list_stmts(children.next().unwrap())?;
                    let else_body = match children.next() {
                        Some(p) => Some(stmt_list_stmts(p)?),
                        None => None,
                    };
                    Ok(Stmt::If {
                        arms: vec![IfArm {
                            cond,
                            body: then_body,
                        }],
                        else_body,
                    })
                }
                r => err(&format!("internal: unexpected if block {:?}", r)),
            }
        }
        Rule::SelectStmt => {
            let mut children = filter_kw(pair.into_inner());
            let subject = expr(children.next().unwrap())?;
            let mut arms = Vec::new();
            let mut else_body = None;
            for child in children {
                match child.as_rule() {
                    Rule::Line => {} // statements before the first Case are ignored (rare)
                    Rule::CaseBlock => {
                        let mut items = Vec::new();
                        let mut body = Vec::new();
                        for c in child.into_inner() {
                            match c.as_rule() {
                                Rule::CaseItems => {
                                    for ci in c.into_inner() {
                                        items.push(case_item(ci)?);
                                    }
                                }
                                _ => body.extend(lines_stmts_one(c)?),
                            }
                        }
                        arms.push(CaseArm { items, body });
                    }
                    Rule::CaseElseBlock => {
                        else_body = Some(lines_stmts(child)?);
                    }
                    _ => {}
                }
            }
            Ok(Stmt::Select {
                expr: subject,
                arms,
                else_body,
            })
        }
        Rule::ForEachStmt => {
            let mut var = String::new();
            let mut coll = None;
            let mut body = Vec::new();
            for child in pair.into_inner() {
                match child.as_rule() {
                    Rule::Ident => {
                        if var.is_empty() {
                            var = child.as_str().to_string();
                        }
                    }
                    Rule::Expr => {
                        if coll.is_none() {
                            coll = Some(expr(child)?);
                        }
                    }
                    _ => body.extend(lines_stmts_one(child)?),
                }
            }
            Ok(Stmt::ForEach {
                var,
                coll: coll.unwrap(),
                body,
            })
        }
        Rule::ForStmt => {
            let mut var = String::new();
            let mut exprs = Vec::new();
            let mut body = Vec::new();
            for child in pair.into_inner() {
                match child.as_rule() {
                    Rule::Ident => {
                        if var.is_empty() {
                            var = child.as_str().to_string();
                        }
                    }
                    Rule::Expr => exprs.push(expr(child)?),
                    _ => body.extend(lines_stmts_one(child)?),
                }
            }
            if exprs.len() < 2 {
                return err("For statement requires start and end expressions");
            }
            Ok(Stmt::For {
                var,
                start: exprs[0].clone(),
                end: exprs[1].clone(),
                step: exprs.get(2).cloned(),
                body,
            })
        }
        Rule::DoStmt => {
            let inner = filter_kw(pair.into_inner()).next().unwrap();
            match inner.as_rule() {
                Rule::DoPre => {
                    let mut children = inner.into_inner();
                    let cond_pair = children.next().unwrap();
                    let (cond, until) = do_cond(cond_pair)?;
                    let body = rest_stmts(children)?;
                    Ok(Stmt::DoWhile { cond, body, until })
                }
                Rule::DoPost => {
                    let children = inner.into_inner();
                    let mut body = Vec::new();
                    let mut cond = None;
                    for c in children {
                        match c.as_rule() {
                            Rule::Line => body.extend(lines_stmts_one(c)?),
                            Rule::DoCond => {
                                let (e, until) = do_cond(c)?;
                                cond = Some((e, until));
                            }
                            _ => {}
                        }
                    }
                    Ok(Stmt::DoLoop { body, cond })
                }
                r => err(&format!("internal: unexpected do form {:?}", r)),
            }
        }
        Rule::WhileStmt => {
            let mut children = filter_kw(pair.into_inner());
            let cond = expr(children.next().unwrap())?;
            let body = rest_stmts(children)?;
            Ok(Stmt::While { cond, body })
        }
        Rule::WithStmt => {
            let mut children = filter_kw(pair.into_inner());
            let obj = expr(children.next().unwrap())?;
            let body = rest_stmts(children)?;
            Ok(Stmt::With { obj, body })
        }
        Rule::TypeDecl => {
            let mut name = String::new();
            let mut fields = Vec::new();
            for child in pair.into_inner() {
                match child.as_rule() {
                    Rule::Ident => name = child.as_str().to_string(),
                    Rule::TypeField => {
                        let mut field_name = String::new();
                        let mut dims = None;
                        let mut ty = None;
                        for c in child.into_inner() {
                            match c.as_rule() {
                                Rule::Ident => field_name = c.as_str().to_string(),
                                Rule::ArrayDims => {
                                    dims = Some(c.into_inner().map(expr).collect::<Result<_, _>>()?);
                                }
                                Rule::TypeName => ty = Some(type_name(c)),
                                _ => {}
                            }
                        }
                        fields.push(VarDecl {
                            name: field_name,
                            dims,
                            ty,
                        });
                    }
                    _ => {}
                }
            }
            Ok(Stmt::TypeDecl { name, fields })
        }
        Rule::EnumDecl => {
            let mut name = String::new();
            let mut members: Vec<(String, Expr)> = Vec::new();
            for child in pair.into_inner() {
                match child.as_rule() {
                    Rule::Ident => name = child.as_str().to_string(),
                    Rule::EnumMember => {
                        let mut member = String::new();
                        let mut value = None;
                        for c in child.into_inner() {
                            match c.as_rule() {
                                Rule::Ident => member = c.as_str().to_string(),
                                Rule::Expr => value = Some(expr(c)?),
                                _ => {}
                            }
                        }
                        // without `= value`, a member is the previous one plus 1
                        // (the first is 0)
                        let value = match value {
                            Some(v) => v,
                            None => match members.last() {
                                Some((_, prev)) => Expr::Binary {
                                    op: BinaryOp::Add,
                                    lhs: Box::new(prev.clone()),
                                    rhs: Box::new(Expr::Lit(Lit::Int(1))),
                                },
                                None => Expr::Lit(Lit::Int(0)),
                            },
                        };
                        members.push((member, value));
                    }
                    _ => {}
                }
            }
            Ok(Stmt::EnumDecl { name, members })
        }
        Rule::WithCallStmt => {
            // `.Caption = x` is an assignment, handled by the Let/Set rules;
            // a bare `.Refresh` or `.Move 1, 2` is a call on the With object
            let mut children = pair.into_inner();
            let first = children.next().unwrap();
            let mut path = vec![ident(first)];
            let mut args = Vec::new();
            for child in children {
                match child.as_rule() {
                    Rule::MemberTail => path.extend(
                        child
                            .into_inner()
                            .filter(|c| c.as_rule() == Rule::Ident)
                            .map(|c| c.as_str().to_string()),
                    ),
                    Rule::Expr => args.push(expr(child)?),
                    _ => {}
                }
            }
            let target = path[1..].iter().fold(
                Expr::WithRef(path[0].clone()),
                |acc, seg| Expr::Member {
                    obj: Box::new(acc),
                    name: seg.clone(),
                },
            );
            Ok(Stmt::Call {
                target,
                args,
                bare: true,
            })
        }
        Rule::PrintStmt => {
            let args = filter_kw(pair.into_inner()).next();
            let (items, trailing) = print_items(args)?;
            Ok(Stmt::Print {
                items,
                trailing_sep: trailing,
            })
        }
        Rule::ExitStmt => {
            let word = pair
                .as_str()
                .trim()
                .rsplit(|c: char| c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            let kind = match word.as_str() {
                "sub" => ExitKind::Sub,
                "function" => ExitKind::Function,
                "property" => ExitKind::Property,
                "for" => ExitKind::For,
                "do" => ExitKind::Do,
                _ => return err("internal: bad exit kind"),
            };
            Ok(Stmt::Exit(kind))
        }
        Rule::GoToStmt => Ok(Stmt::GoTo(ident(pair))),
        Rule::GoSubStmt => Ok(Stmt::GoSub(ident(pair))),
        Rule::ReturnStmt => Ok(Stmt::Return),
        Rule::StopStmt => Ok(Stmt::Stop),
        Rule::OnErrorStmt => {
            let inner = filter_kw(pair.into_inner()).next().unwrap();
            match inner.as_rule() {
                Rule::OnErrGoto => {
                    let mut children = filter_kw(inner.into_inner());
                    match children.next() {
                        Some(p) => Ok(Stmt::OnError(OnErrorMode::GoTo(p.as_str().to_string()))),
                        None => Ok(Stmt::OnError(OnErrorMode::GoToZero)),
                    }
                }
                Rule::OnErrResumeNext => Ok(Stmt::OnError(OnErrorMode::ResumeNext)),
                r => err(&format!("internal: unexpected on error form {:?}", r)),
            }
        }
        Rule::ResumeStmt => {
            let child = filter_kw(pair.into_inner()).next();
            match child.as_ref().map(|c| c.as_rule()) {
                Some(Rule::ResumeNextKw) => Ok(Stmt::Resume(ResumeKind::Next)),
                Some(Rule::Ident) => Ok(Stmt::Resume(ResumeKind::Label(ident(child.unwrap())))),
                None => Ok(Stmt::Resume(ResumeKind::Retry)),
                r => err(&format!("internal: unexpected resume form {:?}", r)),
            }
        }
        Rule::EndStmt => Ok(Stmt::End),
        Rule::CallStmt => {
            let target = expr(filter_kw(pair.into_inner()).next().unwrap())?;
            match target {
                Expr::Call { name, args } => Ok(Stmt::Call {
                    target: Expr::Var(name),
                    args,
                    bare: false,
                }),
                Expr::MethodCall { obj, name, args } => Ok(Stmt::Call {
                    target: Expr::Member { obj, name },
                    args,
                    bare: false,
                }),
                other => Ok(Stmt::Call {
                    target: other,
                    args: vec![],
                    bare: false,
                }),
            }
        }
        Rule::ExprCallStmt => {
            let mut children = pair.into_inner();
            let first = children.next().unwrap();
            let mut path = vec![first.as_str().to_string()];
            let mut element = None;
            let mut args = Vec::new();
            for child in children {
                match child.as_rule() {
                    Rule::Ident => path.push(child.as_str().to_string()),
                    // `Command1(2).Caption`: an element of a control array
                    Rule::ElementIndex => {
                        element = Some(expr(child.into_inner().next().unwrap())?);
                    }
                    // `.Member` segments sit inside MemberTail
                    Rule::MemberTail => path.extend(
                        child
                            .into_inner()
                            .filter(|c| c.as_rule() == Rule::Ident)
                            .map(|c| c.as_str().to_string()),
                    ),
                    Rule::Expr => args.push(expr(child)?),
                    _ => {}
                }
            }
            let head = match element {
                Some(index) => Expr::Call {
                    name: path[0].clone(),
                    args: vec![index],
                },
                None => Expr::Var(path[0].clone()),
            };
            let target = path[1..].iter().fold(head, |acc, seg| Expr::Member {
                obj: Box::new(acc),
                name: seg.clone(),
            });
            Ok(Stmt::Call {
                target,
                args,
                bare: true,
            })
        }
        Rule::SubDecl | Rule::FuncDecl => err("procedures cannot be nested"),
        Rule::GraphicsStmt => graphics_stmt(pair),
        r => err(&format!("internal: unexpected statement rule {:?}", r)),
    }
}

/// `[obj.]Line`, `PSet` or `Circle` with its coordinate pairs.
fn graphics_stmt(pair: Pair<Rule>) -> Result<Stmt, String> {
    let mut target = None;
    let mut shape = None;
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::GraphicsTarget => target = Some(graphics_target(child)?),
            Rule::LineCall => {
                let mut points = Vec::new();
                let mut color = None;
                let mut boxed = 0;
                for p in child.into_inner() {
                    match p.as_rule() {
                        Rule::Point => points.push(g_point(p)?),
                        Rule::GraphicsColor => {
                            color = p.into_inner().next().map(expr).transpose()?
                        }
                        Rule::BoxFlag => {
                            let flag = p.into_inner().next().unwrap();
                            boxed = if flag.as_rule() == Rule::BoxFill {
                                2
                            } else {
                                1
                            };
                        }
                        _ => {}
                    }
                }
                let to = points.pop().ok_or("Line needs a point")?;
                shape = Some(Graphic::Line {
                    from: points.pop(),
                    to,
                    color,
                    boxed,
                });
            }
            Rule::PSetCall => {
                let mut at = None;
                let mut color = None;
                for p in child.into_inner() {
                    match p.as_rule() {
                        Rule::Point => at = Some(g_point(p)?),
                        Rule::GraphicsColor => {
                            color = p.into_inner().next().map(expr).transpose()?
                        }
                        _ => {}
                    }
                }
                shape = Some(Graphic::PSet {
                    at: at.ok_or("PSet needs a point")?,
                    color,
                });
            }
            Rule::CircleCall => {
                let mut inner = child
                    .into_inner()
                    .filter(|p| p.as_rule() != Rule::kw_circle);
                let center = g_point(inner.next().unwrap())?;
                let radius = expr(inner.next().unwrap())?;
                // `, color, start, end, aspect`: each may be left out
                let mut rest: Vec<Option<Expr>> = Vec::new();
                for arg in inner {
                    rest.push(arg.into_inner().next().map(expr).transpose()?);
                }
                if rest.len() > 4 {
                    return err("Circle takes at most color, start, end and aspect");
                }
                rest.resize_with(4, || None);
                let mut rest = rest.into_iter();
                shape = Some(Graphic::Circle {
                    center,
                    radius,
                    color: rest.next().flatten(),
                    start: rest.next().flatten(),
                    end: rest.next().flatten(),
                    aspect: rest.next().flatten(),
                });
            }
            _ => {}
        }
    }
    Ok(Stmt::Graphics {
        target,
        shape: shape.ok_or("graphics statement")?,
    })
}

/// A graphics `Point`: `[Step](x, y)`.
fn g_point(pair: Pair<Rule>) -> Result<GPoint, String> {
    let mut step = false;
    let mut coords = Vec::new();
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::StepKw => step = true,
            Rule::Expr => coords.push(expr(p)?),
            _ => {}
        }
    }
    let y = coords.pop().ok_or("a point needs x and y")?;
    let x = coords.pop().ok_or("a point needs x and y")?;
    Ok(GPoint { step, x, y })
}

/// What a graphics statement draws on: `Picture1`, `Pic(2)`,
/// `Form2.Picture1`, as the expression the interpreter resolves.
fn graphics_target(pair: Pair<Rule>) -> Result<Expr, String> {
    let mut head: Option<Expr> = None;
    let mut segments = Vec::new();
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::Ident if head.is_none() => head = Some(Expr::Var(p.as_str().to_string())),
            Rule::Ident => segments.push(p.as_str().to_string()),
            Rule::ElementIndex => {
                let name = match &head {
                    Some(Expr::Var(n)) => n.clone(),
                    _ => return err("an element index needs a name"),
                };
                let index = expr(p.into_inner().next().unwrap())?;
                head = Some(Expr::Call {
                    name,
                    args: vec![index],
                });
            }
            _ => {}
        }
    }
    let head = head.ok_or("graphics target")?;
    Ok(segments.into_iter().fold(head, |acc, seg| Expr::Member {
        obj: Box::new(acc),
        name: seg,
    }))
}

/// A `DoCond` pair: returns the condition expression and whether it is
/// an `Until` (as opposed to `While`).
fn do_cond(pair: Pair<Rule>) -> Result<(Expr, bool), String> {
    let mut children = pair.into_inner();
    let kw = children.next().unwrap();
    let until = matches!(format!("{:?}", kw.as_rule()).as_str(), "kw_until");
    let e = expr(children.next().unwrap())?;
    Ok((e, until))
}

/// Collect statements from an iterator of remaining children (Line pairs).
fn rest_stmts<'a, I: Iterator<Item = Pair<'a, Rule>>>(it: I) -> Result<Vec<Stmt>, String> {
    let mut out = Vec::new();
    for p in it {
        if p.as_rule() == Rule::Line {
            out.extend(lines_stmts_one(p)?);
        }
    }
    Ok(out)
}

fn case_item(pair: Pair<Rule>) -> Result<CaseItem, String> {
    let mut children = filter_kw(pair.into_inner());
    let first = children.next().unwrap();
    if first.as_rule() == Rule::CompOp {
        // `Is <op> <expr>`
        let op = comp_op(&first);
        let e = expr(children.next().unwrap())?;
        Ok(CaseItem::Is(op, e))
    } else {
        let lo = expr(first)?;
        match children.next() {
            Some(hi) => Ok(CaseItem::Range(lo, expr(hi)?)),
            None => Ok(CaseItem::Value(lo)),
        }
    }
}

fn comp_op(pair: &Pair<Rule>) -> BinaryOp {
    match pair.as_str().trim() {
        ">=" => BinaryOp::Ge,
        "<=" => BinaryOp::Le,
        "<>" => BinaryOp::Ne,
        "=" => BinaryOp::Eq,
        "<" => BinaryOp::Lt,
        ">" => BinaryOp::Gt,
        _ => BinaryOp::Eq,
    }
}

/// Convert an expression-level pair into an `Expr`.
fn expr(pair: Pair<Rule>) -> Result<Expr, String> {
    match pair.as_rule() {
        Rule::Expr => expr(pair.into_inner().next().unwrap()),
        Rule::ImpExpr => binary_chain(pair, BinaryOp::Imp),
        Rule::EqvExpr => binary_chain(pair, BinaryOp::Eqv),
        Rule::XorExpr => binary_chain(pair, BinaryOp::Xor),
        Rule::OrExpr => binary_chain(pair, BinaryOp::Or),
        Rule::AndExpr => binary_chain(pair, BinaryOp::And),
        Rule::NotExpr => {
            let mut children = filter_kw(pair.into_inner());
            let child = children.next().unwrap();
            if child.as_rule() == Rule::NotExpr {
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr(child)?),
                })
            } else {
                expr(child)
            }
        }
        Rule::CmpExpr => {
            let mut lhs: Option<Expr> = None;
            let mut pending_op: Option<BinaryOp> = None;
            for child in pair.into_inner() {
                match child.as_rule() {
                    Rule::CompOp => pending_op = Some(comp_op(&child)),
                    _ => {
                        let e = expr(child)?;
                        lhs = Some(match (lhs, pending_op.take()) {
                            (Some(l), Some(op)) => Expr::Binary {
                                op,
                                lhs: Box::new(l),
                                rhs: Box::new(e),
                            },
                            (Some(_), None) => {
                                return err("internal: comparison chain without operator")
                            }
                            (None, _) => e,
                        });
                    }
                }
            }
            Ok(lhs.unwrap())
        }
        Rule::ConcatExpr => binary_chain(pair, BinaryOp::Concat),
        Rule::AddExpr => op_chain(pair, |s| match s {
            "+" => BinaryOp::Add,
            _ => BinaryOp::Sub,
        }),
        Rule::MulExpr => op_chain(pair, |s| match s {
            "*" => BinaryOp::Mul,
            _ => BinaryOp::Div,
        }),
        Rule::IntDivExpr => binary_chain(pair, BinaryOp::IntDiv),
        Rule::ModExpr => binary_chain(pair, BinaryOp::Mod),
        Rule::PowExpr => binary_chain(pair, BinaryOp::Pow),
        Rule::Unary => {
            let mut children = pair.into_inner();
            let first = children.next().unwrap();
            if first.as_rule() == Rule::Sign {
                if first.as_str() == "+" {
                    return expr(children.next().unwrap());
                }
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr(children.next().unwrap())?),
                })
            } else {
                expr(first)
            }
        }
        Rule::PostfixExpr => postfix_expr(pair),
        r => err(&format!("internal: unexpected expression rule {:?}", r)),
    }
}

/// Left-associative chain where the operator is fixed by the rule
/// (e.g. `AndExpr`, `ConcatExpr`): children are all operands.
fn binary_chain(pair: Pair<Rule>, op: BinaryOp) -> Result<Expr, String> {
    let mut lhs: Option<Expr> = None;
    for child in filter_kw(pair.into_inner()) {
        let e = expr(child)?;
        lhs = Some(match lhs {
            Some(l) => Expr::Binary {
                op,
                lhs: Box::new(l),
                rhs: Box::new(e),
            },
            None => e,
        });
    }
    Ok(lhs.unwrap())
}

/// Left-associative chain with interleaved operator pairs
/// (`AddExpr`, `MulExpr`).
fn op_chain(pair: Pair<Rule>, map: fn(&str) -> BinaryOp) -> Result<Expr, String> {
    let mut lhs: Option<Expr> = None;
    let mut pending_op: Option<BinaryOp> = None;
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::AddOp | Rule::MulOp => pending_op = Some(map(child.as_str())),
            _ => {
                let e = expr(child)?;
                lhs = Some(match (lhs, pending_op.take()) {
                    (Some(l), Some(op)) => Expr::Binary {
                        op,
                        lhs: Box::new(l),
                        rhs: Box::new(e),
                    },
                    (Some(_), None) => return err("internal: operator chain without operator"),
                    (None, _) => e,
                });
            }
        }
    }
    Ok(lhs.unwrap())
}

/// `PostfixExpr`: a primary followed by `(args)` / `.member` tails.
fn postfix_expr(pair: Pair<Rule>) -> Result<Expr, String> {
    let mut children = pair.into_inner();
    let first = children.next().unwrap();
    let mut base = primary(first)?;
    for child in children {
        match child.as_rule() {
            Rule::ArgParens => {
                let args: Vec<Expr> = child.into_inner().map(expr).collect::<Result<_, _>>()?;
                base = match base {
                    Expr::Var(name) => Expr::Call { name, args },
                    Expr::Member { obj, name } => Expr::MethodCall { obj, name, args },
                    other => other, // e.g. (expr)(args) — not supported; keep value
                };
            }
            Rule::MemberAccess => {
                base = Expr::Member {
                    obj: Box::new(base),
                    name: ident(child),
                };
            }
            _ => {}
        }
    }
    Ok(base)
}

fn primary(pair: Pair<Rule>) -> Result<Expr, String> {
    match pair.as_rule() {
        Rule::StringLit => {
            let s = pair.as_str();
            Ok(Expr::Lit(Lit::Str(s[1..s.len() - 1].replace("\"\"", "\""))))
        }
        Rule::Number => {
            let mut s = pair.as_str().replace('_', "");
            // a type suffix: `&HFFFF&` is a Long (positive), not the
            // Integer -1; a Double/Single suffix makes a float
            let suffix = s.chars().last().filter(|c| "&%!#@".contains(*c));
            if suffix.is_some() {
                s.pop();
            }
            if let Some(hex) = s.strip_prefix("&H").or_else(|| s.strip_prefix("&h")) {
                let v = i64::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
                // VB: &H8000 is the Integer -32768, &H8000& the Long 32768;
                // 8 digits wrap into a negative Long (&H8000000F)
                let v = match suffix {
                    None if hex.len() <= 4 && v >= 0x8000 => v - 0x10000,
                    _ => v,
                };
                Ok(Expr::Lit(Lit::Int(v as i32)))
            } else if let Some(oct) = s.strip_prefix("&O").or_else(|| s.strip_prefix("&o")) {
                let v = i64::from_str_radix(oct, 8).map_err(|e| e.to_string())?;
                Ok(Expr::Lit(Lit::Int(v as i32)))
            } else if s.contains('.')
                || s.to_ascii_lowercase().contains('e')
                || matches!(suffix, Some('!' | '#' | '@'))
            {
                Ok(Expr::Lit(Lit::Double(
                    s.parse::<f64>().map_err(|e| e.to_string())?,
                )))
            } else {
                match s.parse::<i32>() {
                    Ok(v) => Ok(Expr::Lit(Lit::Int(v))),
                    Err(_) => Ok(Expr::Lit(Lit::Double(
                        s.parse::<f64>().map_err(|e| e.to_string())?,
                    ))),
                }
            }
        }
        Rule::Ident => Ok(Expr::Var(pair.as_str().to_string())),
        // `.Caption` inside a `With` block: the name after the dot
        Rule::WithRef => Ok(Expr::WithRef(ident(pair))),
        Rule::Expr => expr(pair),
        _ => {
            // keyword literals: true / false / empty / null / nothing
            let kw = pair.as_str().trim().to_ascii_lowercase();
            match kw.as_str() {
                "true" => Ok(Expr::Lit(Lit::Bool(true))),
                "false" => Ok(Expr::Lit(Lit::Bool(false))),
                "empty" => Ok(Expr::Lit(Lit::Empty)),
                "null" => Ok(Expr::Lit(Lit::Null)),
                "nothing" => Ok(Expr::Lit(Lit::Nothing)),
                _ => err(&format!(
                    "internal: unexpected primary {:?}",
                    pair.as_rule()
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_program() {
        let prog = parse("Dim x As Long\nx = 42\nPrint \"x =\"; x\n").unwrap();
        // statements (excluding source-line markers)
        let stmts = prog
            .items
            .iter()
            .filter(|i| !matches!(i, Item::Stmt(Stmt::LineMarker(_))))
            .count();
        assert_eq!(stmts, 3);
        // each statement is preceded by its line marker
        let markers = prog
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Stmt(Stmt::LineMarker(l)) => Some(*l),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(markers, vec![1, 2, 3]);
    }

    #[test]
    fn parses_block_if() {
        let src = "If x > 1 Then\n  Print \"big\"\nElseIf x = 1 Then\n  Print \"one\"\nElse\n  Print \"small\"\nEnd If\n";
        parse(src).unwrap();
    }

    #[test]
    fn parses_single_line_if() {
        parse("If a Then b = 1 Else b = 2\n").unwrap();
    }

    #[test]
    fn parses_procs() {
        let src = "Function Add(a As Long, ByVal b As Long) As Long\n  Add = a + b\nEnd Function\nSub Main()\n  Print Add(1, 2)\nEnd Sub\n";
        let prog = parse(src).unwrap();
        assert_eq!(prog.items.len(), 2);
    }

    #[test]
    fn parses_for_and_foreach() {
        parse("For i = 1 To 10 Step 2\n  Print i\nNext\n").unwrap();
        parse("For Each x In arr\n  Print x\nNext\n").unwrap();
    }

    #[test]
    fn parses_do_forms() {
        parse("Do While x < 5\n  x = x + 1\nLoop\n").unwrap();
        parse("Do\n  x = x + 1\nLoop Until x > 5\n").unwrap();
        parse("Do\n  x = 1\nLoop\n").unwrap();
    }

    #[test]
    fn parses_select() {
        let src = "Select Case n\n  Case 1 To 5\n    Print \"low\"\n  Case Is > 10\n    Print \"high\"\n  Case 7, 9\n    Print \"odd\"\n  Case Else\n    Print \"other\"\nEnd Select\n";
        parse(src).unwrap();
    }

    #[test]
    fn parses_error_handling() {
        parse("On Error GoTo Handler\nOn Error Resume Next\nOn Error GoTo 0\nResume Next\nResume\nResume Handler\nHandler:\n").unwrap();
    }

    #[test]
    fn parses_member_calls_on_control_array_elements() {
        // the calls, without the debugger's line markers
        let calls = |src: &str| -> Vec<(Expr, Vec<Expr>)> {
            parse(src)
                .unwrap()
                .items
                .into_iter()
                .filter_map(|i| match i {
                    Item::Stmt(Stmt::Call { target, args, .. }) => Some((target, args)),
                    _ => None,
                })
                .collect()
        };
        let c = calls("List1(i + 1).AddItem \"x\", 0\nCommand1(2).SetFocus\n");
        assert_eq!(c.len(), 2);
        match &c[0].0 {
            Expr::Member { obj, name } => {
                assert_eq!(name, "AddItem");
                assert!(
                    matches!(&**obj, Expr::Call { name, args } if name == "List1" && args.len() == 1)
                );
            }
            other => panic!("unexpected target {:?}", other),
        }
        assert_eq!(c[0].1.len(), 2);
        // a plain call with a parenthesised first argument is unchanged
        for (target, _) in calls("DoIt (1)\nDoIt (1), 2\n") {
            assert!(
                matches!(&target, Expr::Var(n) if n == "DoIt"),
                "{:?}",
                target
            );
        }
    }

    #[test]
    fn parses_colon_separated_statements_and_labels() {
        parse("a = 1: b = 2\nTop: c = 3\n").unwrap();
    }

    #[test]
    fn parses_expression_precedence() {
        parse("x = 1 + 2 * 3 ^ 2 - 4 \\ 5 Mod 6\n").unwrap();
        parse("s = \"a\" & \"b\" & CStr(3)\n").unwrap();
        parse("f = Not a And b Or Not c Xor d\n").unwrap();
    }

    #[test]
    fn parses_hex_and_comments() {
        parse("' a comment\nx = &HFF ' trailing\nRem old-style comment\n").unwrap();
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("If Then\n").is_err());
        assert!(parse("For i = 1\n").is_err());
    }

    /// The one statement of `src` (its line markers left out).
    fn only(src: &str) -> Stmt {
        let prog = parse(src).unwrap_or_else(|e| panic!("{}: {}", src, e));
        let mut stmts: Vec<Stmt> = prog
            .items
            .into_iter()
            .filter_map(|i| match i {
                Item::Stmt(Stmt::LineMarker(_)) => None,
                Item::Stmt(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(stmts.len(), 1, "{}", src);
        stmts.remove(0)
    }

    #[test]
    fn parses_line_with_its_points_color_and_box_flag() {
        let s = only("Picture1.Line (10, 20)-(30, 40), vbRed, BF\n");
        let Stmt::Graphics { target, shape } = s else {
            panic!("{:?}", s)
        };
        assert!(matches!(target, Some(Expr::Var(ref n)) if n == "Picture1"));
        let Graphic::Line {
            from,
            to,
            color,
            boxed,
        } = shape
        else {
            panic!()
        };
        assert!(from.is_some_and(|p| !p.step));
        assert!(!to.step);
        assert!(color.is_some());
        assert_eq!(boxed, 2);
        // no from point, Step, a box without a colour, a target form
        let s = only("Form2.Picture1.Line -Step(5, 5), , B\n");
        let Stmt::Graphics { target, shape } = s else {
            panic!()
        };
        assert!(matches!(target, Some(Expr::Member { ref name, .. }) if name == "Picture1"));
        let Graphic::Line {
            from,
            to,
            color,
            boxed,
        } = shape
        else {
            panic!()
        };
        assert!(from.is_none() && to.step && color.is_none());
        assert_eq!(boxed, 1);
        // on the form itself, and on an array element
        assert!(matches!(
            only("Line (0, 0)-(1, 1)\n"),
            Stmt::Graphics { target: None, .. }
        ));
        assert!(matches!(
            only("Pic(2).PSet (3, 4), 0\n"),
            Stmt::Graphics {
                target: Some(Expr::Call { .. }),
                shape: Graphic::PSet { .. }
            }
        ));
    }

    #[test]
    fn parses_circle_with_left_out_arguments() {
        let s = only("Picture1.Circle (50, 50), 20, , 0, 3.14, 0.5\n");
        let Stmt::Graphics { shape, .. } = s else {
            panic!()
        };
        let Graphic::Circle {
            color,
            start,
            end,
            aspect,
            ..
        } = shape
        else {
            panic!()
        };
        assert!(color.is_none() && start.is_some() && end.is_some() && aspect.is_some());
        let s = only("Circle Step(1, 2), 5\n");
        let Stmt::Graphics { shape, target } = s else {
            panic!()
        };
        assert!(target.is_none());
        assert!(matches!(
            shape,
            Graphic::Circle {
                center: GPoint { step: true, .. },
                color: None,
                ..
            }
        ));
    }

    #[test]
    fn line_pset_and_circle_are_still_ordinary_names_elsewhere() {
        // a variable called Line, a sub called Circle, a property PSet
        assert!(matches!(only("line = 5\n"), Stmt::Assign { .. }));
        assert!(matches!(only("x = line + 1\n"), Stmt::Assign { .. }));
        assert!(matches!(only("Circle 5\n"), Stmt::Call { .. }));
        assert!(matches!(only("Obj.PSet = 3\n"), Stmt::Assign { .. }));
        assert!(matches!(only("Lines = 2\n"), Stmt::Assign { .. }));
    }

    #[test]
    fn parses_let_erase_redim_and_mid() {
        assert!(matches!(only("Let x = 5\n"), Stmt::Assign { .. }));
        assert!(matches!(only("Erase a, b\n"), Stmt::Erase(v) if v.len() == 2));
        assert!(matches!(
            only("ReDim a(3)\n"),
            Stmt::ReDim { preserve: false, .. }
        ));
        assert!(matches!(
            only("ReDim Preserve a(3), b(2, 1)\n"),
            Stmt::ReDim { preserve: true, vars } if vars.len() == 2
        ));
        assert!(matches!(
            only("Mid(s, 2, 3) = \"abc\"\n"),
            Stmt::MidAssign { length: Some(_), .. }
        ));
        assert!(matches!(
            only("Mid(s, 2) = \"abc\"\n"),
            Stmt::MidAssign { length: None, .. }
        ));
    }

    #[test]
    fn parses_the_option_forms() {
        assert!(matches!(only("Option Explicit\n"), Stmt::OptionExplicit));
        assert!(matches!(only("Option Base 0\n"), Stmt::OptionBase(0)));
        assert!(matches!(only("Option Base 1\n"), Stmt::OptionBase(1)));
        assert!(matches!(
            only("Option Compare Text\n"),
            Stmt::OptionCompareText(true)
        ));
        assert!(matches!(
            only("Option Compare Binary\n"),
            Stmt::OptionCompareText(false)
        ));
        assert!(parse("Option Base 2\n").is_err());
    }

    #[test]
    fn new_keywords_are_not_mistaken_for_names() {
        // as variables and in a dotted name, these stay ordinary names
        assert!(matches!(only("erase_count = 1\n"), Stmt::Assign { .. }));
        assert!(matches!(only("Mid = 3\n"), Stmt::Assign { .. }));
        assert!(matches!(only("letter = 2\n"), Stmt::Assign { .. }));
    }

    #[test]
    fn check_reports_where_a_line_fails_to_parse() {
        // valid source has no diagnostic
        assert_eq!(check("Print 1\nPrint 2\n"), None);
        // an unclosed parenthesis on line 2: the error points at line 2
        let e = check("Print 1\nPrint (2\nPrint 3\n").expect("an error");
        assert_eq!(e.line, 2);
        assert!(e.column >= 1);
        assert!(!e.message.is_empty());
    }

    #[test]
    fn check_and_parse_agree_on_what_is_an_error() {
        for src in ["Print 1\n", "Print (2\n", "If x Then\n", "Sub A()\nEnd Sub\n"] {
            assert_eq!(check(src).is_some(), parse(src).is_err(), "{:?}", src);
        }
    }
}
