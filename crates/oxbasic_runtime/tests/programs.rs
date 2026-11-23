//! End-to-end tests: parse + run BASIC programs and check the output.

use oxbasic_runtime::{run_source, Output};

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
    assert_eq!(
        run("Print 10 \\ 3, 10 Mod 3\n"),
        format!(" 3 {} 1 \n", " ".repeat(11))
    );
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

#[test]
fn debugger_steps_and_watches() {
    use oxbasic_runtime::{DebugAction, Interpreter, Output, PausedInfo};

    struct Sink(String);
    impl Output for Sink {
        fn write(&mut self, s: &str) {
            self.0.push_str(s);
        }
    }

    let src =
        "Dim Total As Long\nTotal = 0\nFor i = 1 To 3\n  Total = Total + i\nNext\nPrint Total\n";
    let prog = oxbasic_parser::parse(src).unwrap();
    let mut sink = Sink(String::new());
    let mut rt = Interpreter::new(&prog, &mut sink);

    let pauses: std::rc::Rc<std::cell::RefCell<Vec<PausedInfo>>> =
        std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let steps = std::rc::Rc::new(std::cell::Cell::new(0usize));
    {
        let pauses = pauses.clone();
        let steps = steps.clone();
        rt.set_debug_hook(Box::new(move |info: &PausedInfo| {
            pauses.borrow_mut().push(info.clone());
            steps.set(steps.get() + 1);
            if steps.get() >= 6 {
                DebugAction::Continue
            } else {
                DebugAction::StepOver
            }
        }));
    }
    rt.run().unwrap();

    let p = pauses.borrow();
    assert!(p.len() >= 6);
    // "break on start": first pause is module-level, before `Dim` on line 1
    assert_eq!(p[0].line, 1);
    assert_eq!(p[0].proc, "");
    // after stepping, watches show the running total
    let total = p.iter().rev().find_map(|info| {
        info.globals
            .iter()
            .find(|(k, _)| k == "total")
            .map(|(_, v)| v.clone())
    });
    assert!(total.is_some());
    assert_eq!(sink.0, " 6 \n");
}

#[test]
fn debugger_breakpoints() {
    use oxbasic_runtime::{DebugAction, Interpreter, Output};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    struct Sink(String);
    impl Output for Sink {
        fn write(&mut self, s: &str) {
            self.0.push_str(s);
        }
    }

    let src = "x = 1\ny = 2\nz = x + y\nPrint z\n";
    let prog = oxbasic_parser::parse(src).unwrap();
    let mut sink = Sink(String::new());
    let mut rt = Interpreter::new(&prog, &mut sink);

    let bps: Arc<Mutex<HashSet<usize>>> = Arc::new(Mutex::new(HashSet::from([3])));
    rt.set_breakpoints(bps);
    let lines = std::rc::Rc::new(std::cell::RefCell::new(Vec::<usize>::new()));
    {
        let lines = lines.clone();
        rt.set_debug_hook(Box::new(move |info: &oxbasic_runtime::PausedInfo| {
            lines.borrow_mut().push(info.line);
            DebugAction::Continue
        }));
    }
    rt.run().unwrap();
    // break-on-start (line 1), then the breakpoint on line 3
    assert_eq!(*lines.borrow(), vec![1, 3]);
    assert_eq!(sink.0, " 3 \n");
}

#[test]
fn debugger_abort() {
    use oxbasic_runtime::{DebugAction, Interpreter, Output};

    struct Sink(String);
    impl Output for Sink {
        fn write(&mut self, s: &str) {
            self.0.push_str(s);
        }
    }

    let src = "For i = 1 To 1000000\n  x = i\nNext\n";
    let prog = oxbasic_parser::parse(src).unwrap();
    let mut sink = Sink(String::new());
    let mut rt = Interpreter::new(&prog, &mut sink);
    rt.set_debug_hook(Box::new(|_| DebugAction::Abort));
    let err = rt.run().unwrap_err();
    assert_eq!(err.number, oxbasic_runtime::DEBUG_ABORT);
}

