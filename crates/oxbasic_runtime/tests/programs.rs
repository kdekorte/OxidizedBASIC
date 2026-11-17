//! End-to-end tests: parse + run BASIC programs and check the output.

use oxbasic_runtime::{Output, run_source};

struct Buf(String);

impl Output for Buf {
    fn write(&mut self, s: &str) {
        self.0.push_str(s);
    }
}

fn run(src: &str) -> String {
    let mut buf = Buf(String::new());
    run_source(src, &mut buf).expect("run failed");
    buf.0
}

#[test]
fn hello_world() {
    assert_eq!(run("Print \"Hello, world!\"\n"), "Hello, world!\n");
}

#[test]
fn arithmetic_and_precedence() {
    assert_eq!(run("Print 2 + 3 * 4 ^ 2\n"), " 50 \n");
    // comma pads the print position to a multiple of 14
    assert_eq!(run("Print 10 \\ 3, 10 Mod 3\n"), format!(" 3 {} 1 \n", " ".repeat(11)));
    assert_eq!(run("Print 7 / 2\n"), " 3.5 \n");
}

#[test]
fn string_concat_and_functions() {
    assert_eq!(run("Print \"a\" & \"b\" & UCase(\"c\")\n"), "abC\n");
    assert_eq!(
        run("Print Left(\"hello\", 2); Mid(\"hello\", 2, 3); Right(\"hello\", 2)\n"),
        "heelllo\n"
    );
    assert_eq!(run("Print InStr(\"hello\", \"ll\")\n"), " 3 \n");
    assert_eq!(run("Print Replace(\"a-b-c\", \"-\", \"+\")\n"), "a+b+c\n");
    assert_eq!(run("Print Len(\"abc\")\n"), " 3 \n");
    assert_eq!(run("Print Trim(\"  x  \") & \"!\"\n"), "x!\n");
}

#[test]
fn variables_and_types() {
    let out = run("Dim x As Long\nx = 42\nDim s As String\ns = \"hi\"\nPrint x; s\n");
    assert_eq!(out, " 42 hi\n");
}

#[test]
fn block_if_elseif_else() {
    let out = run(
        "x = 5\nIf x > 10 Then\n  Print \"big\"\nElseIf x > 3 Then\n  Print \"medium\"\nElse\n  Print \"small\"\nEnd If\n",
    );
    assert_eq!(out, "medium\n");
}

#[test]
fn single_line_if() {
    assert_eq!(
        run("x = 1\nIf x = 1 Then Print \"one\" Else Print \"other\"\n"),
        "one\n"
    );
}

#[test]
fn select_case() {
    let out = run(
        "n = 4\nSelect Case n\n  Case 1 To 3\n    Print \"low\"\n  Case Is > 10\n    Print \"high\"\n  Case 4, 5\n    Print \"mid\"\n  Case Else\n    Print \"other\"\nEnd Select\n",
    );
    assert_eq!(out, "mid\n");
}

#[test]
fn for_next_loops() {
    let out = run("For i = 1 To 5\n  Print i;\nNext\n");
    assert_eq!(out, " 1  2  3  4  5 ");
    let out = run("For i = 10 To 1 Step -3\n  Print i;\nNext\nPrint \"!\"\n");
    assert_eq!(out, " 10  7  4  1 !\n");
}

#[test]
fn do_and_while_loops() {
    let out = run("x = 0\nDo While x < 3\n  x = x + 1\nLoop\nPrint x\n");
    assert_eq!(out, " 3 \n");
    let out = run("x = 0\nDo\n  x = x + 1\nLoop Until x >= 2\nPrint x\n");
    assert_eq!(out, " 2 \n");
    let out = run("x = 0\nWhile x < 4\n  x = x + 2\nWend\nPrint x\n");
    assert_eq!(out, " 4 \n");
}

#[test]
fn exit_do_and_exit_for() {
    let out = run("x = 0\nDo While True\n  x = x + 1\n  If x = 3 Then Exit Do\nLoop\nPrint x\n");
    assert_eq!(out, " 3 \n");
    let out = run("For i = 1 To 10\n  If i = 4 Then Exit For\nNext\nPrint i\n");
    assert_eq!(out, " 4 \n");
}

#[test]
fn subs_functions_byref() {
    let out = run(
        "Function Add(a As Long, b As Long) As Long\n  Add = a + b\nEnd Function\n\
         Sub Bump(ByRef n As Long)\n  n = n + 10\nEnd Sub\n\
         Dim total As Long\ntotal = 5\nBump total\nPrint Add(total, 2)\n",
    );
    assert_eq!(out, " 17 \n");
}

