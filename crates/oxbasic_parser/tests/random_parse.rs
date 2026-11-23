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
fn open_random_with_len_parses() {
    let s = stmts("Open \"f.dat\" For Random As #1 Len = 64\n");
    assert!(matches!(s[0], Stmt::OpenFile { record_len: Some(_), .. }), "{:?}", s);
}

#[test]
fn get_and_put_with_and_without_a_record_number_parse() {
    let s = stmts("Get #1, 3, rec\nPut #1, , rec\n");
    assert!(matches!(s[0], Stmt::FileGet { pos: Some(_), .. }), "{:?}", s);
    assert!(matches!(s[1], Stmt::FilePut { pos: None, .. }), "{:?}", s);
}

#[test]
fn seek_parses() {
    let s = stmts("Seek #2, 10\n");
    assert!(matches!(s[0], Stmt::FileSeek { .. }), "{:?}", s);
}
