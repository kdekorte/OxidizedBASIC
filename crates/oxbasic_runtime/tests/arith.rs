//! The numeric fast paths must give the answers the general rules give:
//! Long arithmetic that overflows becomes a Double, a whole-number Double
//! result becomes a Long, `Mod` and division by zero still raise errors, and
//! names match whatever their case.

use oxbasic_runtime::{Interpreter, Output};

struct Buf(String);
impl Output for Buf {
    fn write(&mut self, s: &str) {
        self.0.push_str(s);
    }
}

fn run(src: &str) -> String {
    let prog = oxbasic_parser::parse(src).unwrap();
    let mut out = Buf(String::new());
    Interpreter::new(&prog, &mut out).run().unwrap();
    out.0
}

#[test]
fn long_arithmetic_overflows_into_a_double() {
    let out = run("Dim a As Long\na = 2000000000\nPrint a + a\nPrint a * 3\nPrint a - 5\n");
    assert_eq!(
        out.split_whitespace().collect::<Vec<_>>(),
        ["4000000000", "6000000000", "1999999995"]
    );
}

#[test]
fn double_results_that_are_whole_numbers_behave_as_before() {
    let out =
        run("Dim d As Double\nd = 1.5\nPrint d + d\nPrint d * 2\nPrint d / 2\nPrint d - 0.5\n");
    assert_eq!(
        out.split_whitespace().collect::<Vec<_>>(),
        ["3", "3", "0.75", "1"]
    );
}

#[test]
fn comparisons_and_logic_on_numbers() {
    let out = run("Dim a As Long, d As Double\na = 3: d = 3.0\n\
         Print a = d; a < 4; d >= 3.5; (a < 4) And (d <= 3); (a > 4) Or (d <> 3)\n");
    assert_eq!(out.trim(), "TrueTrueFalseTrueFalse");
}

#[test]
fn mod_and_division_keep_their_rules() {
    assert_eq!(
        run("Print 17 Mod 5; -17 Mod 5; 7 \\ 2; 7 / 2\n")
            .split_whitespace()
            .collect::<Vec<_>>(),
        ["2", "-2", "3", "3.5"]
    );
    let prog = oxbasic_parser::parse("Dim z As Long\nPrint 5 Mod z\n").unwrap();
    let mut out = Buf(String::new());
    let e = Interpreter::new(&prog, &mut out).run().unwrap_err();
    assert_eq!(e.number, 11, "division by zero");
    let prog = oxbasic_parser::parse("Dim z As Double\nPrint 5 / z\n").unwrap();
    let e = Interpreter::new(&prog, &mut out).run().unwrap_err();
    assert_eq!(e.number, 11);
}

#[test]
fn variable_names_ignore_case() {
    let out = run("Dim Total As Long\nTOTAL = 4\ntotal = Total + 1\nPrint tOtAl\n");
    assert_eq!(out.trim(), "5");
}

#[test]
fn assigning_converts_to_the_declared_type() {
    let out = run("Dim i As Integer, l As Long, s As Single, t As String\ni = 3.7\nl = 2.5\ns = 1\nt = 12\nPrint i; l; s; t\n");
    assert_eq!(
        out.split_whitespace().collect::<Vec<_>>(),
        ["4", "3", "1", "12"]
    );
}
