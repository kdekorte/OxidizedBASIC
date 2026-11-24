//! Standard library: string, math, date, conversion, and array functions
//! for the OxidizedBASIC runtime (Phase 1).

use crate::*;

/// Names recognized as builtins (used to disambiguate array indexing).
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "abs"
            | "sqr"
            | "int"
            | "fix"
            | "sgn"
            | "sin"
            | "cos"
            | "tan"
            | "atn"
            | "exp"
            | "log"
            | "rnd"
            | "randomize"
            | "hex"
            | "oct"
            | "len"
            | "left"
            | "right"
            | "mid"
            | "ucase"
            | "lcase"
            | "trim"
            | "ltrim"
            | "rtrim"
            | "instr"
            | "instrrev"
            | "replace"
            | "space"
            | "string"
            | "strreverse"
            | "chr"
            | "asc"
            | "cstr"
            | "str"
            | "val"
            | "format"
            | "round"
            | "choose"
            | "switch"
            | "timer"
            | "eof"
            | "loc"
            | "lof"
            | "freefile"
            | "split"
            | "join"
            | "strcomp"
            | "cint"
            | "clng"
            | "cdbl"
            | "csng"
            | "cbool"
            | "ccur"
            | "cvar"
            | "cdate"
            | "isnumeric"
            | "isarray"
            | "isempty"
            | "isnull"
            | "isdate"
            | "vartype"
            | "typename"
            | "array"
            | "ubound"
            | "lbound"
            | "now"
            | "date"
            | "time"
            | "year"
            | "month"
            | "day"
            | "hour"
            | "minute"
            | "second"
            | "weekday"
            | "dateserial"
            | "timeserial"
            | "datevalue"
            | "dateadd"
            | "msgbox"
            | "inputbox"
            | "openfiledialog"
            | "savefiledialog"
            | "beep"
            | "doevents"
            | "iif"
            | "rgb"
            | "loadpicture"
    )
}

fn arg(args: &[Variant], i: usize) -> ExecResult<&Variant> {
    args.get(i).ok_or_else(|| VbError {
        number: 450,
        description: "wrong number of arguments".into(),
        line: None,
    })
}

fn want_str(args: &[Variant], i: usize) -> ExecResult<String> {
    to_string(arg(args, i)?)
}

fn want_f64(args: &[Variant], i: usize) -> ExecResult<f64> {
    to_f64(arg(args, i)?)
}

fn want_i64(args: &[Variant], i: usize) -> ExecResult<i64> {
    to_i64(arg(args, i)?)
}

impl<'out> Interpreter<'out> {
    fn next_rnd(&self) -> f64 {
        // xorshift64*
        let mut x = self.rng.get();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng.set(x);
        let v = x.wrapping_mul(0x2545F4914F6CDD1D);
        ((v >> 11) as f64) / ((1u64 << 53) as f64)
    }

