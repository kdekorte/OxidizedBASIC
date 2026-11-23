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
            "text" => Variant::String(match &g.pending_text {
                Some(p) => p.clone(),
                None => g.cell(g.row, g.col).unwrap_or("").to_string(),
            }),
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
                // the current cell is kept in view, as when the user moves
                let (w, h) = crate::grid_layout::inside(&self.form.controls[i].rect);
                crate::grid_layout::reveal(&mut self.form.controls[i].grid, w, h);
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

impl FormHost {
    /// Queue `event` of control `i` with the grid's `Row` and `Col` as its
    /// arguments (`AfterEdit(Row, Col)`), after its `Index` in a control array.
    fn raise_cell(&mut self, i: usize, event: &str, row: usize, col: usize) {
        let c = &self.form.controls[i];
        let mut args: Vec<Variant> = c
            .index
            .map(|n| vec![Variant::Integer(n as i16)])
            .unwrap_or_default();
        args.push(Variant::Long(row as i32));
        args.push(Variant::Long(col as i32));
        self.raised.push(oxbasic_runtime::RaisedEvent {
            form: self.form.name.clone(),
            handler: format!("{}_{}", c.name, event),
            args,
        });
    }

    /// The grid `control` as a mutable spec, when it is a DataGrid.
    pub(crate) fn grid_mut(&mut self, control: usize) -> Option<&mut crate::grid::GridSpec> {
        let c = self.form.controls.get_mut(control)?;
        (c.kind == crate::ControlKind::DataGrid).then_some(&mut c.grid)
    }

    /// The user made `(row, col)` the current cell. Ignored outside the grid
    /// and for a header; the events follow the move (`raise_changes`).
    pub(crate) fn grid_select(&mut self, control: usize, row: usize, col: usize) {
        let Some(g) = self.grid_mut(control) else {
            return;
        };
        if !g.contains(row, col) || g.is_fixed(row, col) {
            return;
        }
        let before = self.watched(control);
        let (w, h) = crate::grid_layout::inside(&self.form.controls[control].rect);
        let g = &mut self.form.controls[control].grid;
        g.select(row, col);
        crate::grid_layout::reveal(g, w, h);
        self.changed = true;
        self.touched[control] = true;
        self.raise_changes(control, &before);
    }

    /// The user scrolled the grid to `top_row` / `left_col` (kept on a
    /// scrolling row and column); `Scroll` runs when the view moved.
    pub(crate) fn grid_view(&mut self, control: usize, top_row: usize, left_col: usize) {
        if self.grid_mut(control).is_none() {
            return;
        }
        let before = self.watched(control);
        let g = &mut self.form.controls[control].grid;
        g.top_row = top_row;
        g.left_col = left_col;
        g.clamp_view();
        self.changed = true;
        self.touched[control] = true;
        self.raise_changes(control, &before);
    }

    /// The user finished editing `(row, col)` with `text`. Refused for a cell
    /// that can't be edited (a read-only grid, a header, outside). Returns
    /// whether the text was kept; `AfterEdit` runs when it changed.
    /// The text typed into the current cell changed (or the edit ended with
    /// `None`): it is what the grid shows and what `Text` reads, and `Change`
    /// runs for each change so the program can follow the typing.
    pub(crate) fn grid_editing(&mut self, control: usize, text: Option<String>) {
        let Some(g) = self.grid_mut(control) else {
            return;
        };
        let changed = g.pending_text != text;
        let typing = text.is_some();
        g.pending_text = text;
        if changed {
            self.changed = true;
            self.touched[control] = true;
            if typing {
                self.raise(control, "Change");
            }
        }
    }

    pub(crate) fn grid_cell(&mut self, control: usize, row: usize, col: usize, text: String) -> bool {
        let Some(g) = self.grid_mut(control) else {
            return false;
        };
        if !g.can_edit(row, col) {
            return false;
        }
        let changed = g.cell(row, col) != Some(text.as_str());
        g.pending_text = None;
        g.set_cell(row, col, &text);
        self.changed = true;
        self.touched[control] = true;
        if changed {
            self.raise_cell(control, "AfterEdit", row, col);
        }
        true
    }
}