#[test]
fn recursion() {
    let out = run(
        "Function Fact(n As Long) As Long\n  If n <= 1 Then\n    Fact = 1\n  Else\n    Fact = n * Fact(n - 1)\n  End If\nEnd Function\nPrint Fact(6)\n",
    );
    assert_eq!(out, " 720 \n");
}

#[test]
fn arrays_and_foreach() {
    let out = run(
        "Dim a(5) As Long\nFor i = 0 To 5\n  a(i) = i * i\nNext\nPrint a(4)\nPrint UBound(a)\nFor Each v In a\n  Print v;\nNext\n",
    );
    assert_eq!(out, " 16 \n 5 \n 0  1  4  9  16  25 ");
}

#[test]
fn array_function_and_split_join() {
    let out = run(
        "arr = Array(10, 20, 30)\nPrint arr(1)\nparts = Split(\"a,b,c\", \",\")\nPrint Join(parts, \"-\")\n",
    );
    assert_eq!(out, " 20 \na-b-c\n");
}

#[test]
fn on_error_resume_next() {
    let out = run("On Error Resume Next\nx = 1 / 0\nPrint Err.Number\nPrint \"after\"\n");
    assert_eq!(out, " 11 \nafter\n");
}

#[test]
fn on_error_goto_handler() {
    // Resume Next continues at the statement AFTER the failing one,
    // so "skipped" (which follows x = 1 / 0) still prints.
    let out = run(
        "On Error GoTo Handler\nx = 1 / 0\nPrint \"skipped\"\nEnd\nHandler:\n  Print \"caught\"; Err.Number\n  Resume Next\n",
    );
    assert_eq!(out, "caught 11 \nskipped\n");
}

#[test]
fn goto_and_labels() {
    let out = run("GoTo Skip\nPrint \"no\"\nSkip:\nPrint \"yes\"\n");
    assert_eq!(out, "yes\n");
}

#[test]
fn colon_separated_statements() {
    assert_eq!(run("a = 1: b = 2: Print a + b\n"), " 3 \n");
}

#[test]
fn comparisons_and_logic() {
    let out = run("Print 3 > 2, 2 = 3, Not True, True And False, True Or False\n");
    // "True"/"False" padded to the next multiple of 14 per comma
    assert_eq!(
        out,
        format!(
            "True{}False{}False{}False{}True\n",
            " ".repeat(10),
            " ".repeat(9),
            " ".repeat(9),
            " ".repeat(9)
        )
    );
}

#[test]
fn hex_literals_and_comments() {
    assert_eq!(run("' comment\nx = &HFF\nPrint x\n"), " 255 \n");
}

#[test]
fn const_and_option_explicit() {
    let out = run("Const Pi = 3\nPrint Pi * 2\n");
    assert_eq!(out, " 6 \n");
    let mut buf = Buf(String::new());
    let r = run_source("Option Explicit\nx = 1\n", &mut buf);
    assert!(r.is_err());
}

#[test]
fn date_functions() {
    let out = run(
        "d = DateSerial(2024, 5, 1)\nPrint Year(d); Month(d); Day(d)\nPrint Format(d, \"yyyy-mm-dd\")\n",
    );
    assert_eq!(out, " 2024  5  1 \n2024-05-01\n");
    let out = run(
        "d = DateSerial(2000, 1, 1)\nd2 = DateAdd(\"d\", 31, d)\nPrint Format(d2, \"yyyy-mm-dd\")\n",
    );
    assert_eq!(out, "2000-02-01\n");
}

#[test]
fn msgbox_prints() {
    assert_eq!(run("MsgBox \"hello box\"\n"), "hello box\n");
}

#[test]
fn unhandled_error_propagates() {
    let mut buf = Buf(String::new());
    let r = run_source("x = 1 / 0\n", &mut buf);
    assert!(r.is_err());
    assert!(r.unwrap_err().contains("division by zero"));
}

#[test]
fn exit_sub() {
    let out = run("Sub Test()\n  Print \"a\"\n  Exit Sub\n  Print \"b\"\nEnd Sub\nTest\n");
    assert_eq!(out, "a\n");
}

#[test]
fn print_separators_and_trailing() {
    let out = run("Print \"a\", \"b\"\n");
    assert!(out.starts_with("a"));
    assert!(out.ends_with("b\n"));
    assert_eq!(run("Print \"a\";\nPrint \"b\"\n"), "ab\n");
}

