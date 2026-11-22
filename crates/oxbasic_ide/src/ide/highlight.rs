//! BASIC syntax highlighting tokenizer for the IDE code editor.
//!
//! Scans a source line into colored spans: keywords, strings, numbers,
//! comments, identifiers, and punctuation.

/// Syntax token kinds, mapped to colors by the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    Keyword,
    Str,
    Num,
    Comment,
    Ident,
    Punct,
}

const KEYWORDS: &[&str] = &[
    "if",
    "then",
    "else",
    "elseif",
    "end",
    "select",
    "case",
    "for",
    "each",
    "in",
    "to",
    "step",
    "next",
    "do",
    "loop",
    "while",
    "until",
    "wend",
    "sub",
    "function",
    "exit",
    "dim",
    "static",
    "public",
    "private",
    "global",
    "const",
    "set",
    "call",
    "goto",
    "on",
    "error",
    "resume",
    "option",
    "explicit",
    "mod",
    "and",
    "or",
    "not",
    "xor",
    "eqv",
    "imp",
    "is",
    "like",
    "as",
    "new",
    "byval",
    "byref",
    "optional",
    "paramarray",
    "true",
    "false",
    "empty",
    "null",
    "nothing",
    "print",
    "rem",
];

/// Whether `word` is a reserved BASIC keyword (case-insensitive) — such
/// words can't name a control.
pub fn is_keyword(word: &str) -> bool {
    KEYWORDS.contains(&word.to_ascii_lowercase().as_str())
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Tokenize one source line into (text, kind) spans, from the Tree-sitter
/// grammar. A line the grammar can't parse on its own (a block's first or
/// last line, say, or one mid-edit) is coloured by [`tokenize_line_plain`],
/// so colouring never gets worse than the plain tokenizer.
pub fn tokenize_line(line: &str) -> Vec<(String, Tok)> {
    match grammar_tokens(line) {
        Some(toks) => toks,
        None => tokenize_line_plain(line),
    }
}

/// The grammar's leaves for `line`, as spans, or `None` when the line has a
/// syntax error or its leaves don't cover its text.
fn grammar_tokens(line: &str) -> Option<Vec<(String, Tok)>> {
    thread_local! {
        static PARSER: std::cell::RefCell<tree_sitter::Parser> = std::cell::RefCell::new({
            let mut p = tree_sitter::Parser::new();
            p.set_language(&oxbasic_vb_grammar::LANGUAGE.into())
                .expect("the grammar loads");
            p
        });
    }
    if line.trim().is_empty() {
        return None;
    }
    // the grammar ends every line with a newline: parse the line with one, so
    // a line on its own is complete, then map the tokens back onto the line
    let source = format!("{line}\n");
    let tree = PARSER.with(|p| p.borrow_mut().parse(&source, None))?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }

    // every leaf, in order; a gap between two leaves is whitespace
    let mut out: Vec<(String, Tok)> = Vec::new();
    let mut pos = 0usize;
    let mut cursor = root.walk();
    let mut stack = vec![root];
    let mut leaves = Vec::new();
    while let Some(node) = stack.pop() {
        if node.child_count() == 0 {
            leaves.push(node);
        } else {
            let mut kids: Vec<_> = node.children(&mut cursor).collect();
            kids.reverse();
            stack.extend(kids);
        }
    }
    for node in leaves {
        let (start, end) = (node.start_byte(), node.end_byte());
        if start < pos || end > line.len() {
            return None;
        }
        if start > pos {
            // whitespace (or a continuation) between tokens
            out.push((line[pos..start].to_string(), Tok::Punct));
        }
        let kind = leaf_kind(&node);
        out.push((line[start..end].to_string(), kind));
        pos = end;
    }
    if pos < line.len() {
        out.push((line[pos..].to_string(), Tok::Punct));
    }
    let joined: String = out.iter().map(|(t, _)| t.as_str()).collect();
    (joined == line).then_some(out)
}

/// The colour kind of one grammar leaf.
fn leaf_kind(node: &tree_sitter::Node) -> Tok {
    match node.kind() {
        "comment" | "apostrophe_comment" => Tok::Comment,
        "string" => Tok::Str,
        "number" | "date_literal" => Tok::Num,
        "identifier" => Tok::Ident,
        // a built-in type (Integer, String) is coloured as a keyword, as VB does
        "builtin_type" => Tok::Keyword,
        // keywords are anonymous leaves whose kind is the keyword's text
        k if k.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) => Tok::Keyword,
        _ => Tok::Punct,
    }
}