    pub(crate) fn builtin(
        &mut self,
        name: &str,
        args: &[Variant],
        _f: &mut Frame,
    ) -> ExecResult<Variant> {
        match name {
            // -- math ------------------------------------------------------
            "abs" => Ok(match arg(args, 0)? {
                Variant::Long(x) => Variant::Long(x.abs()),
                Variant::Integer(x) => Variant::Integer(x.abs()),
                other => numeric(to_f64(other)?.abs()),
            }),
            "sqr" => {
                let x = want_f64(args, 0)?;
                if x < 0.0 {
                    return err(5, "invalid procedure call: Sqr of negative");
                }
                Ok(Variant::Double(x.sqrt()))
            }
            "int" => Ok(numeric(want_f64(args, 0)?.floor())),
            "fix" => Ok(numeric(want_f64(args, 0)?.trunc())),
            "sgn" => {
                let x = want_f64(args, 0)?;
                Ok(Variant::Long(if x > 0.0 {
                    1
                } else if x < 0.0 {
                    -1
                } else {
                    0
                }))
            }
            "sin" => Ok(Variant::Double(want_f64(args, 0)?.sin())),
            "cos" => Ok(Variant::Double(want_f64(args, 0)?.cos())),
            "tan" => Ok(Variant::Double(want_f64(args, 0)?.tan())),
            "atn" => Ok(Variant::Double(want_f64(args, 0)?.atan())),
            "exp" => Ok(Variant::Double(want_f64(args, 0)?.exp())),
            "log" => {
                let x = want_f64(args, 0)?;
                if x <= 0.0 {
                    return err(5, "invalid procedure call: Log of non-positive");
                }
                Ok(Variant::Double(x.ln()))
            }
            "hex" => Ok(Variant::String(format!("{:X}", want_i64(args, 0)?))),
            "oct" => Ok(Variant::String(format!("{:o}", want_i64(args, 0)?))),
            "rnd" => Ok(Variant::Double(self.next_rnd())),
            "randomize" => {
                let seed = match args.first() {
                    Some(v) => to_f64(v)? as u64,
                    None => std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.subsec_nanos() as u64)
                        .unwrap_or(42),
                };
                self.rng.set(seed | 1);
                Ok(Variant::Empty)
            }

            // -- strings ---------------------------------------------------
            "len" => Ok(Variant::Long(
                want_str(args, 0)?.encode_utf16().count() as i32
            )),
            "left" => {
                let s = want_str(args, 0)?;
                let n = want_i64(args, 1)?.max(0) as usize;
                Ok(Variant::String(s.chars().take(n).collect()))
            }
            "right" => {
                let s = want_str(args, 0)?;
                let n = want_i64(args, 1)?.max(0) as usize;
                let cnt = s.chars().count();
                Ok(Variant::String(
                    s.chars().skip(cnt.saturating_sub(n)).collect(),
                ))
            }
            "mid" => {
                let s = want_str(args, 0)?;
                let start = want_i64(args, 1)?;
                if start < 1 {
                    return err(5, "invalid procedure call: Mid start < 1");
                }
                let len = if args.len() > 2 {
                    want_i64(args, 2)?.max(0) as usize
                } else {
                    usize::MAX
                };
                Ok(Variant::String(
                    s.chars().skip(start as usize - 1).take(len).collect(),
                ))
            }
            "ucase" => Ok(Variant::String(want_str(args, 0)?.to_uppercase())),
            "lcase" => Ok(Variant::String(want_str(args, 0)?.to_lowercase())),
            "trim" => Ok(Variant::String(want_str(args, 0)?.trim().to_string())),
            "ltrim" => Ok(Variant::String(want_str(args, 0)?.trim_start().to_string())),
            "rtrim" => Ok(Variant::String(want_str(args, 0)?.trim_end().to_string())),
            "instr" => {
                let (start, hay, needle) = if args.len() >= 3 {
                    (
                        want_i64(args, 0)? as usize,
                        want_str(args, 1)?,
                        want_str(args, 2)?,
                    )
                } else {
                    (0, want_str(args, 0)?, want_str(args, 1)?)
                };
                if needle.is_empty() {
                    return Ok(Variant::Long(1));
                }
                let pos = hay[start.min(hay.len())..]
                    .find(&needle)
                    .map(|i| i + start + 1);
                Ok(Variant::Long(pos.unwrap_or(0) as i32))
            }
            "instrrev" => {
                let hay = want_str(args, 0)?;
                let needle = want_str(args, 1)?;
                Ok(Variant::Long(
                    hay.rfind(&needle).map(|i| i + 1).unwrap_or(0) as i32,
                ))
            }
            "replace" => {
                let s = want_str(args, 0)?;
                let find = want_str(args, 1)?;
                let repl = want_str(args, 2)?;
                if find.is_empty() {
                    return Ok(Variant::String(s));
                }
                Ok(Variant::String(s.replace(&find, &repl)))
            }
            "space" => {
                let n = want_i64(args, 0)?.max(0) as usize;
                Ok(Variant::String(" ".repeat(n)))
            }
            "string" => {
                let n = want_i64(args, 0)?.max(0) as usize;
                let ch = match arg(args, 1)? {
                    Variant::String(s) => s.chars().next().unwrap_or(' '),
                    other => char::from_u32(to_i64(other)? as u32).unwrap_or(' '),
                };
                Ok(Variant::String(ch.to_string().repeat(n)))
            }
            "strreverse" => Ok(Variant::String(want_str(args, 0)?.chars().rev().collect())),
            "chr" => {
                let code = want_i64(args, 0)?;
                match char::from_u32(code as u32) {
                    Some(c) => Ok(Variant::String(c.to_string())),
                    None => err(5, "invalid procedure call: bad Chr code"),
                }
            }
            "asc" => {
                let s = want_str(args, 0)?;
                match s.chars().next() {
                    Some(c) => Ok(Variant::Long(c as i32)),
                    None => err(5, "invalid procedure call: Asc of empty string"),
                }
            }
            "str" => {
                let x = want_f64(args, 0)?;
                Ok(Variant::String(format!(
                    "{}{}",
                    if x >= 0.0 { " " } else { "" },
                    fmt_num(x)
                )))
            }
            "val" => {
                let s = want_str(args, 0)?;
                let t = s.trim_start();
                let num: String = t
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
                    .collect();
                Ok(numeric(num.parse::<f64>().unwrap_or(0.0)))
            }
            "strcomp" => {
                let a = want_str(args, 0)?;
                let b = want_str(args, 1)?;
                Ok(Variant::Long(match a.cmp(&b) {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                }))
            }
            "iif" => {
                let c = to_bool(arg(args, 0)?)?;
                Ok(if c {
                    arg(args, 1)?.clone()
                } else {
                    arg(args, 2)?.clone()
                })
            }
            // RGB(r, g, b) = r + g*256 + b*65536; each part is 0..255
            "rgb" => {
                let part = |i: usize| -> ExecResult<i32> {
                    let n = want_i64(args, i)?;
                    if !(0..=255).contains(&n) {
                        return Err(VbError {
                            number: 5,
                            description: "invalid procedure call or argument".into(),
                            line: None,
                        });
                    }
                    Ok(n as i32)
                };
                let (r, g, b) = (part(0)?, part(1)?, part(2)?);
                Ok(Variant::Long(r | (g << 8) | (b << 16)))
            }

            // `LoadPicture(path)`: a picture is its file here (what a
            // PictureBox's `Picture` holds); no argument, or "", is no
            // picture. A file that isn't there is VB's error 53.
            "loadpicture" => {
                let path = match args.first() {
                    None | Some(Variant::Empty) => String::new(),
                    Some(v) => to_string(v)?,
                };
                // relative to the project's folder, which a program run
                // from the IDE is given
                let full = match &self.base_dir {
                    Some(dir) => dir.join(&path),
                    None => std::path::PathBuf::from(&path),
                };
                if !path.is_empty() && !full.is_file() {
                    return Err(VbError {
                        number: 53,
                        description: format!("File not found: {}", path),
                        line: None,
                    });
                }
                Ok(Variant::String(path))
            }

            // -- conversions ------------------------------------------------
            "cint" | "clng" => Ok(Variant::Long(to_i32(arg(args, 0)?)?)),
            "cdbl" | "csng" | "ccur" | "cvar" => Ok(Variant::Double(want_f64(args, 0)?)),
            "cbool" => Ok(Variant::Boolean(to_bool(arg(args, 0)?)?)),
            "cstr" => Ok(Variant::String(to_string(arg(args, 0)?)?)),
            "cdate" => Ok(Variant::Date(want_f64(args, 0)?)),
            "isnumeric" => Ok(Variant::Boolean(
                matches!(
                    arg(args, 0)?,
                    Variant::Integer(_)
                        | Variant::Long(_)
                        | Variant::Single(_)
                        | Variant::Double(_)
                        | Variant::Currency(_)
                        | Variant::Date(_)
                        | Variant::Empty
                        | Variant::Boolean(_)
                ) || matches!(
                    arg(args, 0)?,
                    Variant::String(s) if s.trim().parse::<f64>().is_ok()
                ),
            )),
            "isarray" => Ok(Variant::Boolean(matches!(arg(args, 0)?, Variant::Array(_)))),
            "isempty" => Ok(Variant::Boolean(matches!(arg(args, 0)?, Variant::Empty))),
            "isnull" => Ok(Variant::Boolean(matches!(arg(args, 0)?, Variant::Null))),
            "isdate" => Ok(Variant::Boolean(matches!(arg(args, 0)?, Variant::Date(_)))),
            "vartype" => Ok(Variant::Long(match arg(args, 0)? {
                Variant::Empty => 0,
                Variant::Null => 1,
                Variant::Integer(_) => 2,
                Variant::Long(_) => 3,
                Variant::Single(_) => 4,
                Variant::Double(_) => 5,
                Variant::Currency(_) => 6,
                Variant::Date(_) => 7,
                Variant::String(_) => 8,
                Variant::Boolean(_) => 11,
                Variant::Nothing => 9,
                Variant::Array(_) => 8204,
                Variant::Record(_) => 8204,
            })),
            "typename" => Ok(Variant::String(
                match arg(args, 0)? {
                    Variant::Empty => "Empty",
                    Variant::Null => "Null",
                    Variant::Integer(_) => "Integer",
                    Variant::Long(_) => "Long",
                    Variant::Single(_) => "Single",
                    Variant::Double(_) => "Double",
                    Variant::Currency(_) => "Currency",
                    Variant::Date(_) => "Date",
                    Variant::String(_) => "String",
                    Variant::Boolean(_) => "Boolean",
                    Variant::Array(_) => "Variant()",
                    Variant::Record(r) => return Ok(Variant::String(r.ty.clone())),
                    Variant::Nothing => "Nothing",
                }
                .to_string(),
            )),

            // -- arrays ----------------------------------------------------
            "array" => {
                let n = args.len();
                Ok(Variant::Array(std::rc::Rc::new(std::cell::RefCell::new(
                    VbArray {
                        dims: vec![n],
                        data: args.to_vec(),
                        lower: self.option_base,
                    },
                ))))
            }
            // `LBound` is the first index; `UBound` the last (both per dimension)
            "ubound" | "lbound" => {
                let arr = match arg(args, 0)? {
                    Variant::Array(a) => a,
                    _ => return err(13, "type mismatch: expected an array"),
                };
                let dim = if args.len() > 1 {
                    want_i64(args, 1)?
                } else {
                    1
                };
                let b = arr.borrow();
                if b.dims.is_empty() {
                    return if name == "ubound" {
                        Ok(Variant::Long((b.lower + b.data.len() as i64 - 1) as i32))
                    } else {
                        Ok(Variant::Long(b.lower as i32))
                    };
                }
                let ix = (dim - 1) as usize;
                if ix >= b.dims.len() {
                    return err(9, "subscript out of range");
                }
                Ok(Variant::Long(if name == "ubound" {
                    (b.lower + b.dims[ix] as i64 - 1) as i32
                } else {
                    b.lower as i32
                }))
            }
            "split" => {
                let s = want_str(args, 0)?;
                let d = if args.len() > 1 {
                    want_str(args, 1)?
                } else {
                    " ".into()
                };
                let parts: Vec<Variant> = if d.is_empty() {
                    s.chars().map(|c| Variant::String(c.to_string())).collect()
                } else {
                    s.split(d.as_str())
                        .map(|p| Variant::String(p.to_string()))
                        .collect()
                };
                let n = parts.len();
                Ok(Variant::Array(std::rc::Rc::new(std::cell::RefCell::new(
                    VbArray {
                        dims: vec![n],
                        data: parts,
                        lower: self.option_base,
                    },
                ))))
            }
            "join" => {
                let arr = match arg(args, 0)? {
                    Variant::Array(a) => a.borrow().data.clone(),
                    _ => return err(13, "type mismatch: Join requires an array"),
                };
                let d = if args.len() > 1 {
                    want_str(args, 1)?
                } else {
                    " ".into()
                };
                let mut parts = Vec::new();
                for v in arr {
                    parts.push(to_string(&v)?);
                }
                Ok(Variant::String(parts.join(&d)))
            }

            // -- dates -----------------------------------------------------
            "now" => Ok(Variant::Date(now_serial())),
            "date" => Ok(Variant::Date(now_serial().floor())),
            "time" => Ok(Variant::Date(now_serial().fract())),
            "year" | "month" | "day" | "weekday" => {
                let d = want_f64(args, 0)?.floor() as i64;
                let (y, m, dd) = civil_from_days(d);
                Ok(Variant::Long(match name {
                    "year" => y as i32,
                    "month" => m as i32,
                    "day" => dd as i32,
                    // weekday: day 0 (1899-12-30) was a Saturday (vbSunday=1)
                    _ => ((d % 7 + 7 + 6) % 7 + 1) as i32,
                }))
            }
            "hour" | "minute" | "second" => {
                let frac = want_f64(args, 0)?.fract();
                let secs = (frac * 86400.0).round() as u64;
                Ok(Variant::Long(match name {
                    "hour" => (secs / 3600) as i32,
                    "minute" => ((secs % 3600) / 60) as i32,
                    _ => (secs % 60) as i32,
                }))
            }
            "dateserial" => {
                let y = want_i64(args, 0)?;
                let m = want_i64(args, 1)? as u32;
                let d = want_i64(args, 2)? as u32;
                Ok(Variant::Date(days_from_civil(y, m, d) as f64))
            }
            "timeserial" => {
                let h = want_f64(args, 0)?;
                let m = want_f64(args, 1)?;
                let s = want_f64(args, 2)?;
                Ok(Variant::Date((h * 3600.0 + m * 60.0 + s) / 86400.0))
            }
            "dateadd" => {
                let unit = want_str(args, 0)?.to_uppercase();
                let n = want_f64(args, 1)?;
                let d = want_f64(args, 2)?;
                Ok(Variant::Date(match unit.as_str() {
                    "YYYY" => {
                        let (y, m, dd) = civil_from_days(d.floor() as i64);
                        days_from_civil(y + n as i64, m, dd) as f64 + d.fract()
                    }
                    "M" => {
                        let (y, m, dd) = civil_from_days(d.floor() as i64);
                        let total = y * 12 + (m as i64 - 1) + n as i64;
                        days_from_civil(total.div_euclid(12), (total.rem_euclid(12) + 1) as u32, dd)
                            as f64
                            + d.fract()
                    }
                    "D" | "Y" | "W" => d + n,
                    "WW" => d + n * 7.0,
                    "H" => d + n / 24.0,
                    "N" => d + n / 1440.0,
                    "S" => d + n / 86400.0,
                    _ => return err(5, "invalid procedure call: unknown DateAdd interval"),
                }))
            }
            "datevalue" => {
                let s = want_str(args, 0)?;
                parse_date_str(&s)
            }

            // -- console / misc --------------------------------------------
            "msgbox" => {
                let prompt = want_str(args, 0)?;
                self.out.write(&prompt);
                self.out.write("\n");
                Ok(Variant::Long(1))
            }
            "inputbox" => {
                let prompt = want_str(args, 0)?;
                self.out.write(&prompt);
                let line = self.out.read_line().unwrap_or_default();
                Ok(Variant::String(line))
            }
            // a cancelled dialog, or a filter that rejects the choice, gives ""
            "openfiledialog" | "savefiledialog" => {
                let filter = if args.is_empty() {
                    String::new()
                } else {
                    want_str(args, 0)?
                };
                let save = name == "savefiledialog";
                let chosen = self.out.choose_file(&filter, save).unwrap_or_default();
                Ok(Variant::String(if matches_filter(&chosen, &filter) {
                    chosen
                } else {
                    String::new()
                }))
            }
            "beep" => Ok(Variant::Empty),
            "doevents" => {
                self.do_events()?;
                Ok(Variant::Empty)
            }

            "format" => {
                let v = arg(args, 0)?.clone();
                let pat = if args.len() > 1 {
                    want_str(args, 1)?
                } else {
                    String::new()
                };
                format_value(&v, &pat)
            }

            // Round(x [, digits]): VB rounds half to even (banker's rounding)
            "round" => {
                let x = want_f64(args, 0)?;
                let digits = if args.len() > 1 {
                    to_i64(arg(args, 1)?)?
                } else {
                    0
                };
                let scale = 10f64.powi(digits as i32);
                let scaled = x * scale;
                let rounded = if (scaled - scaled.trunc()).abs() == 0.5 {
                    // exactly halfway: take the even neighbour
                    let t = scaled.trunc();
                    if t % 2.0 == 0.0 {
                        t
                    } else {
                        t + scaled.signum()
                    }
                } else {
                    scaled.round()
                };
                Ok(numeric(rounded / scale))
            }

            // Choose(index, value1, value2, ...): the value at index (1-based),
            // or Null when index is out of range
            "choose" => {
                let i = to_i64(arg(args, 0)?)?;
                if i >= 1 && (i as usize) < args.len() {
                    Ok(args[i as usize].clone())
                } else {
                    Ok(Variant::Null)
                }
            }

            // Switch(expr1, value1, expr2, value2, ...): the value of the first
            // true expression; Null when none is true
            "switch" => {
                let mut i = 0;
                while i + 1 < args.len() {
                    if to_bool(&args[i])? {
                        return Ok(args[i + 1].clone());
                    }
                    i += 2;
                }
                Ok(Variant::Null)
            }

            // Timer: seconds since midnight, as a Single
            "eof" => {
                let n = want_i64(args, 0)?;
                Ok(Variant::Boolean(self.files.at_end(n)?))
            }
            "loc" => {
                let n = want_i64(args, 0)?;
                Ok(Variant::Long(self.files.loc(n)? as i32))
            }
            "lof" => {
                let n = want_i64(args, 0)?;
                Ok(Variant::Long(self.files.lof(n)? as i32))
            }
            "freefile" => Ok(Variant::Long(self.files.free_file()? as i32)),
            "timer" => {
                let serial = now_serial();
                let secs = (serial - serial.floor()) * 86_400.0;
                Ok(Variant::Single(secs as f32))
            }

            _ => err(35, &format!("Sub or Function not defined: {}", name)),
        }
    }
}

