
use oxbasic_runtime::{run_source, Output};

struct Buf(String);
impl Output for Buf {
    fn write(&mut self, s: &str) {
        self.0.push_str(s);
    }
}

fn run(src: &str) -> String {
    let mut b = Buf(String::new());
    run_source(src, &mut b).unwrap_or_else(|e| b.0.push_str(&format!("ERR {e}")));
    b.0
}

/// Long loops, which the JIT compiles, must print what they print with the
/// loop made too short to compile and the total scaled up by running it in
/// pieces: each pair of programs reaches the same totals.
#[test]
fn long_compiled_loops_give_the_same_totals_as_short_interpreted_ones() {
    // 5000 trips: compiled. The same sum built from 50 loops of 100 trips (each
    // below the threshold, so interpreted).
    let compiled = run("For i = 1 To 5000\n  s = s + i Mod 7 - i \\ 3\nNext i\nPrint s\nPrint i\n");
    let pieces = run(
        "For k = 0 To 49\n  For i = k * 100 + 1 To k * 100 + 100\n    s = s + i Mod 7 - i \\ 3\n  Next i\nNext k\nPrint s\nPrint i\n",
    );
    assert_eq!(compiled, pieces);
    assert!(!compiled.contains("ERR"), "{compiled}");
}

#[test]
fn a_compiled_loop_leaves_the_counter_and_other_variables_as_the_interpreter_does() {
    let out = run("a = 3\nFor i = 10 To 5000 Step 7\n  a = a + 1\nNext i\nPrint a\nPrint i\n");
    assert_eq!(out, " 716 \n 4994 \n");
}

#[test]
fn an_overflowing_long_loop_still_gives_the_interpreters_answer() {
    // the interpreter promotes past Long; compiled code must hand the loop back
    let out = run("s = 2000000000\nFor i = 1 To 3000\n  s = s + 1000000\nNext i\nPrint s\n");
    assert_eq!(out, " 5000000000 \n");
}

#[test]
fn a_loop_that_prints_or_calls_runs_in_the_interpreter() {
    let out = run("For i = 1 To 2000\n  If i = 2000 Then Print i\nNext i\n");
    assert_eq!(out, " 2000 \n");
}

#[test]
fn a_zero_step_is_still_the_interpreters_error() {
    let out = run("z = 0\nFor i = 1 To 5000 Step z\nNext i\n");
    assert!(out.starts_with("ERR"), "{out}");
}
