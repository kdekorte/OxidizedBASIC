// Compiles the grammar's generated parser (src/parser.c, made by
// `tree-sitter generate` from grammar.js) into this crate.
fn main() {
    let src = std::path::Path::new("src");
    let mut build = cc::Build::new();
    build.std("c11").include(src).file(src.join("parser.c"));
    build.warnings(false);
    build.compile("tree-sitter-vb");
    println!("cargo:rerun-if-changed=src/parser.c");
    println!("cargo:rerun-if-changed=grammar.js");
}
