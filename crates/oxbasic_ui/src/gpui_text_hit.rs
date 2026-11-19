//! Where in a text field a click or drag lands: the character position
//! under a point, measured with the real font the field is drawn in. The
//! window turns a mouse position into a character index with this, then
//! the `ControlState`'s caret and selection do the rest.

use crate::{ControlKind, ControlSpec, DEFAULT_FONT_SIZE};
use gpui::{px, Font, FontStyle, FontWeight, Pixels, Point, SharedString, TextRun, Window};

/// Padding and border on each side of a text field's text.
const INSET_X: f32 = 5.0;
const INSET_Y: f32 = 3.0;
/// One row of a MultiLine box (the field's `min_h`).
const ROW: f32 = 16.0;

/// The font a text control is drawn in: the window's own text style, with
/// the control's `Font` over it (the field inherits the rest).
fn font_of(window: &Window, c: &ControlSpec) -> (Font, Pixels) {
    let base = window.text_style();
    let f = &c.font;
    let size = if f.size != DEFAULT_FONT_SIZE && f.size > 0.0 {
        px(f.size * 96.0 / 72.0)
    } else {
        base.font_size.to_pixels(window.rem_size())
    };
    let mut font = base.font();
    if !f.name.is_empty() {
        font.family = SharedString::from(f.name.clone());
    }
    if f.bold {
        font.weight = FontWeight::BOLD;
    }
    if f.italic {
        font.style = FontStyle::Italic;
    }
    (font, size)
}

/// The character (not byte) index in `text` whose left edge is nearest `x`
/// pixels from the text's start.
pub fn index_at_x(window: &Window, c: &ControlSpec, text: &str, x: f32) -> usize {
    if text.is_empty() {
        return 0;
    }
    let (font, size) = font_of(window, c);
    let run = TextRun {
        len: text.len(),
        font,
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line =
        window
            .text_system()
            .shape_line(SharedString::from(text.to_string()), size, &[run], None);
    let byte = line.closest_index_for_x(px(x.max(0.0)));
    text[..byte.min(text.len())].chars().count()
}

/// The character index in `state_text` that point `at` (the control's own
/// coordinates: from its top-left) lands on, for a one-line field or, with
/// `c.multi_line`, a box of lines.
pub fn index_at(window: &Window, c: &ControlSpec, text: &str, at: Point<Pixels>) -> usize {
    let x = f32::from(at.x) - INSET_X;
    if !(c.multi_line && c.kind == ControlKind::TextBox) {
        return index_at_x(window, c, text, x);
    }
    let row = (((f32::from(at.y) - INSET_Y) / ROW).floor().max(0.0)) as usize;
    let lines: Vec<&str> = text.split('\n').collect();
    let row = row.min(lines.len() - 1);
    // characters before this line, each earlier line's break included
    let before: usize = lines[..row].iter().map(|l| l.chars().count() + 1).sum();
    before + index_at_x(window, c, lines[row], x)
}
