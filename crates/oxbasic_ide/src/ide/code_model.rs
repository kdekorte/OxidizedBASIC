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
}
