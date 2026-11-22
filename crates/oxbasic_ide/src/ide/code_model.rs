//! Text-level model of a BASIC module for the code window: locating
//! `Sub`/`Function` blocks, inserting event-handler placeholders, and
//! renaming a control's identifiers.
//!
//! Everything here works on raw source lines, not the parsed AST, so it
//! keeps working while the code is mid-edit and doesn't parse.

use super::highlight::{tokenize_line, Tok};

/// One `Sub`/`Function` block: its name and the 0-based rows of its
/// declaration and `End Sub`/`End Function` lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Procedure {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

/// `s` without its leading keyword `word` (case-insensitive) and the
/// whitespace after it.
fn strip_word<'a>(s: &'a str, word: &str) -> Option<&'a str> {
    let head = s.get(..word.len())?;
    if !head.eq_ignore_ascii_case(word) {
        return None;
    }
    let rest = &s[word.len()..];
    match rest.chars().next() {
        Some(c) if c.is_whitespace() => Some(rest.trim_start()),
        _ => None,
    }
}

fn ident_prefix(s: &str) -> &str {
    let end = s
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    &s[..end]
}

/// The procedure name a declaration line opens, if it is one
/// (`[Public|Private|Friend] [Static] Sub|Function <name>`).
fn declared_name(line: &str) -> Option<String> {
    let mut s = line.trim_start();
    for vis in ["public", "private", "friend"] {
        if let Some(rest) = strip_word(s, vis) {
            s = rest;
            break;
        }
    }
    if let Some(rest) = strip_word(s, "static") {
        s = rest;
    }
    let rest = strip_word(s, "sub").or_else(|| strip_word(s, "function"))?;
    let name = ident_prefix(rest);
    (!name.is_empty()).then(|| name.to_string())
}

/// `End Sub` / `End Function`.
fn is_end_line(line: &str) -> bool {
    match strip_word(line.trim_start(), "end") {
        Some(rest) => {
            let w = ident_prefix(rest);
            w.eq_ignore_ascii_case("sub") || w.eq_ignore_ascii_case("function")
        }
        None => false,
    }
}

/// Every `Sub`/`Function` block in the module, in source order. A block
/// that is never closed runs to the last line.
pub fn find_procedures(lines: &[String]) -> Vec<Procedure> {
    let mut procs = Vec::new();
    let mut open: Option<(String, usize)> = None;
    for (row, line) in lines.iter().enumerate() {
        if open.is_none() {
            if let Some(name) = declared_name(line) {
                open = Some((name, row));
            }
        } else if is_end_line(line) {
            let (name, start) = open.take().unwrap();
            procs.push(Procedure {
                name,
                start,
                end: row,
            });
        }
    }
    if let Some((name, start)) = open {
        procs.push(Procedure {
            name,
            start,
            end: lines.len().saturating_sub(1),
        });
    }
    procs
}

/// The procedure whose block contains `row`.
pub fn proc_at(procs: &[Procedure], row: usize) -> Option<&Procedure> {
    procs.iter().find(|p| p.start <= row && row <= p.end)
}

/// Where the editor caret goes to edit procedure `p`: the first body
/// line, after its indentation (the declaration line for an empty block).
pub fn body_position(lines: &[String], p: &Procedure) -> (usize, usize) {
    let row = if p.start + 1 < p.end {
        p.start + 1
    } else {
        p.start
    };
    let col = lines[row].chars().take_while(|c| *c == ' ').count();
    (row, col)
}

/// Make sure a `Sub <handler>` exists. When it does, return the caret
/// position in its body; otherwise append the classic VB placeholder
///
/// ```text
/// Private Sub Command1_Click()
///     |
/// End Sub
/// ```
///
/// and return the position on its indented body line. The `bool` says
/// whether the placeholder was inserted. New code goes at the end of the
/// module so existing breakpoint line numbers don't move.
pub fn ensure_handler(
    lines: &mut Vec<String>,
    handler: &str,
    params: &str,
) -> (usize, usize, bool) {
    if let Some(p) = find_procedures(lines)
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(handler))
    {
        let (row, col) = body_position(lines, &p);
        return (row, col, false);
    }
    if lines.len() == 1 && lines[0].trim().is_empty() {
        lines.clear();
    }
    if lines.last().is_some_and(|l| !l.trim().is_empty()) {
        lines.push(String::new());
    }
    lines.push(format!("Private Sub {}({})", handler, params));
    lines.push("    ".to_string());
    lines.push("End Sub".to_string());
    (lines.len() - 2, 4, true)
}