/// Whether `path`'s file name matches one of the semicolon-separated patterns
/// in `filter` (`*.bas;*.oxproj`), by [`oxbasic_runtime::pattern_matches`].
/// An empty filter accepts any file, and an empty path (a cancelled dialog)
/// is passed through as it is.
fn matches_filter(path: &str, filter: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    let name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    pattern_matches(&name, filter)
}

/// Whether a file name matches a `;`-separated pattern list (`*.bas;*.oxproj`):
/// each is `*.ext` (ends with that) or an exact name, ignoring case. An empty
/// list matches every name. The one rule the file dialogs and file lists share.
pub fn pattern_matches(name: &str, patterns: &str) -> bool {
    let list: Vec<String> = patterns
        .split(';')
        .map(|p| p.trim().to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    if list.is_empty() {
        return true;
    }
    let name = name.to_ascii_lowercase();
    list.iter().any(|p| match p.strip_prefix('*') {
        Some(ext) => name.ends_with(ext),
        None => name == *p,
    })
}

/// VB6 `Format(value, pattern)`: named formats, numeric patterns (`0`, `#`,
/// `,`, `.`, `%`, `E+`) and date/time patterns. A pattern with none of those
/// codes is shown as typed.
fn format_value(v: &Variant, pat: &str) -> ExecResult<Variant> {
    if pat.is_empty() {
        return Ok(Variant::String(to_string(v)?));
    }
    let named = match pat.to_ascii_lowercase().as_str() {
        "general number" => Some(to_string(v)?),
        "currency" => Some(format_numeric(to_f64(v)?, "$#,##0.00")),
        "fixed" => Some(format_numeric(to_f64(v)?, "0.00")),
        "standard" => Some(format_numeric(to_f64(v)?, "#,##0.00")),
        "percent" => Some(format_numeric(to_f64(v)?, "0.00%")),
        "scientific" => Some(format_numeric(to_f64(v)?, "0.00E+00")),
        "yes/no" => Some(if to_bool(v)? { "Yes" } else { "No" }.into()),
        "true/false" => Some(if to_bool(v)? { "True" } else { "False" }.into()),
        "on/off" => Some(if to_bool(v)? { "On" } else { "Off" }.into()),
        _ => None,
    };
    if let Some(s) = named {
        return Ok(Variant::String(s));
    }
    if is_date_pattern(pat) {
        return Ok(Variant::String(format_date_pattern(to_f64(v)?, pat)));
    }
    if is_numeric_pattern(pat) {
        return Ok(Variant::String(format_numeric(to_f64(v)?, pat)));
    }
    Ok(Variant::String(pat.to_string()))
}

/// A date/time pattern has a date or time code: y, m, d, h, n, s, q, w, or AM/PM.
fn is_date_pattern(pat: &str) -> bool {
    let p = pat.to_ascii_lowercase();
    p.contains('y') || p.contains('d') || p.contains('h') || p.contains('n')
        || p.contains('s') || p.contains('q') || p.contains('w') || p.contains('m')
        || p.contains("am/pm") || p.contains("a/p")
}

/// A numeric pattern has digit placeholders.
fn is_numeric_pattern(pat: &str) -> bool {
    pat.contains('0') || pat.contains('#')
}

/// Date/time pattern: `yy`/`yyyy`, `m`..`mmmm`, `d`..`dddd`, `h`, `n`, `s`,
/// `q` (quarter), `w` (weekday number), `AM/PM`, `A/P`, quoted and `\` literals.
fn format_date_pattern(d: f64, pat: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August",
        "September", "October", "November", "December",
    ];
    const DAYS: [&str; 7] = [
        "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
    ];
    let days = d.floor() as i64;
    let frac = d - days as f64;
    let (y, m, dd) = civil_from_days(days);
    let secs = (frac * 86400.0).round() as u64;
    let (hh, mm, ss) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    // weekday 1 = Sunday, as VB's default; 1899-12-30 was a Saturday
    let wd = ((days + 6).rem_euclid(7)) as usize;
    let hour12 = if hh % 12 == 0 { 12 } else { hh % 12 };
    let chars: Vec<char> = pat.chars().collect();
    let run = |i: usize| {
        let c = chars[i].to_ascii_lowercase();
        chars[i..].iter().take_while(|x| x.to_ascii_lowercase() == c).count()
    };
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                out.push(chars[i]);
                i += 1;
            }
            i += 1;
            continue;
        }
        if c == '\\' && i + 1 < chars.len() {
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        let rest: String = chars[i..].iter().take(5).collect::<String>().to_ascii_lowercase();
        if rest.starts_with("am/pm") {
            out.push_str(if hh < 12 { "AM" } else { "PM" });
            i += 5;
            continue;
        }
        if rest.starts_with("a/p") {
            out.push(if hh < 12 { 'A' } else { 'P' });
            i += 3;
            continue;
        }
        let n = run(i);
        match c.to_ascii_lowercase() {
            'y' => {
                if n <= 2 {
                    out.push_str(&format!("{:02}", y.rem_euclid(100)));
                } else {
                    out.push_str(&format!("{:04}", y));
                }
            }
            'm' => out.push_str(&match n {
                1 => m.to_string(),
                2 => format!("{:02}", m),
                3 => MONTHS[(m - 1) as usize][..3].to_string(),
                _ => MONTHS[(m - 1) as usize].to_string(),
            }),
            'd' => out.push_str(&match n {
                1 => dd.to_string(),
                2 => format!("{:02}", dd),
                3 => DAYS[wd][..3].to_string(),
                _ => DAYS[wd].to_string(),
            }),
            'h' => out.push_str(&if n >= 2 { format!("{:02}", hour12) } else { hour12.to_string() }),
            'n' => out.push_str(&if n >= 2 { format!("{:02}", mm) } else { mm.to_string() }),
            's' => out.push_str(&if n >= 2 { format!("{:02}", ss) } else { ss.to_string() }),
            'q' => out.push_str(&(((m - 1) / 3) + 1).to_string()),
            'w' => out.push_str(&(wd + 1).to_string()),
            _ => {
                out.push(c);
                i += 1;
                continue;
            }
        }
        i += n;
    }
    out
}

