//! Painting a DataGrid: the visible cells (from [`GridLayout`]), grey header
//! cells, the current cell's highlight, and the scrollbars. Only the cells
//! in view are made into elements, so a grid of thousands of rows stays fast.

use crate::gpui_controls::ControlLook;
use crate::grid_layout::{GridLayout, BAR};
use crate::ControlSpec;
use gpui::{div, prelude::*, px, rgb, Div, FontWeight, SharedString, Stateful};

const LINE: u32 = 0xD4D4D8;
const HEADER_BACK: u32 = 0xF0F0F2;
const HEADER_CUR: u32 = 0xDCEEFF;
const CELL_BACK: u32 = 0xFFFFFF;
const CURRENT: u32 = 0x007AFF;
const BAR_TRACK: u32 = 0xF6F6F8;
const BAR_THUMB: u32 = 0xB4B4BA;
const BORDER: f32 = 1.0;

/// The size of the area the cells use: the control's, less its border.
pub fn inner_size(c: &ControlSpec) -> (f32, f32) {
    (
        (c.rect.width.max(4.0) - 2.0 * BORDER).max(0.0),
        (c.rect.height.max(4.0) - 2.0 * BORDER).max(0.0),
    )
}

/// One cell: its text clipped to its box, a gridline right and below it.
fn cell(
    text: &str,
    (x, y, w, h): (f32, f32, f32, f32),
    fixed: bool,
    header_current: bool,
) -> Div {
    let mut d = div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(w))
        .h(px(h))
        .px_1()
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap()
        .border_r_1()
        .border_b_1()
        .border_color(rgb(LINE))
        .bg(rgb(if header_current {
            HEADER_CUR
        } else if fixed {
            HEADER_BACK
        } else {
            CELL_BACK
        }))
        .child(SharedString::from(text.to_string()));
    if fixed {
        d = d.font_weight(FontWeight::MEDIUM);
    }
    d
}

/// A scrollbar: a track with its thumb at `thumb` = (start, length).
fn bar(vertical: bool, track: f32, thumb: (f32, f32)) -> Div {
    let t = div().absolute().bg(rgb(BAR_TRACK));
    let t = if vertical {
        t.w(px(BAR)).h(px(track))
    } else {
        t.h(px(BAR)).w(px(track))
    };
    let th = div().absolute().bg(rgb(BAR_THUMB)).rounded_sm();
    let th = if vertical {
        th.left(px(3.0)).w(px(BAR - 6.0)).top(px(thumb.0)).h(px(thumb.1))
    } else {
        th.top(px(3.0)).h(px(BAR - 6.0)).left(px(thumb.0)).w(px(thumb.1))
    };
    t.child(th)
}

/// The grid inside `base` (the control's own box, already placed and sized).
pub fn paint_grid(base: Stateful<Div>, c: &ControlSpec, look: &ControlLook) -> Stateful<Div> {
    let g = &c.grid;
    let (w, h) = inner_size(c);
    let lay = GridLayout::new(g, w, h);
    let mut view = div()
        .absolute()
        .left_0()
        .top_0()
        .w(px(lay.view_w))
        .h(px(lay.view_h))
        .overflow_hidden();
    for &(r, y, rh) in &lay.rows {
        for &(col, x, cw) in &lay.cols {
            let fixed = g.is_fixed(r, col);
            // the header of the current row and column is tinted
            let tint = fixed && (r == g.row || col == g.col) && !(r < g.fixed_rows && col < g.fixed_cols);
            let text = g.cell(r, col).unwrap_or("");
            let mut el = cell(text, (x, y, cw, rh), fixed, tint)
                .debug_selector(|| format!("{}-cell-{}-{}", c.name, r, col));
            if !fixed && r == g.row && col == g.col {
                // the current cell: an accent frame, strong when focused
                el = el
                    .border_1()
                    .border_color(rgb(if look.focused || look.design { CURRENT } else { LINE }));
            }
            view = view.child(el);
        }
    }
    let mut out = base
        .debug_selector(|| format!("{}-grid", c.name))
        .bg(rgb(CELL_BACK))
        .border_1()
        .border_color(rgb(if look.focused { CURRENT } else { 0xB4B4BA }))
        .overflow_hidden()
        .child(view);
    if let Some(t) = lay.vthumb {
        out = out.child(
            bar(true, lay.view_h, t)
                .left(px(lay.view_w))
                .top_0()
                .debug_selector(|| format!("{}-vbar", c.name)),
        );
    }
    if let Some(t) = lay.hthumb {
        out = out.child(
            bar(false, lay.view_w, t)
                .top(px(lay.view_h))
                .left_0()
                .debug_selector(|| format!("{}-hbar", c.name)),
        );
    }
    if lay.vbar && lay.hbar {
        // the square where the two bars meet
        out = out.child(
            div()
                .absolute()
                .left(px(lay.view_w))
                .top(px(lay.view_h))
                .w(px(BAR))
                .h(px(BAR))
                .bg(rgb(BAR_TRACK)),
        );
    }
    out
}
