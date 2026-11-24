//! Checks the claims in docs/keywords.md by running the programs they describe:
//! each documented built-in gives the value the doc states, and each "not
//! supported" item fails the way the doc says. If the runtime changes, these
//! fail, so the doc is updated with it.

use oxbasic_runtime::{run_source, Output};

struct Buf(String);

impl Output for Buf {
    fn write(&mut self, s: &str) {
        self.0.push_str(s);
    }
}

/// The output of `src`, or its error text.
fn run(src: &str) -> Result<String, String> {
    let mut buf = Buf(String::new());
    run_source(src, &mut buf).map_err(|e| e.to_string())?;
    Ok(buf.0)
}

/// The output of `src`, which must run.
fn out(src: &str) -> String {
    run(&format!("{}\n", src)).unwrap_or_else(|e| panic!("{} failed: {}", src, e))
}

#[test]
fn math_functions_give_the_documented_values() {
    assert_eq!(out("Print Abs(-3)"), " 3 \n");
    assert_eq!(out("Print Sgn(-4)"), " -1 \n");
    // Int floors; Fix truncates toward zero
    assert_eq!(out("Print Int(-2.5)"), " -3 \n");
    assert_eq!(out("Print Fix(-2.5)"), " -2 \n");
    // Round is banker's rounding: half goes to the even neighbour
    assert_eq!(out("Print Round(2.5)"), " 2 \n");
    assert_eq!(out("Print Round(3.5)"), " 4 \n");
    assert_eq!(out("Print Sqr(16)"), " 4 \n");
    assert_eq!(out("Print Exp(0)"), " 1 \n");
    assert_eq!(out("Print Log(1)"), " 0 \n");
    assert_eq!(out("Print Rnd() < 1"), "True\n");
    assert_eq!(out("Randomize 42\nPrint Rnd() < 1"), "True\n");
}

#[test]
fn string_functions_give_the_documented_values() {
    assert_eq!(out("Print Len(\"hello\")"), " 5 \n");
    assert_eq!(out("Print Left(\"hello\", 2)"), "he\n");
    assert_eq!(out("Print Mid(\"hello\", 2, 3)"), "ell\n");
    assert_eq!(out("Print InStr(\"hello\", \"l\")"), " 3 \n");
    assert_eq!(out("Print InStrRev(\"abcabc\", \"b\")"), " 5 \n");
    assert_eq!(out("Print Replace(\"aXbX\", \"X\", \"-\")"), "a-b-\n");
    assert_eq!(out("Print Space(3) & \"|\""), "   |\n");
    assert_eq!(out("Print String(3, \"x\")"), "xxx\n");
    assert_eq!(out("Print StrReverse(\"abc\")"), "cba\n");
    assert_eq!(out("Print LTrim(\"  a \") & \"|\""), "a |\n");
    assert_eq!(out("Print Asc(\"A\")"), " 65 \n");
    assert_eq!(out("Print Chr(66)"), "B\n");
    assert_eq!(out("Print Val(\"12abc\")"), " 12 \n");
    assert_eq!(out("Print Str(5)"), " 5\n");
    assert_eq!(out("Print StrComp(\"a\", \"b\")"), " -1 \n");
    assert_eq!(out("Print Hex(255)"), "FF\n");
    assert_eq!(out("Print Oct(8)"), "10\n");
    assert_eq!(out("Print Join(Array(\"a\", \"b\"), \"-\")"), "a-b\n");
    assert_eq!(out("Dim parts\nparts = Split(\"a,b,c\", \",\")\nPrint parts(1)"), "b\n");
}
#[test]
fn reference_claims_for_the_new_function_details() {
    // Split is zero-based, and Join defaults to a space
    assert_eq!(out("Dim p\np = Split(\"a,b,c\", \",\")\nPrint p(0)"), "a\n");
    assert_eq!(out("Dim p\np = Split(\"a b\")\nPrint Join(p)"), "a b\n");
    // Randomize with a seed gives the same sequence each time
    let twice = "Randomize 7\nPrint Rnd()\nRandomize 7\nPrint Rnd()";
    let got = out(twice);
    let lines: Vec<&str> = got.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], lines[1], "the same seed repeats the value");
    // Named numeric formats
    assert_eq!(out("Print Format(3, \"Fixed\")"), "3.00\n");
    assert_eq!(out("Print Format(1234.5, \"Standard\")"), "1,234.50\n");
    assert_eq!(out("Print Format(0, \"Yes/No\")"), "No\n");
    assert_eq!(out("Print Format(0, \"True/False\")"), "False\n");
    // DateSerial carries an out-of-range month into the next year
    assert_eq!(out("Print Format(DateSerial(2024, 13, 1), \"yyyy-mm-dd\")"), "2025-01-01\n");
    // Weekday: 1 is Sunday. 2024-03-03 was a Sunday.
    assert_eq!(out("Print Weekday(DateSerial(2024, 3, 3))"), " 1 \n");
    // CBool: zero is False, anything else True
    assert_eq!(out("Print CBool(0)"), "False\n");
    // CStr has no leading space, unlike Str
    assert_eq!(out("Print \"[\" & CStr(5) & \"]\""), "[5]\n");
    // IsDate is False for a string that looks like a date
    assert_eq!(out("Print IsDate(\"2024-03-05\")"), "False\n");
    // Format with a date picture
    assert_eq!(out("Print Format(DateSerial(2024, 3, 5), \"mmmm d, yyyy\")"), "March 5, 2024\n");
}