#[test]
fn gosub_and_return() {
    // each GoSub comes back to the statement after it, and a subroutine can
    // call another one
    let src = "GoSub Greet\nPrint \"after 1\"\nGoSub Greet\nPrint \"after 2\"\nEnd\n\
               Greet:\n  Print \"hello\"\n  GoSub Inner\n  Return\n\
               Inner:\n  Print \"inner\"\n  Return\n";
    assert_eq!(run(src), "hello\ninner\nafter 1\nhello\ninner\nafter 2\n");
}

#[test]
fn gosub_inside_a_loop_returns_into_the_loop() {
    let src = "For i = 1 To 3\n  GoSub Show\nNext\nEnd\n\
               Show:\n  Print i;\n  Return\n";
    assert_eq!(run(src), " 1  2  3 ");
}

#[test]
fn return_without_gosub_is_error_3() {
    let mut buf = Buf(String::new());
    let e = run_source("Return\n", &mut buf).unwrap_err();
    assert!(e.contains("Return without GoSub"), "{}", e);
}

#[test]
fn stop_does_nothing_outside_the_ide() {
    assert_eq!(run("Print \"a\"\nStop\nPrint \"b\"\n"), "a\nb\n");
}

#[test]
fn let_is_an_optional_keyword_on_assignment() {
    assert_eq!(run("Let x = 5\nPrint x\n"), " 5 \n");
    assert_eq!(run("Let s = \"a\" & \"b\"\nPrint s\n"), "ab\n");
}

#[test]
fn erase_resets_a_fixed_array_and_empties_a_dynamic_one() {
    // a fixed array keeps its size; its numbers go back to zero
    assert_eq!(
        run("Dim a(2)\na(1) = 7\nErase a\nPrint a(1)\n"),
        " 0 \n"
    );
    // a dynamic array is emptied, so indexing it again is out of range
    let mut buf = Buf(String::new());
    let r = run_source("Dim d() As Integer\nReDim d(3)\nErase d\nPrint d(0)\n", &mut buf);
    assert!(r.unwrap_err().contains("subscript out of range"));
}

#[test]
fn redim_resizes_a_dynamic_array() {
    assert_eq!(run("Dim a() As Integer\nReDim a(3)\nPrint UBound(a)\n"), " 3 \n");
}

#[test]
fn redim_preserve_keeps_the_existing_elements() {
    let src = "Dim a() As Integer\nReDim a(1)\na(1) = 7\nReDim Preserve a(3)\nPrint a(1); UBound(a)\n";
    assert_eq!(run(src), " 7  3 \n");
    // without Preserve the elements are cleared: a new Variant element is Empty
    let src = "Dim a() As Integer\nReDim a(1)\na(1) = 7\nReDim a(3)\nPrint a(1)\n";
    assert_eq!(run(src), "\n");
}

#[test]
fn mid_statement_replaces_characters_in_place() {
    assert_eq!(run("Dim s As String\ns = \"hello\"\nMid(s, 1, 1) = \"J\"\nPrint s\n"), "Jello\n");
    // without a length, the text replaces as many characters as it has
    assert_eq!(run("s = \"abcdef\"\nMid(s, 2) = \"XY\"\nPrint s\n"), "aXYdef\n");
    // a length shorter than the text takes only that many characters
    assert_eq!(run("s = \"abcdef\"\nMid(s, 2, 2) = \"XYZ\"\nPrint s\n"), "aXYdef\n");
    // the text is never extended past the end of the string
    assert_eq!(run("s = \"abc\"\nMid(s, 3) = \"XYZ\"\nPrint s\n"), "abX\n");
}

#[test]
fn option_compare_text_ignores_case_in_comparisons() {
    assert_eq!(run("Print \"a\" = \"A\"\n"), "False\n");
    assert_eq!(run("Option Compare Text\nPrint \"a\" = \"A\"\n"), "True\n");
}

#[test]
fn option_base_1_starts_arrays_at_one() {
    // `Dim a(3)` holds three elements, 1 to 3
    assert_eq!(
        run("Option Base 1\nDim a(3)\na(1) = 10\na(3) = 30\nPrint a(1) + a(3)\n"),
        " 40 \n"
    );
    // index 0 is out of range
    let mut buf = Buf(String::new());
    let e = run_source("Option Base 1\nDim a(3)\na(0) = 1\n", &mut buf).unwrap_err();
    assert!(e.contains("subscript out of range"), "{}", e);
    // LBound and UBound report the real bounds
    assert_eq!(
        run("Option Base 1\nDim a(3)\nPrint LBound(a); UBound(a)\n"),
        " 1  3 \n"
    );
}

