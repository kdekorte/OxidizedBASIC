//! The examples run with the JIT on and off must print the same. Each console
//! example in `examples/` is run both ways; the programs that draw forms need
//! a window and are covered by the IDE's own tests. `basica.bas` prints its own
//! elapsed time, which differs by design, so only its result line is compared.

use oxbasic_runtime::{Interpreter, Output};

struct Buf(String);
impl Output for Buf {
    fn write(&mut self, s: &str) {
        self.0.push_str(s);
    }
}

/// The output of `src` (or its error) with the JIT on or off, and how many
/// loops ran as compiled code.
fn run_counted(src: &str, jit: bool) -> (String, usize) {
    let prog = oxbasic_parser::parse(src).expect("parses");
    let mut buf = Buf(String::new());
    let runs;
    {
        let mut it = Interpreter::new(&prog, &mut buf);
        it.set_jit(jit);
        if let Err(e) = it.run() {
            return (format!("ERROR {e}"), it.jit_loops_run());
        }
        runs = it.jit_loops_run();
    }
    (buf.0, runs)
}

fn run(src: &str, jit: bool) -> String {
    run_counted(src, jit).0
}

fn example(name: &str) -> String {
    let path = format!("{}/../../examples/{}", env!("CARGO_MANIFEST_DIR"), name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn the_console_examples_print_the_same_with_the_jit_on_and_off() {
    for name in ["demo.bas", "fib.bas", "hello.bas"] {
        let src = example(name);
        let off = run(&src, false);
        let on = run(&src, true);
        assert!(!off.starts_with("ERROR"), "{name}: {off}");
        assert_eq!(off, on, "{name}");
    }
}

#[test]
fn basica_gives_the_same_count_with_the_jit_on_and_off() {
    let src = example("basica.bas");
    let result = |jit| {
        run(&src, jit)
            .lines()
            .find(|l| l.starts_with("Final count"))
            .map(str::to_string)
            .expect("a result line")
    };
    assert_eq!(result(false).trim(), "Final count:  -5000000");
    assert_eq!(result(true), result(false));
    assert_eq!(run_counted(&src, false).1, 0, "off: nothing compiled");
    // without the `jit` feature nothing is ever compiled
    let expected = if cfg!(feature = "jit") { 1 } else { 0 };
    assert_eq!(run_counted(&src, true).1, expected, "on: the loop was compiled");
}

/// Loops long enough to be compiled, in the shapes programs use: a sum, a
/// nested pair, a negative step, `Mod` and `\` of negatives, and a loop that
/// mixes in a call or output (which the JIT leaves to the interpreter).
#[test]
fn long_loops_print_the_same_with_the_jit_on_and_off() {
    let programs = [
        "s = 0\nFor i = 1 To 20000\n  s = s + i * 2 - i \\ 3\nNext i\nPrint s\nPrint i\n",
        "t = 0\nj = 0\nFor i = 1 To 1500\n  For j = 1 To 20\n    t = t + (i Mod 7) * (j Mod 5)\n  Next j\nNext i\nPrint t\nPrint i\nPrint j\n",
        "m = 0\nFor i = 5000 To -5000 Step -3\n  m = m + i Mod 7 + i \\ 4\nNext i\nPrint m\nPrint i\n",
        "a = 0\nb = 0\nFor i = -3000 To 3000\n  If i Mod 3 = 0 And i > 0 Then a = a + 1 Else If i < 0 Then b = b - 1\nNext i\nPrint a\nPrint b\n",
        "Function F(x)\n  F = x + 1\nEnd Function\nc = 0\nFor i = 1 To 3000\n  c = F(c)\nNext i\nPrint c\n",
        "Dim n As Long\nSub Work()\n  For k = 1 To 4000\n    n = n + k\n  Next k\n  Print n\n  Print k\nEnd Sub\nWork\nWork\n",
        "w = 0\nDim q As Long\nFor q = 1 To 5000\n  w = w + q\nNext q\nPrint w\nPrint q\n",
        "s = 2000000000\nFor i = 1 To 5000\n  s = s + 1000000\nNext i\nPrint s\n",
        "z = 0\nFor i = 1 To 5000 Step z\nNext i\nPrint i\n",
        "For i = 1 To 5000\n  d = 100 \\ (i - 2500)\nNext i\nPrint d\n",
    ];
    // which of the programs the JIT actually compiled a loop in (the rest are
    // the cases it must leave to the interpreter)
    let compiled_in: [bool; 10] = [
        true, true, true, true, false, true, true, false, false, false,
    ];
    for (src, expect_compiled) in programs.iter().zip(compiled_in) {
        let (off, off_runs) = run_counted(src, false);
        let (on, on_runs) = run_counted(src, true);
        assert_eq!(off, on, "{src}");
        assert_eq!(off_runs, 0, "{src}");
        assert_eq!(
            on_runs > 0,
            expect_compiled && cfg!(feature = "jit"),
            "compiled or not: {src}"
        );
    }
}
