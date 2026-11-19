//! Building one program from a project's forms and modules.
//!
//! Every form has its own code: `Sub Command1_Click()` in `Form2` must not
//! clash with the one in `Form1`. The combined program therefore gets each
//! form's procedures renamed `Form2__Command1_Click`, and the calls and
//! variable uses that refer to them follow. Standard modules keep their
//! names so any form can call them. A [`LineMap`] sends the interpreter's
//! line numbers back to the document and line the user wrote.

use super::highlight::{tokenize_line, Tok};
use super::workspace::{Doc, DocKind};
use oxbasic_ui::project_host::handler_for;
use std::collections::HashSet;

/// Where a line of the combined program came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Origin {
    /// Index into the workspace's documents.
    pub doc: usize,
    /// 1-based line in that document.
    pub line: usize,
}

/// The combined source and the origin of each of its lines.
#[derive(Debug, Clone, Default)]
pub struct Program {
    pub source: String,
    /// `origins[n]` is where combined line `n + 1` came from; `None` for a
    /// generated line.
    pub origins: Vec<Option<Origin>>,
}

impl Program {
    /// The document line of combined line `line` (1-based).
    pub fn origin(&self, line: usize) -> Option<Origin> {
        self.origins.get(line.checked_sub(1)?).copied().flatten()
    }

    /// The combined line a breakpoint at `doc`'s `line` sits on.
    pub fn combined_line(&self, doc: usize, line: usize) -> Option<usize> {
        self.origins
            .iter()
            .position(|o| *o == Some(Origin { doc, line }))
            .map(|i| i + 1)
    }
}

/// The names a form's code declares at module level, which must be
/// prefixed: its procedures and its `Dim`/`Const` variables.
fn declared_names(lines: &[String]) -> HashSet<String> {
    let mut names: HashSet<String> = super::code_model::find_procedures(lines)
        .into_iter()
        .map(|p| p.name.to_ascii_lowercase())
        .collect();
    let procs = super::code_model::find_procedures(lines);
    for (row, line) in lines.iter().enumerate() {
        if procs.iter().any(|p| p.start <= row && row <= p.end) {
            continue; // locals stay local
        }
        let toks = tokenize_line(line);
        let words: Vec<&(String, Tok)> =
            toks.iter().filter(|(t, _)| !t.trim().is_empty()).collect();
        let Some((first, Tok::Keyword)) = words.first() else {
            continue;
        };
        if !matches!(
            first.to_ascii_lowercase().as_str(),
            "dim" | "const" | "public" | "private" | "global" | "static"
        ) {
            continue;
        }
        // `Dim a As Long, b(3) As String`: every identifier before `As`
        // or a comma, in declaration position
        let mut expect_name = true;
        for (text, kind) in words.iter().skip(1) {
            match kind {
                Tok::Ident if expect_name => {
                    names.insert(text.to_ascii_lowercase());
                    expect_name = false;
                }
                Tok::Punct if text.contains(',') => expect_name = true,
                Tok::Keyword if text.eq_ignore_ascii_case("as") => expect_name = false,
                _ => {}
            }
        }
    }
    names
}

/// The names declared by `Dim`/`Const`/parameters inside the procedure
/// spanning rows `start..=end` — these shadow the form-level names there.
fn local_names(lines: &[String], start: usize, end: usize) -> HashSet<String> {
    let mut names = HashSet::new();
    for line in &lines[start..=end.min(lines.len().saturating_sub(1))] {
        let toks = tokenize_line(line);
        let words: Vec<&(String, Tok)> =
            toks.iter().filter(|(t, _)| !t.trim().is_empty()).collect();
        let Some((first, Tok::Keyword)) = words.first() else {
            continue;
        };
        let f = first.to_ascii_lowercase();
        // the declaration line itself holds the parameters
        let is_decl = matches!(
            f.as_str(),
            "sub" | "function" | "public" | "private" | "friend"
        );
        let is_local = matches!(f.as_str(), "dim" | "const" | "static");
        if !is_decl && !is_local {
            continue;
        }
        let mut expect_name = is_local;
        let mut depth = 0;
        for (text, kind) in words.iter().skip(1) {
            match kind {
                Tok::Ident if expect_name && (is_local || depth == 1) => {
                    names.insert(text.to_ascii_lowercase());
                    expect_name = false;
                }
                Tok::Punct => {
                    depth += text.matches('(').count() as i32;
                    depth -= text.matches(')').count() as i32;
                    if text.contains(',') {
                        expect_name = true;
                    }
                    if is_decl && text.contains('(') {
                        expect_name = true;
                    }
                }
                Tok::Keyword if text.eq_ignore_ascii_case("as") => expect_name = false,
                Tok::Keyword
                    if is_decl
                        && matches!(
                            text.to_ascii_lowercase().as_str(),
                            "byval" | "byref" | "optional" | "paramarray"
                        ) =>
                {
                    expect_name = true
                }
                _ => {}
            }
        }
    }
    names
}