/// Tokenize one source line into (text, kind) spans with the plain line
/// tokenizer: the fallback for a line the grammar can't parse.
pub fn tokenize_line_plain(line: &str) -> Vec<(String, Tok)> {

    let mut out = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        // comment: apostrophe anywhere, or Rem at a word boundary
        if chars[i] == '\'' {
            out.push((rest, Tok::Comment));
            break;
        }
        if chars[i].is_ascii_alphabetic()
            && chars[i].to_ascii_lowercase() == 'r'
            && chars.len() >= i + 3
            && chars[i + 1].to_ascii_lowercase() == 'e'
            && chars[i + 2].to_ascii_lowercase() == 'm'
            && (i == 0 || !is_ident_char(chars[i - 1]))
            && (i + 3 >= chars.len() || !is_ident_char(chars[i + 3]))
        {
            out.push((rest, Tok::Comment));
            break;
        }
        if chars[i] == '"' {
            let mut j = i + 1;
            while j < chars.len() {
                if chars[j] == '"' {
                    if j + 1 < chars.len() && chars[j + 1] == '"' {
                        j += 2; // escaped ""
                        continue;
                    }
                    j += 1;
                    break;
                }
                j += 1;
            }
            let s: String = chars[i..j.min(chars.len())].iter().collect();
            out.push((s, Tok::Str));
            i = j.min(chars.len());
            continue;
        }
        if chars[i].is_ascii_digit()
            || (chars[i] == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let mut j = i;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            // exponent / hex suffix
            if j < chars.len() && (chars[j] == 'e' || chars[j] == 'E') {
                j += 1;
                if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                    j += 1;
                }
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
            }
            let s: String = chars[i..j].iter().collect();
            out.push((s, Tok::Num));
            i = j;
            continue;
        }
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let mut j = i;
            while j < chars.len() && is_ident_char(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            let kind = if KEYWORDS.contains(&word.to_ascii_lowercase().as_str()) {
                Tok::Keyword
            } else {
                Tok::Ident
            };
            out.push((word, kind));
            i = j;
            continue;
        }
        // punctuation / whitespace runs
        let mut j = i;
        while j < chars.len()
            && !chars[j].is_ascii_alphanumeric()
            && chars[j] != '_'
            && chars[j] != '"'
            && chars[j] != '\''
        {
            j += 1;
        }
        let s: String = chars[i..j.max(i + 1)].iter().collect();
        out.push((s, Tok::Punct));
        i = j.max(i + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_a_line() {
        let toks = tokenize_line("If x >= 10 Then Print \"hi\" ' note");
        let kinds: Vec<Tok> = toks.iter().map(|(_, k)| *k).collect();
        assert_eq!(kinds[0], Tok::Keyword); // If
        let joined: String = toks.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(joined, "If x >= 10 Then Print \"hi\" ' note");
        assert!(kinds.contains(&Tok::Str));
        assert_eq!(*kinds.last().unwrap(), Tok::Comment);
    }

    #[test]
    fn rem_comment_and_numbers() {
        let toks = tokenize_line("Rem old");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].1, Tok::Comment);
        let toks = tokenize_line("x = 3.5");
        assert!(toks.iter().any(|(t, k)| *k == Tok::Num && t == "3.5"));
    }

    /// The kinds of `line`'s tokens, with the text of each, for the tests below.
    fn kinds(line: &str) -> Vec<(String, Tok)> {
        tokenize_line(line)
    }

    #[test]
    fn the_grammar_colours_a_keyword_in_any_case() {
        let toks = kinds("dIM who As String");
        assert!(toks.contains(&("dIM".to_string(), Tok::Keyword)), "{:?}", toks);
        assert!(toks.contains(&("As".to_string(), Tok::Keyword)), "{:?}", toks);
        assert!(toks.contains(&("who".to_string(), Tok::Ident)), "{:?}", toks);
    }

    #[test]
    fn the_grammar_keeps_the_whole_line_and_its_spacing() {
        let line = "lbl.Caption = \"a\" & b";
        let joined: String = kinds(line).iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(joined, line, "every character is in some token");
        let toks = kinds(line);
        assert!(toks.contains(&("\"a\"".to_string(), Tok::Str)), "{:?}", toks);
        assert!(toks.contains(&("Caption".to_string(), Tok::Ident)), "{:?}", toks);
    }

    #[test]
    fn a_line_with_an_apostrophe_comment_is_a_comment_at_its_end() {
        let toks = kinds("x = 1 ' a note");
        assert_eq!(toks.last().map(|(_, k)| *k), Some(Tok::Comment), "{:?}", toks);
    }

    #[test]
    fn a_line_the_grammar_cannot_parse_falls_back_to_the_plain_tokenizer() {
        // a block's first line on its own is a syntax error, so the plain
        // tokenizer colours it: Sub is a keyword there too
        let toks = kinds("Sub Form_Load()");
        assert_eq!(toks[0], ("Sub".to_string(), Tok::Keyword));
        assert_eq!(toks, tokenize_line_plain("Sub Form_Load()"));
    }
}

#[cfg(test)]
mod grammar_used_tests {
    use super::*;

    #[test]
    fn a_statement_line_is_coloured_by_the_grammar() {
        // the plain tokenizer would also colour these, so the test checks the
        // grammar's own result: a parse that the grammar accepts
        for line in ["x = 1", "Print \"hi\"", "lbl.Caption = \"a\" & b", "Dim n As Integer"] {
            assert!(grammar_tokens(line).is_some(), "the grammar didn't take {:?}", line);
        }
    }

    #[test]
    fn a_block_line_on_its_own_falls_back() {
        // End Sub only means something inside its procedure
        assert!(grammar_tokens("End Sub").is_none());
        assert!(grammar_tokens("Sub Form_Load()").is_none());
    }
}