#[test]
fn option_base_0_is_the_default() {
    assert_eq!(
        run("Dim a(3)\nPrint LBound(a); UBound(a)\n"),
        " 0  3 \n"
    );
    assert_eq!(run("Option Base 0\nDim a(2)\nPrint UBound(a)\n"), " 2 \n");
}

#[test]
fn option_base_1_applies_to_array_builtins_and_redim() {
    // Array() and Split() start at the base too
    assert_eq!(
        run("Option Base 1\nv = Array(7, 8, 9)\nPrint v(1); v(3); LBound(v); UBound(v)\n"),
        " 7  9  1  3 \n"
    );
    // ReDim keeps the elements that are still in range
    assert_eq!(
        run("Option Base 1\nDim d()\nReDim d(2)\nd(1) = 5\nReDim Preserve d(3)\nPrint d(1); LBound(d); UBound(d)\n"),
        " 5  1  3 \n"
    );
}

#[test]
fn option_base_1_rejects_a_bound_below_the_base() {
    let mut buf = Buf(String::new());
    assert!(run_source("Option Base 1\nDim a(0)\n", &mut buf).is_err());
}

#[test]
fn choose_switch_round_and_timer() {
    assert_eq!(run("Print Choose(2, \"a\", \"b\")\n"), "b\n");
    assert_eq!(run("Print Choose(9, \"a\", \"b\")\n"), "Null\n");
    assert_eq!(run("Print Switch(1 > 2, \"x\", 2 > 1, \"y\")\n"), "y\n");
    // VB rounds halves to the even neighbour
    assert_eq!(run("Print Round(2.5); Round(3.5); Round(2.25, 1)\n"), " 2  4  2.2 \n");
    assert_eq!(run("Print Timer >= 0\n"), "True\n");
}


#[test]
fn with_block_reads_members_of_its_object() {
    // a With on a user Type (members come with the Type work); here a
    // value's members are read through the built-in Err object
    assert_eq!(
        run("With Err\nPrint .Number\nEnd With\n"),
        " 0 \n"
    );
}

#[test]
fn with_block_outside_a_with_is_an_error() {
    let mut buf = Buf(String::new());
    let e = run_source("Print .Number\n", &mut buf).unwrap_err();
    assert!(e.contains("With block"), "{}", e);
}

#[test]
fn type_declares_a_record_with_fields() {
    let src = "Type Point\n    X As Long\n    Y As Long\nEnd Type\nDim p As Point\np.X = 3\np.Y = 4\nPrint p.X + p.Y\n";
    assert_eq!(run(src), " 7 \n");
}

#[test]
fn type_fields_start_at_their_defaults() {
    let src = "Type Person\n    Name As String\n    Age As Long\nEnd Type\nDim p As Person\nPrint p.Age; \"[\"; p.Name; \"]\"\n";
    assert_eq!(run(src), " 0 []\n");
}

#[test]
fn assigning_a_record_copies_it() {
    let src = "Type Point\n    X As Long\nEnd Type\nDim a As Point\nDim b As Point\na.X = 1\nb = a\nb.X = 9\nPrint a.X; b.X\n";
    assert_eq!(run(src), " 1  9 \n");
}

#[test]
fn array_of_records_has_one_record_per_element() {
    let src = "Type Point\n    X As Long\nEnd Type\nDim pts(2) As Point\npts(1).X = 5\nPrint pts(1).X; pts(0).X\n";
    assert_eq!(run(src), " 5  0 \n");
}


#[test]
fn format_numeric_patterns_match_vb() {
    assert_eq!(run("Print Format(1234.5, \"#,##0.00\")\n"), "1,234.50\n");
    assert_eq!(run("Print Format(3, \"0000\")\n"), "0003\n");
    assert_eq!(run("Print Format(0.256, \"0.0%\")\n"), "25.6%\n");
    assert_eq!(run("Print Format(12345.6, \"0.00E+00\")\n"), "1.23E+04\n");
}