/// `lines` with every identifier in `names` prefixed by `form__`, except
/// inside a procedure that declares its own variable of that name.
/// Strings and comments are left alone.
fn prefix_identifiers(lines: &[String], form: &str, names: &HashSet<String>) -> Vec<String> {
    let procs = super::code_model::find_procedures(lines);
    let locals: Vec<HashSet<String>> = procs
        .iter()
        .map(|p| local_names(lines, p.start, p.end))
        .collect();
    lines
        .iter()
        .enumerate()
        .map(|(row, line)| {
            let shadow = procs
                .iter()
                .position(|p| p.start <= row && row <= p.end)
                .map(|i| &locals[i]);
            let mut out = String::with_capacity(line.len() + 8);
            for (text, kind) in tokenize_line(line) {
                let lower = text.to_ascii_lowercase();
                // the procedure's own name is always the form's
                let own_decl = procs
                    .iter()
                    .any(|p| p.start == row && p.name.eq_ignore_ascii_case(&text));
                let shadowed = !own_decl && shadow.is_some_and(|s| s.contains(&lower));
                if kind == Tok::Ident && names.contains(&lower) && !shadowed {
                    out.push_str(&handler_for(form, &text));
                } else {
                    out.push_str(&text);
                }
            }
            out
        })
        .collect()
}

/// Build the program for a project. Forms come first (the startup form's
/// code is what runs), then the standard modules. `FORM_SEP` keeps each
/// form's procedures and module-level variables apart.
pub fn combine(docs: &[Doc]) -> Program {
    let mut program = Program::default();
    let push = |program: &mut Program, text: String, origin: Option<Origin>| {
        program.origins.push(origin);
        program.source.push_str(&text);
        program.source.push('\n');
    };
    // modules' names are shared by every form, so a form's own names must
    // not shadow what a module declares
    let module_names: HashSet<String> = docs
        .iter()
        .filter(|d| d.kind == DocKind::Module)
        .flat_map(|d| declared_names(&d.lines))
        .collect();
    for (i, doc) in docs.iter().enumerate() {
        let lines: Vec<String> = match doc.kind {
            DocKind::Form => {
                let mut names = declared_names(&doc.lines);
                names.retain(|n| !module_names.contains(n));
                prefix_identifiers(&doc.lines, &doc.name, &names)
            }
            DocKind::Module => doc.lines.clone(),
        };
        for (n, line) in lines.into_iter().enumerate() {
            push(
                &mut program,
                line,
                Some(Origin {
                    doc: i,
                    line: n + 1,
                }),
            );
        }
    }
    program
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ide::workspace::to_lines;

    fn form(name: &str, code: &str) -> Doc {
        let mut d = Doc::new_form(name);
        d.lines = to_lines(code);
        d
    }

    fn module(name: &str, code: &str) -> Doc {
        let mut d = Doc::new_module(name);
        d.lines = to_lines(code);
        d
    }

    #[test]
    fn each_forms_handlers_and_variables_get_the_forms_prefix() {
        let docs = [
            form(
                "Form1",
                "Dim Clicks As Long\nSub Command1_Click()\n  Clicks = Clicks + 1\nEnd Sub",
            ),
            form("Form2", "Sub Command1_Click()\n  Print \"two\"\nEnd Sub"),
        ];
        let p = combine(&docs);
        assert!(
            p.source.contains("Dim Form1__Clicks As Long"),
            "{}",
            p.source
        );
        assert!(p.source.contains("Sub Form1__Command1_Click()"));
        assert!(p.source.contains("Form1__Clicks = Form1__Clicks + 1"));
        assert!(p.source.contains("Sub Form2__Command1_Click()"));
        oxbasic_parser::parse(&p.source).expect("the combined program parses");
    }

    #[test]
    fn locals_strings_and_comments_are_not_renamed() {
        let docs = [form(
            "Form1",
            "Dim Total As Long\nSub Add()\n  Dim Total As Long\n  Total = 1\n  Print \"Total\" ' Total\nEnd Sub",
        )];
        let src = combine(&docs).source;
        assert!(src.contains("Dim Form1__Total As Long"), "{}", src);
        // the procedure's own local `Total` keeps its name
        assert!(src.contains("  Dim Total As Long"), "{}", src);
        assert!(src.contains("Print \"Total\" ' Total"), "{}", src);
    }

    #[test]
    fn modules_keep_their_names_and_forms_can_call_them() {
        let docs = [
            form("Form1", "Sub Command1_Click()\n  Print Twice(4)\nEnd Sub"),
            module(
                "Module1",
                "Function Twice(n As Long) As Long\n  Twice = n * 2\nEnd Function",
            ),
        ];
        let src = combine(&docs).source;
        assert!(src.contains("Function Twice("), "{}", src);
        assert!(
            src.contains("Print Twice(4)"),
            "the call isn't prefixed: {}",
            src
        );
    }

    #[test]
    fn a_name_a_module_declares_is_shared_not_prefixed() {
        let docs = [
            form(
                "Form1",
                "Dim Shared1 As Long\nSub A()\n  Shared1 = 1\nEnd Sub",
            ),
            module("Module1", "Dim Shared1 As Long"),
        ];
        let src = combine(&docs).source;
        assert!(!src.contains("Form1__Shared1"), "{}", src);
    }

    #[test]
    fn lines_map_back_to_their_documents() {
        let docs = [
            form("Form1", "' one\nPrint 1"),
            module("Module1", "' two\nPrint 2"),
        ];
        let p = combine(&docs);
        assert_eq!(p.origin(1), Some(Origin { doc: 0, line: 1 }));
        assert_eq!(p.origin(4), Some(Origin { doc: 1, line: 2 }));
        assert_eq!(p.origin(99), None);
        assert_eq!(p.combined_line(1, 2), Some(4));
        assert_eq!(p.combined_line(5, 1), None);
    }
}