/// Numeric pattern: `0` is a required digit, `#` an optional one, `,` groups
/// thousands between digit placeholders, `.` is the decimal point, `%`
/// multiplies by 100, and `E+`/`E-` gives scientific notation.
fn format_numeric(x: f64, pat: &str) -> String {
    let pat = pat.split(';').next().unwrap_or(pat);
    let percent = pat.contains('%');
    let x = if percent { x * 100.0 } else { x };
    let suffix = if percent { "%" } else { "" };

    if let Some(e_pos) = pat.to_ascii_lowercase().find('e') {
        let mantissa_pat = &pat[..e_pos];
        let after = &pat[e_pos + 1..];
        let plus = after.starts_with('+');
        let exp_digits = after
            .trim_start_matches(['+', '-'])
            .chars()
            .filter(|c| *c == '0')
            .count()
            .max(1);
        let decimals = mantissa_pat
            .split('.')
            .nth(1)
            .map(|s| s.chars().filter(|c| *c == '0' || *c == '#').count())
            .unwrap_or(0);
        let mut exp = if x == 0.0 { 0 } else { x.abs().log10().floor() as i32 };
        let mut mant = if x == 0.0 { 0.0 } else { x / 10f64.powi(exp) };
        // rounding can push the mantissa to 10: renormalise
        if format!("{:.*}", decimals, mant.abs()).starts_with("10") {
            mant /= 10.0;
            exp += 1;
        }
        let sign = if exp < 0 {
            "-"
        } else if plus {
            "+"
        } else {
            ""
        };
        return format!(
            "{:.*}E{}{:0width$}{}",
            decimals,
            mant,
            sign,
            exp.abs(),
            suffix,
            width = exp_digits
        );
    }

    let (int_pat, dec_pat) = match pat.split_once('.') {
        Some((i, d)) => (i, d),
        None => (pat, ""),
    };
    let decimals = dec_pat.chars().filter(|c| *c == '0' || *c == '#').count();
    let min_int = int_pat.chars().filter(|c| *c == '0').count();
    let grouping = int_pat.contains(',');

    // round to the decimals asked for, then split at the point
    let body = format!("{:.*}", decimals, x.abs());
    let (int_digits, frac) = match body.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (body.clone(), String::new()),
    };
    let mut int_s = int_digits.trim_start_matches('0').to_string();
    while int_s.len() < min_int {
        int_s.insert(0, '0');
    }
    if grouping {
        let mut grouped = String::new();
        let n = int_s.len();
        for (k, ch) in int_s.chars().enumerate() {
            if k > 0 && (n - k) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(ch);
        }
        int_s = grouped;
    }
    let negative = x < 0.0 && body.chars().any(|c| c.is_ascii_digit() && c != '0');
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&int_s);
    if decimals > 0 {
        out.push('.');
        out.push_str(&frac);
    }
    out.push_str(suffix);
    out
}


/// Parse a few common date literal shapes ("2024-05-01", "5/1/2024").
fn parse_date_str(s: &str) -> ExecResult<Variant> {
    let t = s.trim();
    let parts: Vec<&str> = if t.contains('-') {
        t.split('-').collect()
    } else {
        t.split('/').collect()
    };
    if parts.len() == 3 {
        let a: i64 = parts[0].parse().unwrap_or(0);
        let b: u32 = parts[1].parse().unwrap_or(0);
        let c: u32 = parts[2].parse().unwrap_or(0);
        if a > 31 {
            return Ok(Variant::Date(days_from_civil(a, b, c) as f64));
        } else {
            let y: i64 = parts[2].parse().unwrap_or(0);
            return Ok(Variant::Date(days_from_civil(y, a as u32, b) as f64));
        }
    }
    err(
        13,
        &format!("type mismatch: cannot convert \"{}\" to date", s),
    )
}