#[test]
fn documented_edge_cases_behave_as_the_reference_says() {
    // Choose gives Null for an index outside the list
    assert_eq!(out("Print IsNull(Choose(5, \"a\", \"b\"))"), "True\n");
    // Switch gives Null when no expression is true
    assert_eq!(out("Print IsNull(Switch(0, \"a\"))"), "True\n");
    // Round takes an optional digits argument
    assert_eq!(out("Print Round(3.14159, 2)"), " 3.14 \n");
    // Str puts a space before a non-negative number, not a negative one
    assert_eq!(out("Print \"[\" & Str(5) & \"]\""), "[ 5]\n");
    assert_eq!(out("Print \"[\" & Str(-5) & \"]\""), "[-5]\n");
    // Val reads only the leading number
    assert_eq!(out("Print Val(\"  3.5 apples\")"), " 3.5 \n");
    // Len counts characters
    assert_eq!(out("Print Len(\"héllo\")"), " 5 \n");
    // DateAdd in months keeps the day, and DateSerial normalises the month
    assert_eq!(out("Print Format(DateAdd(\"m\", 1, DateSerial(2024, 1, 15)), \"yyyy-mm-dd\")"), "2024-02-15\n");
    // Mid with a start below 1 is error 5
    assert!(run("Print Mid(\"abc\", 0, 1)\n").unwrap_err().contains("5"));
    // Asc of an empty string is error 5
    assert!(run("Print Asc(\"\")\n").unwrap_err().contains("5"));
    // Sqr and Log of a bad argument are error 5
    assert!(run("Print Sqr(-1)\n").unwrap_err().contains("5"));
    assert!(run("Print Log(0)\n").unwrap_err().contains("5"));
    // RGB takes parts 0 to 255
    assert!(run("Print RGB(256, 0, 0)\n").unwrap_err().contains("5"));
    // MsgBox writes its prompt to the output
    assert_eq!(out("MsgBox \"hi\""), "hi\n");
    // Beep does nothing visible
    assert_eq!(out("Beep\nPrint \"x\""), "x\n");
}



#[test]
fn choice_and_conversion_functions_give_the_documented_values() {
    assert_eq!(out("Print IIf(1 > 0, \"y\", \"n\")"), "y\n");
    assert_eq!(out("Print Choose(2, \"a\", \"b\", \"c\")"), "b\n");
    assert_eq!(out("Print Switch(0, \"a\", 1, \"b\")"), "b\n");
    assert_eq!(out("Print CInt(2.5)"), " 3 \n");
    assert_eq!(out("Print CLng(3.5)"), " 4 \n");
    assert_eq!(out("Print CBool(5)"), "True\n");
    assert_eq!(out("Print CStr(1.5)"), "1.5\n");
    assert_eq!(out("Print Cdbl(\"1.5\") + 1"), " 2.5 \n");
}

#[test]
fn type_tests_give_the_documented_values() {
    assert_eq!(out("Print VarType(1)"), " 3 \n");
    assert_eq!(out("Print TypeName(\"x\")"), "String\n");
    assert_eq!(out("Print IsNumeric(\"12\")"), "True\n");
    assert_eq!(out("Print IsDate(\"x\")"), "False\n");
    assert_eq!(out("Print IsEmpty(x)"), "True\n");
    assert_eq!(out("Print IsNull(Null)"), "True\n");
    assert_eq!(out("Print IsArray(Array(1))"), "True\n");
}

#[test]
fn array_functions_give_the_documented_bounds() {
    assert_eq!(out("Print LBound(Array(5, 6))"), " 0 \n");
    assert_eq!(out("Print UBound(Array(5, 6))"), " 1 \n");
}

#[test]
fn date_functions_give_the_documented_values() {
    assert_eq!(out("Print Format(DateSerial(2024, 3, 5), \"yyyy-mm-dd\")"), "2024-03-05\n");
    assert_eq!(out("Print Weekday(DateSerial(2024, 3, 5))"), " 3 \n");
    assert_eq!(out("Print DateAdd(\"d\", 1, DateSerial(2024, 1, 31))"), " 2024-02-01 \n");
    assert_eq!(out("Print DateValue(\"2024-03-05\")"), " 2024-03-05 \n");
    assert_eq!(out("Print Hour(TimeSerial(13, 5, 6))"), " 13 \n");
    assert_eq!(out("Print TypeName(Now) <> \"\""), "True\n");
}

