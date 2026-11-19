//! Standard library: string, math, date, conversion, and array functions
//! for the OxidizedBASIC runtime (Phase 1).

use crate::*;

/// Names recognized as builtins (used to disambiguate array indexing).
pub(crate) fn is_builtin(name: &str) -> bool {
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
                    },
                ))))
            }
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
                        Ok(Variant::Long(b.data.len() as i32 - 1))
                    } else {
                        Ok(Variant::Long(0))
                    };
                }
                let ix = (dim - 1) as usize;
                if ix >= b.dims.len() {
                    return err(9, "subscript out of range");
                }
                Ok(Variant::Long(if name == "ubound" {
                    b.dims[ix] as i32 - 1
                } else {
                    0
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
            "beep" | "doevents" => Ok(Variant::Empty),

            "format" => {
                let v = arg(args, 0)?.clone();
                let pat = if args.len() > 1 {
                    want_str(args, 1)?
                } else {
                    String::new()
                };
                format_value(&v, &pat)
            }

            _ => err(35, &format!("Sub or Function not defined: {}", name)),
        }
    }
}

/// Minimal `Format` implementation: date tokens (yyyy/mm/dd/hh/nn/ss) and
/// numeric patterns like "0.00".
fn format_value(v: &Variant, pat: &str) -> ExecResult<Variant> {
    let p = pat.to_ascii_lowercase();
    if p.contains("yyyy") || p.contains("mm") || p.contains("dd") || p.contains("nn") {
        let d = to_f64(v)?;
        let days = d.floor() as i64;
        let frac = d - days as f64;
        let (y, m, dd) = civil_from_days(days);
        let secs = (frac * 86400.0).round() as u64;
        let (hh, mm, ss) = (secs / 3600, (secs % 3600) / 60, secs % 60);
        let mut out = String::new();
        let mut chars = pat.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                'y' | 'Y' => {
                    let mut n = 1;
                    while matches!(chars.peek(), Some('y') | Some('Y')) {
                        chars.next();
                        n += 1;
                    }
                    out.push_str(&format!("{:0width$}", y, width = n.min(4)));
                }
                'm' | 'M' => {
                    let mut n = 1;
                    while matches!(chars.peek(), Some('m') | Some('M')) {
                        chars.next();
                        n += 1;
                    }
                    out.push_str(&format!("{:0width$}", m, width = n.min(2)));
                }
                'd' | 'D' => {
                    let mut n = 1;
                    while matches!(chars.peek(), Some('d') | Some('D')) {
                        chars.next();
                        n += 1;
                    }
                    out.push_str(&format!("{:0width$}", dd, width = n.min(2)));
                }
                'h' | 'H' => out.push_str(&format!("{:02}", hh)),
                'n' | 'N' => out.push_str(&format!("{:02}", mm)),
                's' | 'S' => out.push_str(&format!("{:02}", ss)),
                other => out.push(other),
            }
        }
        Ok(Variant::String(out))
    } else if p.contains('0') || p.contains('#') || p.contains('.') {
        let x = to_f64(v)?;
        let decimals = p
            .split('.')
            .nth(1)
            .map(|s| s.chars().filter(|c| *c == '0' || *c == '#').count())
            .unwrap_or(0);
        Ok(Variant::String(format!("{:.*}", decimals, x)))
    } else {
        Ok(Variant::String(to_string(v)?))
    }
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
