//! Slot resolution, step 1: the variable names a procedure (or the main
//! module) uses, in a stable order, so each can later get an index. Pure: it
//! reads the AST and reports names, and changes nothing at run time.

use oxbasic_ast::{CaseItem, Expr, GPoint, Graphic, PrintItem, Stmt, VarDecl};

/// The slots of one procedure (or the main module): each name it uses, lowercased,
/// with its index. The index is fixed for the procedure's life, so a frame's
/// slot vector has one entry per index.
#[derive(Debug, Clone, Default)]
pub struct SlotTable {
    index: std::collections::HashMap<String, usize>,
    names: Vec<String>,
}

impl SlotTable {
    /// The slot for `name` (any case), if the procedure uses it.
    pub fn slot(&self, name: &str) -> Option<usize> {
        self.index.get(&name.to_lowercase()).copied()
    }

    /// How many slots the table has.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the table has no slots.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// The slot table of a procedure: its parameters first (so a parameter's slot
/// is its position), then every other name its body uses, in first-use order.
pub fn table_for(params: &[oxbasic_ast::Param], body: &[Stmt]) -> SlotTable {
    let mut names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut add = |n: &str, names: &mut Vec<String>| {
        let l = n.to_lowercase();
        if seen.insert(l.clone()) {
            names.push(l);
        }
    };
    for p in params {
        add(&p.name, &mut names);
    }
    for n in names_in_stmts(body) {
        add(&n, &mut names);
    }
    let index = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();
    SlotTable { index, names }
}

/// Every variable name a list of statements mentions, in first-use order and
/// without repeats (names are kept as written; the caller lowercases them).
pub fn names_in_stmts(stmts: &[Stmt]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in stmts {
        stmt_names(s, &mut out);
    }
    out
}

fn push(out: &mut Vec<String>, name: &str) {
    if !out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
        out.push(name.to_string());
    }
}

fn expr_names(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Lit(_) => {}
        Expr::Var(name) => push(out, name),
        // a `.Member` inside `With` names no variable of its own
        Expr::WithRef(_) => {}
        // an array element or a call: the name may be a variable (an array)
        Expr::Call { name, args } => {
            push(out, name);
            for a in args {
                expr_names(a, out);
            }
        }
        Expr::Member { obj, .. } => expr_names(obj, out),
        Expr::MethodCall { obj, args, .. } => {
            expr_names(obj, out);
            for a in args {
                expr_names(a, out);
            }
        }
        Expr::Unary { expr, .. } => expr_names(expr, out),
        Expr::Binary { lhs, rhs, .. } => {
            expr_names(lhs, out);
            expr_names(rhs, out);
        }
    }
}

fn opt_expr(e: &Option<Expr>, out: &mut Vec<String>) {
    if let Some(e) = e {
        expr_names(e, out);
    }
}

fn decl_names(decls: &[VarDecl], out: &mut Vec<String>) {
    for d in decls {
        push(out, &d.name);
        if let Some(dims) = &d.dims {
            for e in dims {
                expr_names(e, out);
            }
        }
    }
}

fn print_names(items: &[PrintItem], out: &mut Vec<String>) {
    for it in items {
        expr_names(&it.expr, out);
    }
}

fn gpoint_names(p: &GPoint, out: &mut Vec<String>) {
    expr_names(&p.x, out);
    expr_names(&p.y, out);
}

fn graphic_names(g: &Graphic, out: &mut Vec<String>) {
    match g {
        Graphic::Line { from, to, color, .. } => {
            if let Some(f) = from {
                gpoint_names(f, out);
            }
            gpoint_names(to, out);
            opt_expr(color, out);
        }
        Graphic::PSet { at, color } => {
            gpoint_names(at, out);
            opt_expr(color, out);
        }
        Graphic::Circle {
            center,
            radius,
            color,
            start,
            end,
            aspect,
        } => {
            gpoint_names(center, out);
            expr_names(radius, out);
            opt_expr(color, out);
            opt_expr(start, out);
            opt_expr(end, out);
            opt_expr(aspect, out);
        }
    }
}

fn case_names(items: &[CaseItem], out: &mut Vec<String>) {
    for it in items {
        match it {
            CaseItem::Range(a, b) => {
                expr_names(a, out);
                expr_names(b, out);
            }
            CaseItem::Is(_, e) | CaseItem::Value(e) => expr_names(e, out),
        }
    }
}