/// Rename the identifier `old` to `new` wherever it appears as a bare
/// identifier (a control or form name), leaving strings and comments alone.
/// Returns how many changed.
pub fn rename_identifier(lines: &mut [String], old: &str, new: &str) -> usize {
    rename_control(lines, old, new, &[])
}

/// Rename control `old` to `new` in code: the bare identifier
/// (`old.Caption`) and its event handlers (`old_Click` -> `new_Click`, for
/// the event names in `events`). Strings and comments are left alone.
/// Returns how many identifiers changed.
pub fn rename_control(lines: &mut [String], old: &str, new: &str, events: &[&str]) -> usize {
    let n = old.len();
    let mut changed = 0;
    for line in lines.iter_mut() {
        let mut out = String::with_capacity(line.len());
        let mut line_changed = false;
        for (text, kind) in tokenize_line(line) {
            if kind == Tok::Ident {
                if text.eq_ignore_ascii_case(old) {
                    out.push_str(new);
                    changed += 1;
                    line_changed = true;
                    continue;
                }
                if text.len() > n + 1
                    && text.is_char_boundary(n)
                    && text[..n].eq_ignore_ascii_case(old)
                    && text.as_bytes()[n] == b'_'
                {
                    let event = &text[n + 1..];
                    if events.iter().any(|e| e.eq_ignore_ascii_case(event)) {
                        out.push_str(new);
                        out.push('_');
                        out.push_str(event);
                        changed += 1;
                        line_changed = true;
                        continue;
                    }
                }
            }
            out.push_str(&text);
        }
        if line_changed {
            *line = out;
        }
    }
    changed
}

/// A block that can be folded: its header row, and the row of its closing
/// line (`End Sub`, `Next`, `Loop`, ...). The rows between are hidden when
/// the block is folded; the header stays visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fold {
    pub start: usize,
    pub end: usize,
}

/// The first word of a line, lowercased, and what follows it.
fn first_word(line: &str) -> (String, &str) {
    let t = line.trim_start();
    let w = ident_prefix(t);
    (w.to_ascii_lowercase(), t[w.len()..].trim())
}

/// The closing word a line opens a foldable block with (`sub` closes with
/// `End Sub`, `for` with `Next`, ...), if it opens one.
fn opens_block(line: &str) -> Option<&'static str> {
    let (word, rest) = first_word(line);
    match word.as_str() {
        "sub" => Some("sub"),
        "function" => Some("function"),
        "property" => Some("property"),
        "for" => Some("next"),
        "while" => Some("wend"),
        "do" => Some("loop"),
        "with" => Some("with"),
        "type" => Some("type"),
        "enum" => Some("enum"),
        "select" if rest.to_ascii_lowercase().starts_with("case") => Some("select"),
        // a block If has nothing after its `Then`; a one-line If does not fold
        "if" if rest.to_ascii_lowercase().ends_with("then") => Some("if"),
        _ => None,
    }
}

/// Whether `line` closes a block that `closer` names.
fn closes_block(line: &str, closer: &str) -> bool {
    let (word, rest) = first_word(line);
    match closer {
        "next" => word == "next",
        "loop" => word == "loop",
        "wend" => word == "wend",
        _ if word == "end" => ident_prefix(rest).eq_ignore_ascii_case(closer),
        _ => false,
    }
}

/// Every foldable block in the module, nested ones included, in source order.
/// A block that is never closed is left out.
pub fn find_folds(lines: &[String]) -> Vec<Fold> {
    let mut folds = Vec::new();
    // open blocks, innermost last: (header row, the word that closes it)
    let mut open: Vec<(usize, &'static str)> = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        // the innermost open block closes first (a `Next` ends the nearest `For`)
        if let Some(&(start, closer)) = open.last() {
            if closes_block(line, closer) {
                open.pop();
                folds.push(Fold { start, end: row });
                continue;
            }
        }
        if let Some(closer) = opens_block(line) {
            open.push((row, closer));
        }
    }
    folds.sort_by_key(|f| f.start);
    folds
}

