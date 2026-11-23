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
fn a_type_inside_a_type_is_written_field_by_field_and_read_back() {
    let dir = std::env::temp_dir().join("oxbasic_nested_type");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("n.dat");
    let path = file.to_string_lossy().replace('"', "\"\"");
    let src = format!(
        "Type Inner\nN As String * 2\nEnd Type\nType Outer\nI As Inner\nAge As Integer\nEnd Type\n\
         Dim w As Outer\nDim r As Outer\nw.I.N = \"Hi\"\nw.Age = 7\n\
         Open \"{p}\" For Random As #1 Len = 4\nPut #1, 1, w\nClose #1\n\
         Open \"{p}\" For Random As #2 Len = 4\nGet #2, 1, r\nClose #2\n\
         Print r.I.N; \"|\"; r.Age\n",
        p = path
    );
    assert_eq!(run(&src).unwrap(), "Hi| 7 \n");
    // Inner is 2 bytes and Age 2 bytes: 4 bytes, the record length
    assert_eq!(std::fs::read(&file).unwrap(), vec![b'H', b'i', 7, 0]);
}
