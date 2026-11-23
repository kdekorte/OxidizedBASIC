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
    let dir = std::env::temp_dir().join(format!("oxbasic_direct_{}", test));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp folder");
    let file = dir.join("data.dat");
    let literal = file.to_string_lossy().replace('"', "\"\"");
    (dir, literal)
}

#[test]
fn a_random_file_round_trips_records_by_number() {
    let (_dir, path) = folder("random");
    let src = format!(
        "Open \"{p}\" For Random As #1 Len = 8\nPut #1, 2, \"second\"\nPut #1, 1, \"first\"\nClose #1\n\
         Open \"{p}\" For Random As #2 Len = 8\nGet #2, 1, a\nGet #2, 2, b\nPrint a; \"|\"; b\nClose #2\n",
        p = path
    );
    assert_eq!(run(&src).unwrap(), "first|second\n");
}

#[test]
fn a_random_record_is_padded_to_the_record_length() {
    let (_dir, path) = folder("random_pad");
    // a fixed-length string is 4 bytes; the record is 6, so 2 bytes of padding
    let src = format!(
        "Type Rec\nName As String * 4\nEnd Type\nDim r As Rec\nr.Name = \"ab\"\nOpen \"{p}\" For Random As #1 Len = 6\nPut #1, 1, r\nClose #1\n",
        p = path
    );
    run(&src).unwrap();
    assert_eq!(std::fs::read(path.replace("\"\"", "\"")).unwrap(), b"ab    ");
}

#[test]
fn a_type_record_is_written_in_vb_layout_and_read_back() {
    let (_dir, path) = folder("type_layout");
    let src = format!(
        "Type Rec\nName As String * 4\nAge As Integer\nEnd Type\nDim w As Rec\nDim r As Rec\nw.Name = \"Al\"\nw.Age = 258\nOpen \"{p}\" For Random As #1 Len = 6\nPut #1, 1, w\nClose #1\nOpen \"{p}\" For Random As #2 Len = 6\nGet #2, 1, r\nClose #2\nPrint r.Name; \"|\"; r.Age\n",
        p = path
    );
    assert_eq!(run(&src).unwrap(), "Al  | 258 \n");
    // the bytes VB writes: "Al  " then 258 as a little-endian 2-byte Integer
    assert_eq!(std::fs::read(path.replace("\"\"", "\"")).unwrap(), vec![b'A', b'l', b' ', b' ', 2, 1]);
}

#[test]
fn a_random_record_number_below_one_is_error_63() {
    let (_dir, path) = folder("random_bad");
    let src = format!("Open \"{p}\" For Random As #1 Len = 8\nPut #1, 0, \"x\"\n", p = path);
    let err = run(&src).unwrap_err();
    assert!(err.contains("runtime error 63"), "{}", err);
}

#[test]
fn a_binary_file_round_trips_by_position_with_seek() {
    let (_dir, path) = folder("binary");
    let src = format!(
        "Open \"{p}\" For Binary As #1\nPut #1, , \"hello\"\nSeek #1, 1\nGet #1, , a\nPrint a\nClose #1\n",
        p = path
    );
    assert_eq!(run(&src).unwrap(), "hello\n");
}

#[test]
fn a_random_file_is_not_read_with_input() {
    let (_dir, path) = folder("random_input");
    let src = format!("Open \"{p}\" For Random As #1 Len = 8\nInput #1, a\n", p = path);
    let err = run(&src).unwrap_err();
    assert!(err.contains("runtime error 54"), "{}", err);
}

#[test]
fn loc_and_lof_report_the_record_and_the_length() {
    let (_dir, path) = folder("loc_lof");
    let src = format!("Open \"{p}\" For Random As #1 Len = 4\nPut #1, 1, \"ab\"\nPut #1, 2, \"cd\"\nPrint Lof(1)\nPrint Loc(1)\nClose #1\n", p = path);
    assert_eq!(run(&src).unwrap(), " 8 \n 2 \n");
}
