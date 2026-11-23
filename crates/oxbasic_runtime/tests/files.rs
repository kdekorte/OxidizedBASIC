//! File I/O: a program writes a sequential file and reads it back. The files
//! are in a folder under the system's temporary directory, named for each
//! test, so the tests can run in parallel.

use oxbasic_runtime::{run_source, Output};

struct Buf(String);

impl Output for Buf {
    fn write(&mut self, s: &str) {
        self.0.push_str(s);
    }
}

fn run(src: &str) -> Result<String, String> {
    let mut buf = Buf(String::new());
    run_source(src, &mut buf)?;
    Ok(buf.0)
}

/// A fresh folder for one test's files, and the path of a file in it.
fn folder(test: &str) -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("oxbasic_files_{}", test));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp folder");
    let file = dir.join("data.txt");
    // a path literal in BASIC: backslashes are fine inside a string
    let literal = file.to_string_lossy().replace('"', "\"\"");
    (dir, literal)
}

#[test]
fn print_then_line_input_reads_each_line_back() {
    let (_dir, path) = folder("line_input");
    let src = format!(
        "Open \"{p}\" For Output As #1\nPrint #1, \"first\"\nPrint #1, \"second\"\nClose #1\n\
         Open \"{p}\" For Input As #2\nLine Input #2, a\nLine Input #2, b\nPrint a; b\nClose #2\n",
        p = path
    );
    // `Print a; b` joins the values with nothing between them, as VB does
    assert_eq!(run(&src).unwrap(), "firstsecond\n");
}

#[test]
fn input_reads_comma_separated_values_and_quoted_text() {
    let (_dir, path) = folder("input_values");
    let src = format!(
        "Open \"{p}\" For Output As #1\nWrite #1, \"Smith, J\", 42, 3.5\nClose #1\n\
         Open \"{p}\" For Input As #2\nInput #2, name, age, height\nPrint name; age; height\nClose #2\n",
        p = path
    );
    // a number prints with a space either side, as VB prints numbers
    assert_eq!(run(&src).unwrap(), "Smith, J 42  3.5 \n");
}

#[test]
fn eof_is_true_after_the_last_line_is_read() {
    let (_dir, path) = folder("eof");
    let src = format!(
        "Open \"{p}\" For Output As #1\nPrint #1, \"only\"\nClose #1\n\
         Open \"{p}\" For Input As #2\nLine Input #2, a\nPrint EOF(2)\nClose #2\n",
        p = path
    );
    assert_eq!(run(&src).unwrap(), "True\n");
}

#[test]
fn append_adds_to_the_end_of_an_existing_file() {
    let (_dir, path) = folder("append");
    let src = format!(
        "Open \"{p}\" For Output As #1\nPrint #1, \"one\"\nClose #1\n\
         Open \"{p}\" For Append As #1\nPrint #1, \"two\"\nClose #1\n\
         Open \"{p}\" For Input As #2\nLine Input #2, a\nLine Input #2, b\nPrint a; b\nClose #2\n",
        p = path
    );
    assert_eq!(run(&src).unwrap(), "onetwo\n");
}

#[test]
fn opening_a_missing_file_for_input_is_error_53() {
    let (_dir, path) = folder("missing");
    let err = run(&format!("Open \"{}\" For Input As #1\n", path)).unwrap_err();
    assert!(err.contains("runtime error 53"), "{}", err);
}

#[test]
fn reading_past_the_end_is_error_62() {
    let (_dir, path) = folder("past_end");
    let src = format!(
        "Open \"{p}\" For Output As #1\nPrint #1, \"x\"\nClose #1\n\
         Open \"{p}\" For Input As #2\nLine Input #2, a\nLine Input #2, b\nClose #2\n",
        p = path
    );
    let err = run(&src).unwrap_err();
    assert!(err.contains("runtime error 62"), "{}", err);
}

#[test]
fn using_a_file_number_twice_is_error_55() {
    let (_dir, path) = folder("already_open");
    let src = format!(
        "Open \"{p}\" For Output As #1\nOpen \"{p}\" For Output As #1\n",
        p = path
    );
    let err = run(&src).unwrap_err();
    assert!(err.contains("runtime error 55"), "{}", err);
}

#[test]
fn a_file_number_out_of_range_is_error_52() {
    let err = run("Open \"x.txt\" For Output As #0\n").unwrap_err();
    assert!(err.contains("runtime error 52"), "{}", err);
}

#[test]
fn an_unclosed_output_file_is_written_when_the_program_ends() {
    let (_dir, path) = folder("unclosed");
    let src = format!("Open \"{p}\" For Output As #1\nPrint #1, \"kept\"\n", p = path);
    run(&src).unwrap();
    assert_eq!(std::fs::read_to_string(path.replace("\"\"", "\"")).unwrap(), "kept\n");
}