/// The rows hidden by the folded blocks: every row strictly inside a folded
/// block. A header row stays visible.
pub fn hidden_rows(
    folds: &[Fold],
    folded: &std::collections::BTreeSet<usize>,
) -> std::collections::HashSet<usize> {
    let mut hidden = std::collections::HashSet::new();
    for f in folds.iter().filter(|f| folded.contains(&f.start)) {
        hidden.extend(f.start + 1..f.end);
    }
    hidden
}

/// The fold that hides `row` while it is folded, if any. The innermost block
/// wins when folds are nested, so one click opens the one that is shown.
pub fn folded_block_hiding(
    folds: &[Fold],
    folded: &std::collections::BTreeSet<usize>,
    row: usize,
) -> Option<Fold> {
    folds
        .iter()
        .filter(|f| folded.contains(&f.start) && f.start < row && row <= f.end)
        .min_by_key(|f| f.end - f.start)
        .copied()
}



#[cfg(test)]
mod tests {
    use super::*;

    fn lines(src: &str) -> Vec<String> {
        src.lines().map(|l| l.to_string()).collect()
    }

    #[test]
    fn finds_procedures_with_visibility_and_static() {
        let l = lines(
            "Dim x As Long\n\
             Sub Command1_Click()\n\
             \x20   x = 1\n\
             End Sub\n\
             Private Static Function Twice(n As Long) As Long\n\
             \x20   Twice = n * 2\n\
             End Function\n\
             Public Sub Unclosed()\n\
             \x20   x = 2",
        );
        let procs = find_procedures(&l);
        let p = |name: &str, start, end| Procedure {
            name: name.into(),
            start,
            end,
        };
        assert_eq!(
            procs,
            vec![
                p("Command1_Click", 1, 3),
                p("Twice", 4, 6),
                p("Unclosed", 7, 8)
            ]
        );
        assert_eq!(proc_at(&procs, 0), None);
        assert_eq!(proc_at(&procs, 2).unwrap().name, "Command1_Click");
        assert_eq!(proc_at(&procs, 6).unwrap().name, "Twice");
        // `Subtotal = 1` and a bare `End` statement are not block boundaries
        let l = lines("Subtotal = 1\nSub A()\n  End\nEnd Sub");
        assert_eq!(find_procedures(&l), vec![p("A", 1, 3)]);
    }

    #[test]
    fn ensure_handler_finds_existing_code_case_insensitively() {
        let mut l = lines("Sub command1_click()\n    MsgBox \"hi\"\nEnd Sub");
        let before = l.clone();
        assert_eq!(ensure_handler(&mut l, "Command1_Click", ""), (1, 4, false));
        assert_eq!(l, before, "an existing handler must not be duplicated");
    }

    #[test]
    fn ensure_handler_appends_a_parseable_placeholder() {
        let mut l = lines("Dim x As Long");
        let (row, col, inserted) = ensure_handler(&mut l, "Text1_KeyPress", "KeyAscii As Integer");
        assert!(inserted);
        assert_eq!(
            l,
            lines(
                "Dim x As Long\n\nPrivate Sub Text1_KeyPress(KeyAscii As Integer)\n    \nEnd Sub"
            )
        );
        assert_eq!((row, col), (3, 4));
        oxbasic_parser::parse(&(l.join("\n") + "\n")).expect("placeholder must parse");

        // an empty module gets the placeholder at the top
        let mut empty = vec![String::new()];
        assert_eq!(ensure_handler(&mut empty, "Form_Load", ""), (1, 4, true));
        assert_eq!(empty[0], "Private Sub Form_Load()");
    }

    #[test]
    fn rename_touches_identifiers_and_handlers_only() {
        let mut l = lines(
            "Sub Command1_Click()\n\
             \x20   Command1.Caption = \"Command1\" ' Command1 clicked\n\
             \x20   Command10.Caption = \"x\"\n\
             \x20   Call Command1_Helper\n\
             End Sub\n\
             Private Sub command1_mousedown(Button As Integer)\n\
             End Sub",
        );
        let n = rename_control(&mut l, "Command1", "OkButton", &["Click", "MouseDown"]);
        assert_eq!(n, 3);
        assert_eq!(l[0], "Sub OkButton_Click()");
        assert_eq!(
            l[1],
            "    OkButton.Caption = \"Command1\" ' Command1 clicked"
        );
        assert_eq!(
            l[2], "    Command10.Caption = \"x\"",
            "a longer name must not match"
        );
        assert_eq!(
            l[3], "    Call Command1_Helper",
            "a non-event suffix must not match"
        );
        assert_eq!(l[5], "Private Sub OkButton_mousedown(Button As Integer)");
    }

