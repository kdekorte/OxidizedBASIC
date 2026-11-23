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
fn a_binary_put_writes_exactly_the_value_bytes_and_seek_is_a_byte_position() {
    let dir = std::env::temp_dir().join("oxbasic_binary_exact");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("b.bin");
    let path = file.to_string_lossy().replace('"', "\"\"");
    let src = format!(
        "Dim n As Integer\nn = 258\nOpen \"{p}\" For Binary As #1\nPut #1, , n\nPut #1, , n\nSeek #1, 3\nGet #1, , n\nClose #1\nPrint n\n",
        p = path
    );
    assert_eq!(run(&src).unwrap(), " 258 \n");
    // two 2-byte Integers, not 128-byte records
    assert_eq!(std::fs::read(&file).unwrap(), vec![2, 1, 2, 1]);
}
