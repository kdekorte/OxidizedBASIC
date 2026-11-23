use oxbasic_ast::{Item, Stmt};

fn stmts(src: &str) -> Vec<Stmt> {
    let p = oxbasic_parser::parse(src).expect("parses");
    p.items
        .into_iter()
        .filter_map(|i| match i {
            Item::Stmt(s) => Some(s),
            _ => None,
        })
        .filter(|s| !matches!(s, Stmt::LineMarker(_)))
        .collect()
}

#[test]
fn a_fixed_length_string_keeps_its_size_in_the_type_name() {
    let s = stmts("Type Rec\n    Name As String * 10\n    Age As Long\nEnd Type\nDim r As String * 5\n");
    let dbg = format!("{:?}", s);
    assert!(dbg.contains("String*10") || dbg.contains("string*10"), "{}", dbg);
    assert!(dbg.contains("String*5") || dbg.contains("string*5"), "{}", dbg);
}
