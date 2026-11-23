//! The run-time properties and methods of a DataGrid (`Rows`, `Cols`,
//! `TextMatrix(r, c)`, `Row`, `Col`, `Text`, `ColWidth(c)`, `AddItem`...),
//! as MSFlexGrid has them. They read and write the control's `GridSpec`;
//! drawing and the user's editing are elsewhere.

use super::host::{err, FormHost};
use oxbasic_runtime::{ExecResult, Variant};

/// The most rows or columns a grid may have: a guard against a typo such as
/// `Rows = 1000000000` allocating the machine away.
pub const MAX_GRID_SIDE: i64 = 100_000;

fn unsupported<T>() -> ExecResult<T> {
    err(438, "object doesn't support this property or method")
}

/// Argument `n` as an index, 381 when missing or negative.
fn arg(args: &[Variant], n: usize) -> ExecResult<usize> {
    let Some(v) = args.get(n) else {
        return err(450, "wrong number of arguments");
    };
    let k = v.as_i64()?;
    if k < 0 {
        return err(381, "Invalid property array index");
    }
    Ok(k as usize)
}

impl FormHost {
    /// A DataGrid property read. `Err(438)` for a name it doesn't have.
    pub(crate) fn get_grid(&self, i: usize, p: &str, args: &[Variant]) -> ExecResult<Variant> {
        let g = &self.form.controls[i].grid;
        Ok(match p {
            "rows" => Variant::Long(g.rows as i32),
            "cols" => Variant::Long(g.cols as i32),
            "fixedrows" => Variant::Long(g.fixed_rows as i32),
            "fixedcols" => Variant::Long(g.fixed_cols as i32),
            "editable" => Variant::Boolean(g.editable),
            "row" => Variant::Long(g.row as i32),
            "col" => Variant::Long(g.col as i32),
            "toprow" => Variant::Long(g.top_row as i32),
            "leftcol" => Variant::Long(g.left_col as i32),
            "scrollbars" => Variant::Integer(g.scroll_bars as i16),
            "text" => Variant::String(g.cell(g.row, g.col).unwrap_or("").to_string()),
            "textmatrix" => {
                let (r, c) = (arg(args, 0)?, arg(args, 1)?);
                match g.cell(r, c) {
                    Some(t) => Variant::String(t.to_string()),
                    None => return err(381, "Subscript out of range"),
                }
            }
            "colwidth" => match g.col_width(arg(args, 0)?) {
                Some(w) => Variant::Single(w),
                None => return err(381, "Subscript out of range"),
            },
            "rowheight" => match g.row_height(arg(args, 0)?) {
                Some(h) => Variant::Single(h),
                None => return err(381, "Subscript out of range"),
            },
            _ => return unsupported(),
        })
    }
}

impl FormHost {
    /// A DataGrid property assignment.
    pub(crate) fn set_grid(
        &mut self,
        i: usize,
        p: &str,
        args: &[Variant],
        value: &Variant,
    ) -> ExecResult<()> {
        let g = &mut self.form.controls[i].grid;
        match p {
            "rows" | "cols" => {
                let n = value.as_i64()?;
                if !(0..=MAX_GRID_SIDE).contains(&n) {
                    return err(380, "Invalid property value");
                }
                let (r, c) = if p == "rows" { (n as usize, g.cols) } else { (g.rows, n as usize) };
                g.resize(r, c);
            }
            "fixedrows" | "fixedcols" => {
                let n = value.as_i64()?;
                let ok = n >= 0
                    && if p == "fixedrows" {
                        g.set_fixed_rows(n as usize)
                    } else {
                        g.set_fixed_cols(n as usize)
                    };
                if !ok {
                    return err(380, "Invalid property value");
                }
            }
            "editable" => g.editable = value.as_bool()?,
            "row" | "col" => {
                let n = value.as_i64()?;
                let (r, c) = if p == "row" { (n, g.col as i64) } else { (g.row as i64, n) };
                if r < 0 || c < 0 || !g.select(r as usize, c as usize) {
                    return err(381, "Subscript out of range");
                }
            }
            "toprow" | "leftcol" => {
                let n = value.as_i64()?;
                let limit = if p == "toprow" { g.rows } else { g.cols } as i64;
                if n < 0 || n >= limit.max(1) {
                    return err(381, "Subscript out of range");
                }
                if p == "toprow" {
                    g.top_row = n as usize;
                } else {
                    g.left_col = n as usize;
                }
                g.clamp_view();
            }
            "scrollbars" => {
                let n = value.as_i64()?;
                if !(0..=3).contains(&n) {
                    return err(380, "Invalid property value");
                }
                g.scroll_bars = n as u8;
            }
            "text" => {
                let (r, c) = (g.row, g.col);
                g.set_cell(r, c, &value.as_string()?);
            }
            "textmatrix" => {
                let (r, c) = (arg(args, 0)?, arg(args, 1)?);
                if !g.set_cell(r, c, &value.as_string()?) {
                    return err(381, "Subscript out of range");
                }
            }
            "colwidth" | "rowheight" => {
                let n = arg(args, 0)?;
                let size = value.as_f64()? as f32;
                let ok = if p == "colwidth" {
                    g.set_col_width(n, size)
                } else {
                    g.set_row_height(n, size)
                };
                if !ok {
                    return err(381, "Subscript out of range");
                }
            }
            _ => return unsupported(),
        }
        Ok(())
    }

    /// A DataGrid method: `AddItem "a" & vbTab & "b"`, `RemoveItem r`, `Clear`.
    pub(crate) fn call_grid(&mut self, i: usize, m: &str, args: &[Variant]) -> ExecResult<Variant> {
        let g = &mut self.form.controls[i].grid;
        match m {
            "additem" => {
                let Some(item) = args.first() else {
                    return err(450, "wrong number of arguments");
                };
                if g.rows as i64 >= MAX_GRID_SIDE {
                    return err(380, "Invalid property value");
                }
                g.add_item(&item.as_string()?);
            }
            "removeitem" => {
                if !g.remove_item(arg(args, 0)?) {
                    return err(381, "Subscript out of range");
                }
            }
            "clear" => g.clear(),
            _ => return unsupported(),
        }
        Ok(Variant::Empty)
    }
}
