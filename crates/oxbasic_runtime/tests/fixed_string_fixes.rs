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

#[test]
fn a_fixed_string_starts_as_spaces_and_pads_or_cuts_on_assignment() {
    let src = "Dim s As String * 5\nPrint Len(s); \"[\"; s; \"]\"\ns = \"abc\"\nPrint Len(s); \"[\"; s; \"]\"\ns = \"abcdefgh\"\nPrint Len(s); \"[\"; s; \"]\"\n";
    assert_eq!(run(src).unwrap(), " 5 [     ]\n 5 [abc  ]\n 5 [abcde]\n");
}

#[test]
fn a_fixed_string_field_of_a_type_pads_on_assignment() {
    let src = "Type R\nN As String * 4\nEnd Type\nDim r As R\nPrint Len(r.N)\nr.N = \"xy\"\nPrint Len(r.N); \"[\"; r.N; \"]\"\n";
    assert_eq!(run(src).unwrap(), " 4 \n 4 [xy  ]\n");
}

#[test]
fn a_random_record_with_a_dynamic_string_round_trips() {
    let dir = std::env::temp_dir().join("oxbasic_dyn_string_record");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("d.dat").to_string_lossy().replace('"', "\"\"");
    let src = format!(
        "Type R\nN As String\nA As Integer\nEnd Type\nDim w As R\nDim r As R\nw.N = \"hello\"\nw.A = 9\n\
         Open \"{p}\" For Random As #1 Len = 64\nPut #1, 1, w\nClose #1\n\
         Open \"{p}\" For Random As #2 Len = 64\nGet #2, 1, r\nClose #2\nPrint r.N; \"|\"; r.A\n",
        p = path
    );
    assert_eq!(run(&src).unwrap(), "hello| 9 \n");
}

#[test]
fn a_record_longer_than_its_length_is_an_error_not_cut_short() {
    let dir = std::env::temp_dir().join("oxbasic_record_too_long");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("l.dat");
    let path = file.to_string_lossy().replace('"', "\"\"");
    let src = format!(
        "Type R\nN As String\nEnd Type\nDim w As R\nw.N = \"this is a long name\"\n\
         Open \"{p}\" For Random As #1 Len = 8\nPut #1, 1, w\nClose #1\n",
        p = path
    );
    let err = run(&src).unwrap_err();
    assert!(err.contains("runtime error 63"), "{}", err);
}