    fn fold(start: usize, end: usize) -> Fold {
        Fold { start, end }
    }

    #[test]
    fn finds_the_blocks_of_each_kind() {
        let l = lines(
            "Sub A()\nIf x > 1 Then\n  y = 1\nEnd If\nFor i = 1 To 3\n  Print i\nNext i\nDo\n  x = 1\nLoop\nWhile x\nWend\nEnd Sub",
        );
        assert_eq!(
            find_folds(&l),
            vec![fold(0, 12), fold(1, 3), fold(4, 6), fold(7, 9), fold(10, 11)]
        );
    }

    #[test]
    fn nested_blocks_close_at_their_own_end() {
        let l = lines("For i = 1 To 2\nFor j = 1 To 2\nNext j\nNext i");
        assert_eq!(find_folds(&l), vec![fold(0, 3), fold(1, 2)]);
    }

    #[test]
    fn a_one_line_if_does_not_fold() {
        let l = lines("If x Then y = 1\nIf x Then\ny = 2\nEnd If");
        assert_eq!(find_folds(&l), vec![fold(1, 3)]);
    }

    #[test]
    fn select_with_type_and_enum_fold() {
        let l = lines(
            "Select Case x\nCase 1\nEnd Select\nWith p\n.X = 1\nEnd With\nType T\nN As Long\nEnd Type\nEnum E\nA\nEnd Enum",
        );
        assert_eq!(
            find_folds(&l),
            vec![fold(0, 2), fold(3, 5), fold(6, 8), fold(9, 11)]
        );
    }

    #[test]
    fn an_unclosed_block_is_not_a_fold() {
        assert_eq!(find_folds(&lines("Sub A()\nx = 1")), vec![]);
    }

    #[test]
    fn a_variable_named_like_a_keyword_is_not_a_block() {
        // `Subtotal`, `Format` and `Done` start with a block word but are not one
        let l = lines("Subtotal = 1\nFormat = 2\nDone = 3\nFor i = 1 To 2\nNext i");
        assert_eq!(find_folds(&l), vec![fold(3, 4)]);
    }

    fn folded_set(rows: &[usize]) -> std::collections::BTreeSet<usize> {
        rows.iter().copied().collect()
    }

    #[test]
    fn a_folded_block_hides_its_body_but_keeps_its_header_and_end() {
        // Sub (0..4) holds For (1..3)
        let l = lines("Sub A()\nFor i = 1 To 2\nPrint i\nNext i\nx = 1\nEnd Sub");
        let folds = find_folds(&l);
        assert_eq!(folds, vec![fold(0, 5), fold(1, 3)]);

        let hidden = hidden_rows(&folds, &folded_set(&[0]));
        // the Sub's body is hidden, including the nested For; its header (0)
        // and End Sub (5) stay visible
        assert!(!hidden.contains(&0));
        assert!(!hidden.contains(&5));
        assert_eq!(hidden, [1, 2, 3, 4].into_iter().collect());

        // folding just the For hides its body row, not its Next
        let hidden = hidden_rows(&folds, &folded_set(&[1]));
        assert_eq!(hidden, [2].into_iter().collect());
    }

    #[test]
    fn nothing_is_hidden_until_a_block_is_folded() {
        let folds = find_folds(&lines("For i = 1 To 2\nNext i"));
        assert!(hidden_rows(&folds, &folded_set(&[])).is_empty());
    }

    #[test]
    fn the_innermost_folded_block_is_the_one_that_hides_a_row() {
        let folds = find_folds(&lines("Sub A()\nFor i = 1 To 2\nPrint i\nNext i\nEnd Sub"));
        let both = folded_set(&[0, 1]);
        // row 2 is inside both folds: the For is the one to open
        assert_eq!(folded_block_hiding(&folds, &both, 2), Some(fold(1, 3)));
        // row 4 (End Sub) is hidden only by the Sub
        assert_eq!(folded_block_hiding(&folds, &both, 4), Some(fold(0, 4)));
        // a header row is never hidden
        assert_eq!(folded_block_hiding(&folds, &both, 0), None);
        // nothing hides a row when nothing is folded
        assert_eq!(folded_block_hiding(&folds, &folded_set(&[]), 2), None);
    }
}