#[test]
fn format_named_formats_match_vb() {
    assert_eq!(run("Print Format(0.5, \"Percent\")\n"), "50.00%\n");
    assert_eq!(run("Print Format(1234.5, \"Fixed\")\n"), "1234.50\n");
    assert_eq!(run("Print Format(1234.5, \"Standard\")\n"), "1,234.50\n");
    assert_eq!(run("Print Format(True, \"Yes/No\")\n"), "Yes\n");
    assert_eq!(run("Print Format(0, \"On/Off\")\n"), "Off\n");
}

#[test]
fn format_date_patterns_match_vb() {
    // 2024-05-01 was a Wednesday
    let d = "DateSerial(2024, 5, 1)";
    assert_eq!(run(&format!("Print Format({}, \"yyyy-mm-dd\")\n", d)), "2024-05-01\n");
    assert_eq!(
        run(&format!("Print Format({}, \"dddd, mmmm d, yyyy\")\n", d)),
        "Wednesday, May 1, 2024\n"
    );
    assert_eq!(run(&format!("Print Format({}, \"ddd mmm\")\n", d)), "Wed May\n");
    assert_eq!(run(&format!("Print Format({}, \"q\")\n", d)), "2\n");
}

#[test]
fn with_block_works_on_a_record() {
    let src = "Type Point\n    X As Long\n    Y As Long\nEnd Type\nDim p As Point\nWith p\n    .X = 2\n    .Y = .X * 10\n    Print .Y\nEnd With\nPrint p.Y\n";
    assert_eq!(run(src), " 20 \n 20 \n");
}

#[test]
fn enum_members_are_constants_with_implicit_values() {
    let src = "Enum Color\n    Red\n    Green\n    Blue = 10\n    Alpha\nEnd Enum\nPrint Red; Green; Blue; Alpha\n";
    assert_eq!(run(src), " 0  1  10  11 \n");
}

#[test]
fn enum_members_are_visible_inside_procedures() {
    let src = "Enum Level\n    Low = 1\n    High = 3\nEnd Enum\nSub Show()\n    Print High - Low\nEnd Sub\nShow\n";
    assert_eq!(run(src), " 2 \n");
}



#[test]
fn def_types_give_undeclared_names_a_type_by_first_letter() {
    // i is an Integer: 2.7 is stored as 3
    assert_eq!(run("DefInt I-N\ni = 2.7\nPrint i\n"), " 3 \n");
    // an explicit As type wins over the DefType
    assert_eq!(run("DefInt I-N\nDim idx As Double\nidx = 2.5\nPrint idx\n"), " 2.5 \n");
    // a name outside the range stays a Variant
    assert_eq!(run("DefInt I-N\nx = 2.5\nPrint x\n"), " 2.5 \n");
}

#[test]
fn type_array_field_holds_a_fixed_array() {
    let src = "Type Box\n    Vals(2) As Long\nEnd Type\nDim b As Box\nb.Vals(1) = 7\nPrint b.Vals(1); b.Vals(0)\n";
    assert_eq!(run(src), " 7  0 \n");
}

#[test]
fn type_field_of_another_user_type() {
    let src = "Type Inner\n    N As Long\nEnd Type\nType Outer\n    I As Inner\nEnd Type\nDim o As Outer\no.I.N = 4\nPrint o.I.N\n";
    assert_eq!(run(src), " 4 \n");
}

#[test]
fn a_runtime_error_reports_the_line_it_happened_on() {
    // line 3 divides by zero
    let mut buf = Buf(String::new());
    let e = run_source("x = 1\ny = 2\nz = 1 / 0\nPrint z\n", &mut buf).unwrap_err();
    assert!(e.contains("on line 3"), "{}", e);
}

#[test]
fn an_error_inside_a_sub_reports_the_line_inside_the_sub() {
    // the error is on line 4, inside the Sub, not on the line that called it
    let src = "Sub Boom()\n  x = 1\n  Print x\n  y = 1 / 0\nEnd Sub\nBoom\n";
    let mut buf = Buf(String::new());
    let e = run_source(src, &mut buf).unwrap_err();
    assert!(e.contains("on line 4"), "{}", e);
}

#[test]
fn an_error_handled_by_on_error_keeps_its_number() {
    // a trapped error prints nothing; Err.Number is unchanged
    assert_eq!(
        run("On Error Resume Next\nx = 1 / 0\nPrint Err.Number\n"),
        " 11 \n"
    );
}


