//! File I/O for the interpreter: VB's sequential files. A file is opened under
//! a number (`Open ... As #1`), read with `Input #` or `Line Input #`, written
//! with `Print #` or `Write #`, and closed with `Close`. Relative file names
//! are in the project's folder (`base_dir`), as `LoadPicture` resolves them.

use crate::{err, ExecResult};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};

/// VB's largest file number.
pub const MAX_FILE_NUMBER: i64 = 511;

/// One open file: its path, and how it is read or written.
pub enum OpenFile {
    /// Opened for `Input`: the lines not read yet, and the file's path.
    Input {
        path: String,
        lines: std::collections::VecDeque<String>,
        /// The fields of the line being read by `Input #`, not yet used.
        fields: std::collections::VecDeque<Field>,
    },
    /// Opened for `Output` or `Append`: the writer.
    Output {
        path: String,
        file: std::io::BufWriter<std::fs::File>,
    },
}

/// One value read by `Input #`: a quoted or text field, or a bare field that
/// may be a number.
pub enum Field {
    Text(String),
    Number(String),
}

/// Split one line into `Input #` fields: comma-separated, a quoted field keeps
/// its commas, and the quotes are removed.
pub fn split_fields(line: &str) -> std::collections::VecDeque<Field> {
    let mut out = std::collections::VecDeque::new();
    let mut chars = line.chars().peekable();
    loop {
        // skip the blanks before a field
        while matches!(chars.peek(), Some(' ') | Some('\t')) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        if chars.peek() == Some(&'"') {
            chars.next();
            let mut s = String::new();
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
                s.push(c);
            }
            out.push_back(Field::Text(s));
        } else {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c == ',' {
                    break;
                }
                s.push(c);
                chars.next();
            }
            let t = s.trim().to_string();
            out.push_back(Field::Number(t));
        }
        // the comma after a field, if any
        while matches!(chars.peek(), Some(' ') | Some('\t')) {
            chars.next();
        }
        if chars.peek() == Some(&',') {
            chars.next();
        } else {
            break;
        }
    }
    out
}

#[derive(Default)]
pub struct FileTable {
    files: BTreeMap<i64, OpenFile>,
}

impl FileTable {
    /// Check a file number is in VB's range.
    pub fn check_number(n: i64) -> ExecResult<()> {
        if (1..=MAX_FILE_NUMBER).contains(&n) {
            Ok(())
        } else {
            err(52, "Bad file name or number")
        }
    }

    /// Whether file `n` is open.
    pub fn is_open(&self, n: i64) -> bool {
        self.files.contains_key(&n)
    }

    /// Open `path` under number `n`, in `mode`. `full` is the path to open
    /// (the project's folder, resolved). Errors as VB does: 52 for a bad
    /// number, 55 when the number is already in use, 53 when an input file is
    /// missing.
    pub fn open(
        &mut self,
        n: i64,
        path: &str,
        full: &std::path::Path,
        mode: oxbasic_ast::FileMode,
    ) -> ExecResult<()> {
        Self::check_number(n)?;
        if self.is_open(n) {
            return err(55, "File already open");
        }
        let entry = match mode {
            oxbasic_ast::FileMode::Input => {
                let file = std::fs::File::open(full)
                    .map_err(|_| not_found(path))?;
                let lines = BufReader::new(file)
                    .lines()
                    .map(|l| l.unwrap_or_default())
                    .collect();
                OpenFile::Input {
                    fields: std::collections::VecDeque::new(),
                    path: path.to_string(),
                    lines,
                }
            }
            oxbasic_ast::FileMode::Output | oxbasic_ast::FileMode::Append => {
                let file = std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(mode == oxbasic_ast::FileMode::Append)
                    .truncate(mode == oxbasic_ast::FileMode::Output)
                    .open(full)
                    .map_err(|e| io_error(path, e))?;
                OpenFile::Output {
                    path: path.to_string(),
                    file: std::io::BufWriter::new(file),
                }
            }
        };
        self.files.insert(n, entry);
        Ok(())
    }

    /// Close file `n`; an unopened number is error 52 in VB. Closing an
    /// output file writes its buffered text.
    pub fn close(&mut self, n: i64) -> ExecResult<()> {
        match self.files.remove(&n) {
            Some(OpenFile::Output { mut file, .. }) => {
                file.flush().map_err(|e| io_error("", e))
            }
            Some(_) => Ok(()),
            None => err(52, "Bad file name or number"),
        }
    }

    /// Close every open file, writing what is buffered.
    pub fn close_all(&mut self) -> ExecResult<()> {
        let open: Vec<i64> = self.files.keys().copied().collect();
        for n in open {
            self.close(n)?;
        }
        Ok(())
    }

    /// The next `Input #` field of file `n`. An input file keeps the fields of
    /// its current line; when they are used up, the next line is read. Error
    /// 62 past the end of the file.
    pub fn next_field(&mut self, n: i64) -> ExecResult<Field> {
        loop {
            match self.files.get_mut(&n) {
                Some(OpenFile::Input { lines, fields, .. }) => {
                    if let Some(field) = fields.pop_front() {
                        return Ok(field);
                    }
                    match lines.pop_front() {
                        Some(line) => *fields = split_fields(&line),
                        None => return err(62, "Input past end of file"),
                    }
                }
                Some(OpenFile::Output { .. }) => return err(54, "Bad file mode"),
                None => return err(52, "Bad file name or number"),
            }
        }
    }

    /// The next line of input file `n`, without its line break; error 62 at
    /// the end of the file, or 52 when `n` is not open for input.
    pub fn read_line(&mut self, n: i64) -> ExecResult<String> {
        match self.files.get_mut(&n) {
            Some(OpenFile::Input { lines, .. }) => match lines.pop_front() {
                Some(line) => Ok(line),
                None => err(62, "Input past end of file"),
            },
            Some(OpenFile::Output { .. }) => err(54, "Bad file mode"),
            None => err(52, "Bad file name or number"),
        }
    }

    /// Whether input file `n` has no more lines (`EOF(n)`).
    pub fn at_end(&self, n: i64) -> ExecResult<bool> {
        match self.files.get(&n) {
            Some(OpenFile::Input { lines, .. }) => Ok(lines.is_empty()),
            Some(OpenFile::Output { .. }) => err(54, "Bad file mode"),
            None => err(52, "Bad file name or number"),
        }
    }

    /// Write `text` and a line break to output file `n`; error 52 when `n`
    /// is not open for output.
    pub fn write_line(&mut self, n: i64, text: &str) -> ExecResult<()> {
        match self.files.get_mut(&n) {
            Some(OpenFile::Output { file, .. }) => {
                writeln!(file, "{}", text).map_err(|e| io_error("", e))
            }
            Some(OpenFile::Input { .. }) => err(54, "Bad file mode"),
            None => err(52, "Bad file name or number"),
        }
    }
}

fn not_found(path: &str) -> crate::VbError {
    crate::VbError {
        number: 53,
        description: format!("File not found: {}", path),
        line: None,
    }
}

fn io_error(path: &str, e: std::io::Error) -> crate::VbError {
    crate::VbError {
        number: 75,
        description: format!("Path/File access error: {} {}", path, e),
        line: None,
    }
}
