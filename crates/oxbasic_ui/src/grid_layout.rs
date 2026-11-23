//! Where a DataGrid's cells and scrollbars go for a given size: which rows
//! and columns are in view, their positions, whether each scrollbar is
//! needed, and where its thumb sits. Pure geometry, so it is tested without a
//! window and the painter only draws what it is given.

use crate::grid::GridSpec;

/// Thickness in pixels of a grid's scrollbars.
pub const BAR: f32 = 16.0;
/// The shortest a scrollbar thumb gets.
pub const MIN_THUMB: f32 = 16.0;

/// One visible row or column: its index, where it starts, and its size.
pub type Slot = (usize, f32, f32);

/// A scrollbar thumb: where it starts along its track and how long it is.
pub type Thumb = (f32, f32);

#[derive(Debug, Clone, PartialEq)]
pub struct GridLayout {
    /// The visible columns and rows. Fixed ones come first and stay put; the
    /// scrolling ones follow from `LeftCol` / `TopRow`, the last possibly cut
    /// off by the edge of the view.
    pub cols: Vec<Slot>,
    pub rows: Vec<Slot>,
    /// The cells' area, inside any scrollbars.
    pub view_w: f32,
    pub view_h: f32,
    /// A vertical / horizontal scrollbar is shown.
    pub vbar: bool,
    pub hbar: bool,
    /// The thumbs, when their bars are shown.
    pub vthumb: Option<Thumb>,
    pub hthumb: Option<Thumb>,
    /// The largest `TopRow` / `LeftCol` that scrolling reaches.
    pub max_top_row: usize,
    pub max_left_col: usize,
}

fn total(sizes: &[f32]) -> f32 {
    sizes.iter().sum()
}

/// The first scrolling index from which everything after it fits in `room`
/// (so scrolling there shows the last item whole), at least `fixed` and at
/// most the last index.
fn max_start(sizes: &[f32], fixed: usize, room: f32) -> usize {
    let mut at = sizes.len();
    let mut used = 0.0;
    while at > fixed && used + sizes[at - 1] <= room {
        at -= 1;
        used += sizes[at];
    }
    at.max(fixed).min(sizes.len().saturating_sub(1))
}

/// The slots visible along one axis: the fixed ones, then from `start` on
/// until `room` is used up.
fn slots(sizes: &[f32], fixed: usize, start: usize, room: f32) -> Vec<Slot> {
    let mut out = Vec::new();
    let mut at = 0.0;
    for i in (0..fixed.min(sizes.len())).chain(start.max(fixed)..sizes.len()) {
        if at >= room {
            break;
        }
        out.push((i, at, sizes[i]));
        at += sizes[i];
    }
    out
}

/// A thumb along a track of `track` pixels: its length is the share of the
/// content in view, its place the share scrolled.
fn thumb(track: f32, view: f32, content: f32, pos: usize, first: usize, last: usize) -> Thumb {
    let len = (track * view / content.max(1.0)).clamp(MIN_THUMB.min(track), track.max(0.0));
    let span = last.saturating_sub(first);
    let frac = if span == 0 {
        0.0
    } else {
        (pos.saturating_sub(first)).min(span) as f32 / span as f32
    };
    (frac * (track - len).max(0.0), len)
}

impl GridLayout {
    /// The layout of `g` in an area `w` x `h` pixels (inside its border).
    pub fn new(g: &GridSpec, w: f32, h: f32) -> GridLayout {
        let content_w = total(&g.col_widths);
        let content_h = total(&g.row_heights);
        let (mut vbar, mut hbar) = (false, false);
        // each bar takes room the other may then need: settle in two passes
        for _ in 0..2 {
            hbar = g.scroll_bars & 1 != 0 && content_w > w - if vbar { BAR } else { 0.0 };
            vbar = g.scroll_bars & 2 != 0 && content_h > h - if hbar { BAR } else { 0.0 };
        }
        let view_w = (w - if vbar { BAR } else { 0.0 }).max(0.0);
        let view_h = (h - if hbar { BAR } else { 0.0 }).max(0.0);
        let fixed_w = total(&g.col_widths[..g.fixed_cols.min(g.cols)]);
        let fixed_h = total(&g.row_heights[..g.fixed_rows.min(g.rows)]);
        let max_left_col = max_start(&g.col_widths, g.fixed_cols, view_w - fixed_w);
        let max_top_row = max_start(&g.row_heights, g.fixed_rows, view_h - fixed_h);
        let left = g.left_col.min(max_left_col);
        let top = g.top_row.min(max_top_row);
        GridLayout {
            cols: slots(&g.col_widths, g.fixed_cols, left, view_w),
            rows: slots(&g.row_heights, g.fixed_rows, top, view_h),
            view_w,
            view_h,
            vbar,
            hbar,
            vthumb: vbar.then(|| thumb(view_h, view_h, content_h, top, g.fixed_rows, max_top_row)),
            hthumb: hbar.then(|| thumb(view_w, view_w, content_w, left, g.fixed_cols, max_left_col)),
            max_top_row,
            max_left_col,
        }
    }

    /// The cell under a point of the cells' area, as (row, col).
    pub fn cell_at(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let find = |v: &[Slot], p: f32| {
            v.iter().find(|(_, at, size)| p >= *at && p < at + size).map(|s| s.0)
        };
        if x >= self.view_w || y >= self.view_h {
            return None;
        }
        Some((find(&self.rows, y)?, find(&self.cols, x)?))
    }
}

/// The area a grid's cells use inside its control's `rect`: less the border.
pub fn inside(r: &crate::Rect) -> (f32, f32) {
    ((r.width.max(4.0) - 2.0).max(0.0), (r.height.max(4.0) - 2.0).max(0.0))
}

/// The first scrolling index to show so that `target` is in view whole, on
/// one axis: `sizes` of every item, the first `fixed` of which never scroll,
/// `start` the index now first, `room` the length available. A fixed item is
/// always in view. Scrolls the least: up to `target` when it is before
/// `start`, else down just far enough, and never past the last page.
pub fn reveal_axis(sizes: &[f32], fixed: usize, start: usize, room: f32, target: usize) -> usize {
    let fixed_len: f32 = sizes[..fixed.min(sizes.len())].iter().sum();
    let last = max_start(sizes, fixed, room - fixed_len);
    let start = start.max(fixed).min(last);
    if target < fixed {
        return start;
    }
    if target < start {
        return target.min(last);
    }
    // the length from `start` through `target`
    let mut first = start;
    while first < target.min(last) {
        let used: f32 = sizes[first..=target.min(sizes.len() - 1)].iter().sum();
        if fixed_len + used <= room + 0.01 {
            break;
        }
        first += 1;
    }
    first
}

/// Scroll `g` the least that brings its current cell into view in an area
/// `w` x `h` (the grid's inside, less its border), as moving by key or setting
/// `Row` / `Col` does. Returns whether `TopRow` or `LeftCol` moved.
pub fn reveal(g: &mut GridSpec, w: f32, h: f32) -> bool {
    let (top, left) = (g.top_row, g.left_col);
    // the bars take room, and which are needed depends on the content, not on
    // where the view is: lay out once to learn the view's size
    let l = GridLayout::new(g, w, h);
    g.top_row = reveal_axis(&g.row_heights, g.fixed_rows, g.top_row, l.view_h, g.row);
    g.left_col = reveal_axis(&g.col_widths, g.fixed_cols, g.left_col, l.view_w, g.col);
    (g.top_row, g.left_col) != (top, left)
}