#[test]
fn format_gives_the_documented_named_and_numeric_formats() {
    assert_eq!(out("Print Format(1234.5, \"Currency\")"), "1,234.50\n");
    assert_eq!(out("Print Format(0.5, \"Percent\")"), "50.00%\n");
    assert_eq!(out("Print Format(12, \"Yes/No\")"), "Yes\n");
    assert_eq!(out("Print Format(-1, \"On/Off\")"), "On\n");
    assert_eq!(out("Print Format(3.14159, \"0.00\")"), "3.14\n");
    assert_eq!(out("Print Format(1234567, \"#,##0\")"), "1,234,567\n");
    assert_eq!(out("Print Format(1, \"0.0E+00\")"), "1.0E+00\n");
}

#[test]
fn file_functions_need_an_open_file() {
    // Lof and Loc name a file number: with none open, error 52
    assert!(run("Print Lof(1)\n").unwrap_err().contains("52"));
    assert!(run("Print Loc(1)\n").unwrap_err().contains("52"));
}

#[test]
fn not_supported_items_fail_as_the_doc_says() {
    // Property procedures are not in the grammar: a syntax error
    assert!(run("Property Get Name()\nEnd Property\n").is_err());
    // Lock is not a statement: a syntax error
    assert!(run("Dim x\nLock #1\n").is_err());
    // Option Base is 0 or 1 only
    assert!(run("Option Base 2\n").unwrap_err().contains("0 or 1"));
}

/// A temporary folder for one test's files, cleared first.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("oxbasic-claims-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn freefile_is_the_lowest_unused_file_number() {
    // with nothing open, the first number is free
    assert_eq!(out("Print FreeFile"), " 1 \n");
    assert_eq!(out("Print FreeFile()"), " 1 \n");
    let dir = scratch("freefile");
    let p = dir.join("a.txt");
    let src = format!(
        "Open \"{}\" For Output As #1\nPrint FreeFile\nOpen \"{}\" For Output As #2\nPrint FreeFile\nClose #1\nPrint FreeFile\n",
        p.display(),
        dir.join("b.txt").display()
    );
    // 2 is the next free number, then with #2 open 3, then #1 is freed: 1
    assert_eq!(out(&src), " 2 \n 3 \n 1 \n");
}

#[test]
fn lof_gives_the_length_of_a_sequential_input_file() {
    let dir = scratch("lof-input");
    let p = dir.join("in.txt");
    std::fs::write(&p, "hello\nworld\n").unwrap();
    let src = format!(
        "Open \"{}\" For Input As #1\nPrint Lof(1)\nClose #1\n",
        p.display()
    );
    // 12 bytes: "hello\n" (6) and "world\n" (6)
    assert_eq!(out(&src), " 12 \n");
}

#[test]
fn lof_counts_text_still_buffered_in_a_sequential_output_file() {
    let dir = scratch("lof-output");
    let p = dir.join("out.txt");
    // nothing is flushed to disk yet when Lof is asked: it must still count it
    let src = format!(
        "Open \"{}\" For Output As #1\nPrint #1, \"abc\"\nPrint Lof(1)\nClose #1\n",
        p.display()
    );
    // "abc" then the newline the write adds: 4 bytes
    assert_eq!(out(&src), " 4 \n");
}

#[test]
fn lock_and_unlock_work_on_an_open_file() {
    let dir = scratch("lock-ok");
    let p = dir.join("r.dat");
    let src = format!(
        "Open \"{}\" For Random As #1 Len = 8\nLock #1\nUnlock #1\nLock #1, 3\nUnlock #1, 3\nPrint \"done\"\nClose #1\n",
        p.display()
    );
    assert_eq!(out(&src), "done\n");
}

#[test]
fn lock_on_a_closed_file_is_error_52() {
    let err = run("Lock #1\n").unwrap_err();
    assert!(err.contains("52"), "{}", err);
}

#[test]
fn an_overlapping_lock_is_error_70() {
    let dir = scratch("lock-conflict");
    let p = dir.join("r.dat");
    let src = format!(
        "Open \"{}\" For Random As #1 Len = 8\nLock #1, 1 To 5\nLock #1, 4\n",
        p.display()
    );
    let err = run(&src).unwrap_err();
    assert!(err.contains("70"), "{}", err);
}

#[test]
fn unlocking_a_range_never_locked_is_error_70() {
    let dir = scratch("unlock-unknown");
    let p = dir.join("r.dat");
    let src = format!(
        "Open \"{}\" For Random As #1 Len = 8\nUnlock #1, 2\n",
        p.display()
    );
    let err = run(&src).unwrap_err();
    assert!(err.contains("70"), "{}", err);
}
