//! The JIT's compiled `For` loops against the interpreter: each loop runs both
//! ways and must end with the same variable values, the counter included, or
//! the compiled answer must be refused so the interpreter runs it.

use super::*;
use oxbasic_ast::Item;

struct Buf(String);
impl crate::Output for Buf {
    fn write(&mut self, s: &str) {
        self.0.push_str(s);
    }
}

/// The first `For` loop of `src`.
fn the_loop(src: &str) -> Stmt {
    let prog = oxbasic_parser::parse(src).expect("parses");
    prog.items
        .into_iter()
        .find_map(|i| match i {
            Item::Stmt(s @ Stmt::For { .. }) => Some(s),
            _ => None,
        })
        .expect("a For loop")
}

/// Run `loop_src` (a `For` loop over `names`, which start at `start`) both
/// ways. Returns the compiled values (or `None`) and the interpreter's values.
fn both(loop_src: &str, names: &[&str], start: &[i64]) -> (Option<Vec<i64>>, Vec<i64>) {
    let Stmt::For { var, start: s, end, step, body } = the_loop(loop_src) else {
        unreachable!()
    };
    let owned: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    let jit = compile_for(&var, &s, &end, step.as_ref(), &body, &owned).and_then(|c| c.run(start));
    let mut program = String::new();
    for (n, v) in names.iter().zip(start) {
        program.push_str(&format!("{} = {}\n", n, v));
    }
    program.push_str(loop_src);
    program.push('\n');
    for n in names {
        program.push_str(&format!("Print {}\n", n));
    }
    let mut buf = Buf(String::new());
    let interp = match crate::run_source(&program, &mut buf) {
        Ok(()) => buf.0.lines().filter_map(|l| l.trim().parse().ok()).collect(),
        Err(_) => Vec::new(),
    };
    (jit, interp)
}

#[test]
fn a_loop_gives_the_interpreters_values_and_final_counter() {
    let (jit, interp) = both("For i = 0 To 9\n  s = s + i\nNext i", &["i", "s"], &[0, 0]);
    assert_eq!(interp, vec![9, 45], "the interpreter leaves the counter at its last value");
    assert_eq!(jit, Some(interp));
}

#[test]
fn the_basica_loop_gives_the_interpreters_answer() {
    let src = "For i = 0 To 999\n  If i Mod 2 = 0 Then c = c + i Else c = c - i\nNext i";
    let (jit, interp) = both(src, &["i", "c"], &[0, 0]);
    assert_eq!(interp, vec![999, -500]);
    assert_eq!(jit, Some(interp));
}

#[test]
fn negative_mod_and_integer_division_truncate_as_the_interpreter_does() {
    let src = "For i = -5 To 5\n  m = m + i Mod 3\n  d = d + i \\ 2\nNext i";
    let (jit, interp) = both(src, &["i", "m", "d"], &[0, 0, 0]);
    assert_eq!(interp.len(), 3);
    assert_eq!(jit, Some(interp));
}

#[test]
fn comparisons_and_logic_are_vbs_true_and_false() {
    let src = "For i = 1 To 6\n  If i > 2 And i < 5 Then s = s + 1\n  If Not (i = 3) Then t = t + 1\n  u = u + (i >= 4)\nNext i";
    let (jit, interp) = both(src, &["i", "s", "t", "u"], &[0, 0, 0, 0]);
    assert_eq!(interp.len(), 4);
    assert_eq!(jit, Some(interp));
}

#[test]
fn steps_and_a_loop_that_never_runs_match() {
    for src in [
        "For i = 10 To 1 Step -3\n  s = s + i\nNext i",
        "For i = 5 To 1\n  s = s + 1\nNext i",
        "For i = 1 To 10 Step 4\n  s = s + i\nNext i",
    ] {
        let (jit, interp) = both(src, &["i", "s"], &[0, 0]);
        assert_eq!(interp.len(), 2, "{}", src);
        assert_eq!(jit, Some(interp), "{}", src);
    }
}

#[test]
fn nested_loops_match() {
    let src = "For i = 1 To 4\n  For j = 1 To 3\n    s = s + i * j\n  Next j\nNext i";
    let (jit, interp) = both(src, &["i", "j", "s"], &[0, 0, 0]);
    assert_eq!(interp.len(), 3);
    assert_eq!(jit, Some(interp));
}

#[test]
fn an_overflow_a_zero_divisor_or_a_zero_step_is_refused() {
    let (jit, _) = both("For i = 1 To 2\n  s = s * 100000\nNext i", &["i", "s"], &[0, 99999]);
    assert_eq!(jit, None, "a result past Long is refused");
    let (jit, _) = both("For i = 1 To 2\n  s = 5 \\ z\nNext i", &["i", "s", "z"], &[0, 0, 0]);
    assert_eq!(jit, None, "a zero divisor is refused");
    let (jit, _) = both("For i = 1 To 2 Step z\n  s = s + 1\nNext i", &["i", "s", "z"], &[0, 0, 0]);
    assert_eq!(jit, None, "a zero step is refused");
}

#[test]
fn a_loop_outside_the_subset_is_not_compiled() {
    let Stmt::For { var, start, end, step, body } = the_loop("For i = 1 To 3\n  Print i\nNext i")
    else {
        unreachable!()
    };
    let names = vec!["i".to_string()];
    assert!(compile_for(&var, &start, &end, step.as_ref(), &body, &names).is_none());
}
