//! The Tree-sitter grammar for OxidizedBASIC's VB6-style BASIC, used by the
//! IDE's code editor to colour and read the code. `grammar.js` is the source;
//! `src/parser.c` is generated from it with `tree-sitter generate`.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_vb() -> *const ();
}

/// The language, for `tree_sitter::Parser::set_language`.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_vb) };

/// The highlight queries (`queries/highlights.scm`).
pub const HIGHLIGHTS_QUERY: &str = include_str!("../queries/highlights.scm");

#[cfg(test)]
mod tests {
    #[test]
    fn the_showcase_parses_without_errors() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("the grammar loads");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase");
        for entry in std::fs::read_dir(dir).expect("showcase folder") {
            let path = entry.expect("entry").path();
            if path.extension().is_some_and(|e| e == "bas") {
                let src = std::fs::read_to_string(&path).expect("reads");
                let tree = parser.parse(&src, None).expect("parses");
                assert!(
                    !tree.root_node().has_error(),
                    "{} has a syntax error",
                    path.display()
                );
            }
        }
    }

    /// The node kinds of every named node in `src`, in tree order.
    fn node_kinds(src: &str) -> Vec<String> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("the grammar loads");
        let tree = parser.parse(src, None).expect("parses");
        let mut kinds = Vec::new();
        let mut cursor = tree.walk();
        loop {
            if cursor.node().is_named() {
                kinds.push(cursor.node().kind().to_string());
            }
            if cursor.goto_first_child() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    return kinds;
                }
            }
        }
    }

    #[test]
    fn a_hex_literal_is_one_number_and_not_a_concatenation() {
        let kinds = node_kinds("x = &HFF\n");
        assert!(kinds.iter().any(|k| k == "number"), "{:?}", kinds);
        assert!(!kinds.iter().any(|k| k == "binary_expression"), "{:?}", kinds);
    }

    #[test]
    fn a_built_in_type_is_read_in_any_case() {
        let kinds = node_kinds("Dim n As integer\n");
        assert!(kinds.iter().any(|k| k == "builtin_type"), "{:?}", kinds);
    }
}