fn stmt_names(s: &Stmt, out: &mut Vec<String>) {
    match s {
        Stmt::LineMarker(_) | Stmt::Label(_) | Stmt::Stop | Stmt::Return | Stmt::End => {}
        Stmt::Dim(decls) | Stmt::ReDim { vars: decls, .. } => decl_names(decls, out),
        Stmt::Const(decls) => {
            for d in decls {
                push(out, &d.name);
                expr_names(&d.value, out);
            }
        }
        Stmt::Assign { target, value } => {
            expr_names(target, out);
            expr_names(value, out);
        }
        Stmt::If { arms, else_body } => {
            for a in arms {
                expr_names(&a.cond, out);
                for s in &a.body {
                    stmt_names(s, out);
                }
            }
            for s in else_body.iter().flatten() {
                stmt_names(s, out);
            }
        }
        Stmt::Select { expr, arms, else_body } => {
            expr_names(expr, out);
            for a in arms {
                case_names(&a.items, out);
                for s in &a.body {
                    stmt_names(s, out);
                }
            }
            for s in else_body.iter().flatten() {
                stmt_names(s, out);
            }
        }
        Stmt::For { var, start, end, step, body } => {
            push(out, var);
            expr_names(start, out);
            expr_names(end, out);
            opt_expr(step, out);
            for s in body {
                stmt_names(s, out);
            }
        }
        Stmt::ForEach { var, coll, body } => {
            push(out, var);
            expr_names(coll, out);
            for s in body {
                stmt_names(s, out);
            }
        }
        Stmt::DoWhile { cond, body, .. } | Stmt::While { cond, body } => {
            expr_names(cond, out);
            for s in body {
                stmt_names(s, out);
            }
        }
        Stmt::DoLoop { body, cond } => {
            for s in body {
                stmt_names(s, out);
            }
            if let Some((e, _)) = cond {
                expr_names(e, out);
            }
        }
        Stmt::With { obj, body } => {
            expr_names(obj, out);
            for s in body {
                stmt_names(s, out);
            }
        }
        // a type's field names are members, not variables of the procedure
        Stmt::TypeDecl { .. } => {}
        Stmt::EnumDecl { members, .. } => {
            for (_, e) in members {
                expr_names(e, out);
            }
        }
        Stmt::Call { target, args, .. } => {
            expr_names(target, out);
            for a in args {
                expr_names(a, out);
            }
        }
        Stmt::Print { items, .. } => print_names(items, out),
        Stmt::Graphics { target, shape } => {
            opt_expr(target, out);
            graphic_names(shape, out);
        }
        Stmt::Exit(_) | Stmt::GoSub(_) | Stmt::GoTo(_) | Stmt::OnError(_) | Stmt::Resume(_) => {}
        Stmt::Erase(exprs) | Stmt::CloseFile(exprs) => {
            for e in exprs {
                expr_names(e, out);
            }
        }
        Stmt::OptionExplicit | Stmt::OptionBase(_) | Stmt::OptionCompareText(_) => {}
        Stmt::OpenFile { path, number, record_len, .. } => {
            expr_names(path, out);
            expr_names(number, out);
            opt_expr(record_len, out);
        }
        Stmt::FileGet { number, pos, target } => {
            expr_names(number, out);
            opt_expr(pos, out);
            expr_names(target, out);
        }
        Stmt::FilePut { number, pos, value } => {
            expr_names(number, out);
            opt_expr(pos, out);
            expr_names(value, out);
        }
        Stmt::FileSeek { number, pos } => {
            expr_names(number, out);
            expr_names(pos, out);
        }
        Stmt::LockFile { number, range } | Stmt::UnlockFile { number, range } => {
            expr_names(number, out);
            if let Some((a, b)) = range {
                expr_names(a, out);
                opt_expr(b, out);
            }
        }
        Stmt::InputFile { number, targets } => {
            expr_names(number, out);
            for t in targets {
                expr_names(t, out);
            }
        }
        Stmt::LineInputFile { number, target } => {
            expr_names(number, out);
            expr_names(target, out);
        }
        Stmt::PrintFile { number, items, .. } => {
            expr_names(number, out);
            print_names(items, out);
        }
        Stmt::DefType(_) => {}
        Stmt::MidAssign { target, start, length, value } => {
            expr_names(target, out);
            expr_names(start, out);
            opt_expr(length, out);
            expr_names(value, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names the walker finds in a program's statements (the module's own
    /// top-level statements), lowercased and sorted for comparison.

    /// The slot table of the first procedure in `src`.
    fn table_of(src: &str) -> SlotTable {
        let prog = oxbasic_parser::parse(src).expect("the source parses");
        for item in prog.items {
            if let oxbasic_ast::Item::Proc(p) = item {
                return table_for(&p.params, &p.body);
            }
        }
        panic!("no procedure in the source");
    }

    #[test]
    fn parameters_take_the_first_slots_in_order() {
        let t = table_of("Sub Add(a, b)\n  c = a + b\nEnd Sub\n");
        assert_eq!(t.slot("a"), Some(0));
        assert_eq!(t.slot("b"), Some(1));
        assert_eq!(t.slot("c"), Some(2));
    }

    #[test]
    fn slot_lookup_ignores_case() {
        let t = table_of("Sub S(Total)\n  total = total + 1\nEnd Sub\n");
        assert_eq!(t.slot("TOTAL"), Some(0));
        assert_eq!(t.slot("total"), Some(0));
    }

    #[test]
    fn every_name_the_body_uses_has_a_slot_and_nothing_else_does() {
        let t = table_of("Sub S()\n  x = y\n  For i = 1 To 3\n  Next i\nEnd Sub\n");
        for n in ["x", "y", "i"] {
            assert!(t.slot(n).is_some(), "{} has a slot", n);
        }
        assert_eq!(t.slot("zzz"), None, "an unused name has no slot");
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn slots_are_unique() {
        let t = table_of("Sub S(a)\n  b = a\n  c = b\nEnd Sub\n");
        let mut slots: Vec<usize> = ["a", "b", "c"].iter().map(|n| t.slot(n).unwrap()).collect();
        slots.sort();
        slots.dedup();
        assert_eq!(slots.len(), 3);
    }

    fn names_of(src: &str) -> Vec<String> {
        let prog = oxbasic_parser::parse(src).expect("the source parses");
        let stmts: Vec<Stmt> = prog
            .items
            .into_iter()
            .filter_map(|i| match i {
                oxbasic_ast::Item::Stmt(s) => Some(s),
                _ => None,
            })
            .collect();
        let mut v: Vec<String> = names_in_stmts(&stmts).iter().map(|n| n.to_lowercase()).collect();
        v.sort();
        v
    }

    #[test]
    fn assignment_finds_both_sides() {
        assert_eq!(names_of("x = y + 1\n"), vec!["x", "y"]);
    }

    #[test]
    fn dim_and_const_declare_their_names() {
        assert_eq!(names_of("Dim a As Long, b(3)\n"), vec!["a", "b"]);
        assert_eq!(names_of("Const k = 2\n"), vec!["k"]);
    }

    #[test]
    fn for_finds_its_counter_bounds_and_step() {
        assert_eq!(names_of("For i = a To b Step c\nNext i\n"), vec!["a", "b", "c", "i"]);
    }

    #[test]
    fn for_each_finds_its_variable_and_collection() {
        assert_eq!(names_of("For Each v In items\nNext v\n"), vec!["items", "v"]);
    }

    #[test]
    fn if_finds_conditions_and_bodies() {
        assert_eq!(
            names_of("If p Then\n  q = 1\nElseIf r Then\n  s = 2\nElse\n  t = 3\nEnd If\n"),
            vec!["p", "q", "r", "s", "t"]
        );
    }

    #[test]
    fn select_case_finds_the_selector_and_every_case_item() {
        assert_eq!(
            names_of("Select Case x\nCase lo To hi\n  a = 1\nCase Is > m\n  b = 2\nCase k\nEnd Select\n"),
            vec!["a", "b", "hi", "k", "lo", "m", "x"]
        );
    }

    #[test]
    fn do_and_while_loops_find_their_conditions() {
        assert_eq!(names_of("Do While c\n  n = n + 1\nLoop\n"), vec!["c", "n"]);
        assert_eq!(names_of("Do\n  m = 1\nLoop Until d\n"), vec!["d", "m"]);
        assert_eq!(names_of("While e\n  f = 1\nWend\n"), vec!["e", "f"]);
    }

    #[test]
    fn with_finds_the_object() {
        assert_eq!(names_of("With obj\n  .Caption = s\nEnd With\n"), vec!["obj", "s"]);
    }

    #[test]
    fn print_finds_each_item() {
        assert_eq!(names_of("Print a; b, c\n"), vec!["a", "b", "c"]);
    }

    #[test]
    fn call_and_its_arguments_are_found() {
        assert_eq!(names_of("Helper p, q\n"), vec!["helper", "p", "q"]);
    }

    #[test]
    fn a_name_used_as_an_array_element_is_found() {
        assert_eq!(names_of("v(i) = w(j)\n"), vec!["i", "j", "v", "w"]);
    }

    #[test]
    fn file_statements_find_their_names() {
        assert_eq!(names_of("Open p For Input As #n\n"), vec!["n", "p"]);
        assert_eq!(names_of("Close #h\n"), vec!["h"]);
        assert_eq!(names_of("Input #n, a, b\n"), vec!["a", "b", "n"]);
        assert_eq!(names_of("Lock #f, r1 To r2\n"), vec!["f", "r1", "r2"]);
    }

    #[test]
    fn graphics_find_their_points_and_colour() {
        assert_eq!(names_of("Line (x1, y1)-(x2, y2), col\n"), vec!["col", "x1", "x2", "y1", "y2"]);
        assert_eq!(names_of("PSet (px, py), c\n"), vec!["c", "px", "py"]);
    }

    #[test]
    fn a_name_is_reported_once() {
        assert_eq!(names_of("x = x + x\n"), vec!["x"]);
    }
}
