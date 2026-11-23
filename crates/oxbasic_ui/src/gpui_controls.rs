//! Shared GPUI drawing of the classic VB6 controls, used by the IDE's form
//! designer, the IDE's run window and the standalone `oxbasic gui` window,
//! so a control looks (and, at run time, behaves) the same everywhere:
//!
//! | Control | Looks like |
//! |---------|------------|
//! | `Label` | bare text — no border, no box |
//! | `CommandButton` | a push button with a centred caption |
//! | `TextBox` | a white single-line field |
//! | `Frame` | a group box: thin outline with the caption set into its top edge |
//! | `ListBox` | a white scrolling list, the selected item highlighted |
//! | `ComboBox` | a field with a ▾ button that drops a pick list (`Style` 0 and 2), or a field above an always-visible list (`Style` 1) |
//! | `PictureBox` | a plain bordered canvas |
//! | `Timer` | invisible at run time; a small ◷ icon in the designer |

use crate::{ControlKind, ControlSpec, FormSpec, DEFAULT_FONT_SIZE};
use gpui::{
    deferred, div, img, prelude::*, px, relative, rgb, AnyElement, Context, CursorStyle, Div,
    FontWeight, Keystroke, ObjectFit, SharedString, Stateful,
};

pub use crate::FORM_BACK;
const TEXT: u32 = 0x1D1D1F;
const DIM: u32 = 0x8E8E93;
const ACCENT: u32 = 0x007AFF;
const BUTTON_FACE: u32 = 0xF6F6F8;
const BUTTON_HOVER: u32 = 0xE8E8ED;
const BUTTON_ACTIVE: u32 = 0xDCEEFF;
const BUTTON_BORDER: u32 = 0xB4B4BA;
const FIELD_BORDER: u32 = 0x8E8E93;
const ROW_HOVER: u32 = 0xE8F1FD;
/// Height of one list row.
const ROW_H: f32 = 20.0;
/// Width of a combo box's ▾ button.
const ARROW_W: f32 = 20.0;

/// The interactive pieces of a control; a caller attaches its event
/// handlers to them through the `hook` of [`paint_control`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The whole control (a button, a text field, a label, a combo box...).
    Body,
    /// A combo box's text field (the part left of the ▾ button).
    Field,
    /// A combo box's ▾ button.
    Arrow,
    /// Row `i` of a list box, or of a Simple combo's list.
    ListItem(usize),
    /// Row `i` of a combo box's dropped-down list.
    DropItem(usize),
}

/// What changes how a control is drawn.
#[derive(Clone, Debug, Default)]
pub struct ControlLook {
    /// Drawn in the designer: no hover effects, placeholders for empty lists.
    pub design: bool,
    /// Current text of a TextBox / ComboBox.
    pub text: String,
    /// Selected row of a ListBox / ComboBox.
    pub list_index: Option<usize>,
    /// A combo box's list is open.
    pub dropped: bool,
    /// The control has the keyboard focus.
    pub focused: bool,
    /// `Value` of a CheckBox (0, 1, 2) or OptionButton (0, 1).
    pub value: i32,
    /// The selected tab of a TabControl (`Tab`).
    pub tab: u8,
    /// Where the control is on the form (its `Left`/`Top` plus those of the
    /// containers around it); `None` is its own `Left`/`Top`.
    pub origin: Option<(f32, f32)>,
    /// The form's size, so a Shape or Line is drawn no further than the form.
    pub form_size: Option<(f32, f32)>,
    /// The caret and the other end of the selection in a text field, when
    /// it has the focus.
    pub caret: usize,
    pub anchor: Option<usize>,
    /// The rows a MultiSelect ListBox has selected.
    pub selected: Vec<usize>,
    /// What the program drew on a PictureBox.
    pub canvas: crate::canvas::Canvas,
    /// The folder a `Picture` file name is relative to.
    pub base_dir: Option<std::path::PathBuf>,
}

impl ControlLook {
    /// The designer's static view of a control.
    pub fn design(c: &ControlSpec) -> ControlLook {
        ControlLook {
            design: true,
            text: c.text().to_string(),
            value: c.value,
            ..Default::default()
        }
    }

    /// How a running control looks: its state, at `origin` on the form.
    pub fn running(state: &ControlState, origin: (f32, f32), focused: bool) -> ControlLook {
        ControlLook {
            design: false,
            text: state.text.clone(),
            list_index: state.list_index,
            dropped: state.dropped,
            focused,
            value: state.value,
            tab: state.tab,
            origin: Some(origin),
            form_size: None,
            caret: state.caret,
            anchor: state.anchor,
            selected: state.selected.clone(),
            canvas: state.canvas.clone(),
            base_dir: None,
        }
    }
}

/// VB's `MousePointer` as a GPUI cursor (`None`: the control's usual one).
/// `vbHourglass` and `vbArrowHourglass` are macOS's waiting cursor, which
/// GPUI has no style for; [`crate::mac_cursor`] puts it behind
/// `CursorStyle::DragCopy`, and the arrow is shown where that isn't there.
pub fn cursor_for(pointer: i16) -> Option<CursorStyle> {
    Some(match pointer {
        1 => CursorStyle::Arrow,
        2 => CursorStyle::Crosshair,
        3 => CursorStyle::IBeam,
        // VB's size pointers: the four-way and each axis
        5 | 15 => CursorStyle::ClosedHand,
        6 => CursorStyle::ResizeUpRightDownLeft,
        7 => CursorStyle::ResizeUpDown,
        8 => CursorStyle::ResizeUpLeftDownRight,
        9 => CursorStyle::ResizeLeftRight,
        10 => CursorStyle::ResizeUp,
        11 | 13 => {
            if crate::mac_cursor::install_wait_cursor() {
                CursorStyle::DragCopy
            } else {
                CursorStyle::Arrow
            }
        }
        12 => CursorStyle::OperationNotAllowed,
        14 => CursorStyle::ContextualMenu,
        99 => CursorStyle::PointingHand,
        _ => return None,
    })
}

/// The text of a tooltip: a small yellow box (VB's ToolTipText).
pub struct Tip(pub SharedString);

impl Render for Tip {
    fn render(&mut self, _w: &mut gpui::Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "tooltip".into())
            .px_1()
            .py(px(1.0))
            .bg(rgb(0xFFFFE1))
            .border_1()
            .border_color(rgb(0x646464))
            .text_size(px(12.0))
            .text_color(rgb(TEXT))
            .child(self.0.clone())
    }
}

pub use crate::host::ControlState;

/// The text a keystroke types, if any: the platform's `key_char` (so
/// shift-' types `"` and shift-9 `(`), never a control character.
pub fn typed_text(ks: &Keystroke) -> Option<String> {
    if ks.modifiers.platform || ks.modifiers.control {
        return None;
    }
    let s = match &ks.key_char {
        Some(s) => s.clone(),
        None if ks.key == "space" => " ".to_string(),
        None if ks.key.chars().count() == 1 => {
            if ks.modifiers.shift {
                ks.key.to_uppercase()
            } else {
                ks.key.clone()
            }
        }
        None => return None,
    };
    (!s.is_empty() && !s.chars().any(|c| c.is_control())).then_some(s)
}

/// Apply a keystroke to single-line text. Returns whether it changed.
pub fn edit_text(text: &mut String, ks: &Keystroke) -> bool {
    if ks.modifiers.platform || ks.modifiers.control {
        return false;
    }
    if ks.key == "backspace" {
        return text.pop().is_some();
    }
    match typed_text(ks) {
        Some(s) => {
            text.push_str(&s);
            true
        }
        None => false,
    }
}

/// Drawing order: the controls in their order (`ZOrder` moves a control to
/// the end, on top, or the start), except that a container is drawn before
/// what it contains, so what is in a Frame is never hidden by it. Frames on
/// the form are drawn first, as VB's designer puts them behind the rest.
/// The width of one tab caption in a TabControl's strip, in pixels: the
/// caption's characters at the strip's text size, plus padding. Drawing and
/// the click that picks a tab both use this, so they always agree.
pub const TAB_CHAR_W: f32 = 8.0;
pub const TAB_PAD: f32 = 16.0;
/// The height of a TabControl's tab strip, in pixels.
pub const TAB_STRIP_H: f32 = 24.0;

pub fn tab_width(caption: &str) -> f32 {
    caption.chars().count() as f32 * TAB_CHAR_W + TAB_PAD
}

/// The tab a press at `x` (relative to the control's left edge) lands on, or
/// `None` when it is past the last tab. `captions` are the tabs' captions in
/// order.
pub fn tab_at(captions: &[String], x: f32) -> Option<usize> {
    if x < 0.0 {
        return None;
    }
    let mut left = 0.0;
    for (i, c) in captions.iter().enumerate() {
        let right = left + tab_width(c);
        if x < right {
            return Some(i);
        }
        left = right;
    }
    None
}

pub fn z_order(form: &FormSpec) -> Vec<usize> {
    let mut out = Vec::with_capacity(form.controls.len());
    // the form's own controls, frames first, then each one's contents
    let mut roots: Vec<usize> = (0..form.controls.len())
        .filter(|&i| form.container_of(i).is_none())
        .collect();
    roots.sort_by_key(|&i| form.controls[i].kind != ControlKind::Frame);
    fn add(form: &FormSpec, i: usize, out: &mut Vec<usize>) {
        if out.contains(&i) {
            return; // a loop in a hand-edited file
        }
        out.push(i);
        for j in 0..form.controls.len() {
            if form.container_of(j) == Some(i) {
                add(form, j, out);
            }
        }
    }
    for i in roots {
        add(form, i, &mut out);
    }
    // anything not reached (a container loop) still gets drawn
    for i in 0..form.controls.len() {
        if !out.contains(&i) {
            out.push(i);
        }
    }
    out
}

fn caret() -> Div {
    div().flex_none().w(px(1.5)).h(px(14.0)).bg(rgb(ACCENT))
}

/// A text field showing `text`: one line, or wrapped lines for a
/// MultiLine TextBox (scrolling when it has `ScrollBars`). With the focus
/// it shows the caret, and the selection (`SelStart`/`SelLength`) tinted.
fn text_field(base: Stateful<Div>, c: &ControlSpec, look: &ControlLook) -> Stateful<Div> {
    let multi = c.multi_line && c.kind == ControlKind::TextBox;
    let mut field = base
        .px_1()
        .bg(rgb(0xFFFFFF))
        .border_1()
        .border_color(rgb(if look.focused { ACCENT } else { FIELD_BORDER }));
    field = if multi {
        // ScrollBars: 1 horizontal (no wrapping), 2 vertical, 3 both
        let field = field.flex().flex_col().items_start();
        match c.scroll_bars {
            1 => field.overflow_x_scroll().whitespace_nowrap(),
            2 => field.overflow_y_scroll().whitespace_normal(),
            3 => field.overflow_scroll().whitespace_nowrap(),
            _ => field.overflow_hidden().whitespace_normal(),
        }
    } else {
        field
            .flex()
            .items_center()
            .overflow_hidden()
            .whitespace_nowrap()
    };
    if !look.focused || look.design {
        return if multi {
            field.children(look.text.split('\n').map(|l| {
                div()
                    .min_h(px(16.0))
                    .child(SharedString::from(l.to_string()))
            }))
        } else {
            field.child(SharedString::from(look.text.clone()))
        };
    }
    // the text in three runs around the caret: before the selection, the
    // selection (tinted), after it; the caret at its end
    let chars: Vec<char> = look.text.chars().collect();
    let caret = look.caret.min(chars.len());
    let (from, to) = match look.anchor.map(|a| a.min(chars.len())) {
        Some(a) if a != caret => (a.min(caret), a.max(caret)),
        _ => (caret, caret),
    };
    if multi {
        // one row per line, the caret and tint on the rows they fall in
        let mut rows = Vec::new();
        let mut start = 0;
        for line in look.text.split('\n') {
            let end = start + line.chars().count();
            rows.push(text_row(&chars, start, end, from, to, caret));
            start = end + 1;
        }
        return field.children(rows);
    }
    field.child(text_row(&chars, 0, chars.len(), from, to, caret))
}

/// Characters `start..end` of `chars` as one row: the part in `from..to`
/// tinted as selected, and the caret where it is in the row.
fn text_row(chars: &[char], start: usize, end: usize, from: usize, to: usize, at: usize) -> Div {
    let piece = |a: usize, b: usize| -> String { chars[a..b].iter().collect() };
    let (sel_a, sel_b) = (from.clamp(start, end), to.clamp(start, end));
    let mut row = div()
        .flex()
        .items_center()
        .min_h(px(16.0))
        .whitespace_nowrap();
    let before = piece(start, sel_a);
    let sel = piece(sel_a, sel_b);
    let after = piece(sel_b, end);
    let caret_here = at >= start && at <= end;
    // the caret sits at the selection's start or its end (or alone)
    let caret_at_start = caret_here && at == sel_a && sel_a != sel_b;
    row = row.child(SharedString::from(before));
    if caret_at_start {
        row = row.child(caret());
    }
    if !sel.is_empty() {
        row = row.child(div().bg(rgb(0xB4D7FF)).child(SharedString::from(sel)));
    }
    if caret_here && !caret_at_start {
        row = row.child(caret());
    }
    row.child(SharedString::from(after))
}

/// A list's rows: `items`, the selected one highlighted, each wired by
/// `hook` as `part(i)`. Scrolls vertically when it overflows.
fn list_rows<V: 'static>(
    id: SharedString,
    c: &ControlSpec,
    look: &ControlLook,
    part: fn(usize) -> Part,
    hook: &mut dyn FnMut(Part, Stateful<Div>, &mut Context<V>) -> Stateful<Div>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let mut list = div().id(id.clone()).flex().flex_col().overflow_y_scroll();
    if c.items.is_empty() && look.design {
        // VB's designer shows the control's name in an empty list.
        list = list.child(
            div()
                .px_1()
                .h(px(ROW_H))
                .flex()
                .items_center()
                .child(SharedString::from(c.name.clone())),
        );
    }
    for (i, item) in c.items.iter().enumerate() {
        // a MultiSelect list shows every selected row; the others the one
        let selected = if c.multi_select != 0 {
            look.selected.contains(&i)
        } else {
            look.list_index == Some(i)
        };
        let mut row = div()
            .id((id.clone(), i))
            .debug_selector(|| format!("{}-item-{}", c.name, i))
            .flex_none()
            .h(px(ROW_H))
            .px_1()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .overflow_hidden()
            .child(SharedString::from(item.clone()));
        if selected {
            row = row.bg(rgb(ACCENT)).text_color(rgb(0xFFFFFF));
        } else if !look.design {
            row = row.hover(|s| s.bg(rgb(ROW_HOVER)));
        }
        // the focused row of a MultiSelect list has a dotted box, as in VB
        if c.multi_select != 0 && look.focused && look.list_index == Some(i) {
            row = row.border_1().border_dashed().border_color(rgb(TEXT));
        }
        list = list.child(hook(part(i), row, cx));
    }
    list
}

/// The shapes a program drew on a PictureBox, painted over its background
/// and picture: lines and ellipses as stroked paths, boxes and dots as
/// quads. Coordinates are the picture box's own pixels, so the canvas is as
/// big as the control and sits at its top-left corner.
fn draw_canvas(canvas: &crate::canvas::Canvas) -> gpui::Canvas<()> {
    use crate::canvas::{ellipse_points, Shape};
    let shapes = canvas.shapes.clone();
    let raster = canvas.raster.clone();
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            // the pixels `PSet` set, as one image under the shapes
            paint_raster(&raster, bounds.origin, window);
            let origin = bounds.origin;
            let pt = |p: (f32, f32)| gpui::point(origin.x + px(p.0), origin.y + px(p.1));
            // VB draws `DrawWidth` 1 lines; a half-pixel inset keeps them crisp
            let stroke = |window: &mut gpui::Window, pts: &[(f32, f32)], color: u32| {
                let mut path = gpui::PathBuilder::stroke(px(1.0));
                let mut it = pts.iter();
                if let Some(first) = it.next() {
                    path.move_to(pt(*first));
                    for p in it {
                        path.line_to(pt(*p));
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, rgb(color));
                }
            };
            for shape in &shapes {
                match shape {
                    Shape::Line { from, to, color } => stroke(window, &[*from, *to], *color),
                    Shape::Rect {
                        from,
                        to,
                        color,
                        filled,
                    } => {
                        let (x0, x1) = (from.0.min(to.0), from.0.max(to.0));
                        let (y0, y1) = (from.1.min(to.1), from.1.max(to.1));
                        if *filled {
                            window.paint_quad(gpui::fill(
                                gpui::Bounds::new(
                                    pt((x0, y0)),
                                    gpui::size(px(x1 - x0 + 1.0), px(y1 - y0 + 1.0)),
                                ),
                                rgb(*color),
                            ));
                        } else {
                            let ring = [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)];
                            stroke(window, &ring, *color);
                        }
                    }
                    Shape::Ellipse {
                        center,
                        radius,
                        aspect,
                        color,
                        arc,
                        spokes,
                    } => {
                        let pts = ellipse_points(*center, *radius, *aspect, *arc, *spokes);
                        stroke(window, &pts, *color);
                    }
                }
            }
        },
    )
    .absolute()
    .left_0()
    .top_0()
    .size_full()
}

thread_local! {
    /// The image built for each PSet bitmap (by its id): its `rev` and the
    /// GPUI image. Rebuilt only when the bitmap changed, and the old one
    /// dropped from the window's atlas.
    static RASTER_IMAGES: std::cell::RefCell<std::collections::HashMap<u64, (u64, std::sync::Arc<gpui::RenderImage>)>> =
        Default::default();
}

/// Paint the pixels `PSet` set at `origin`, one pixel to a logical pixel.
fn paint_raster(
    raster: &crate::canvas::Raster,
    origin: gpui::Point<gpui::Pixels>,
    window: &mut gpui::Window,
) {
    if raster.id == 0 || raster.width == 0 || raster.height == 0 {
        return;
    }
    let image = RASTER_IMAGES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((rev, img)) = cache.get(&raster.id) {
            if *rev == raster.rev {
                return Some(img.clone());
            }
        }
        // GPUI wants BGRA bytes; an unset pixel stays transparent
        let mut bytes = Vec::with_capacity(raster.pixels.len() * 4);
        for p in raster.pixels.iter() {
            if *p == 0 {
                bytes.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                bytes.extend_from_slice(&[*p as u8, (*p >> 8) as u8, (*p >> 16) as u8, 255]);
            }
        }
        let buffer = image::RgbaImage::from_raw(raster.width as u32, raster.height as u32, bytes)?;
        let img = std::sync::Arc::new(gpui::RenderImage::new(smallvec::SmallVec::from_elem(
            image::Frame::new(buffer),
            1,
        )));
        if let Some((_, old)) = cache.insert(raster.id, (raster.rev, img.clone())) {
            let _ = window.drop_image(old);
        }
        Some(img)
    });
    if let Some(img) = image {
        let size = gpui::size(px(raster.width as f32), px(raster.height as f32));
        let _ = window.paint_image(
            gpui::Bounds::new(origin, size),
            gpui::Corners::default(),
            img,
            0,
            false,
        );
    }
}

/// Draw control `c` as VB6 shows it, at its form position. `hook` attaches
/// the caller's event handlers (and debug selectors) to each [`Part`]; the
/// designer and the run windows differ only in their hooks.
pub fn paint_control<V: 'static>(
    c: &ControlSpec,
    look: &ControlLook,
    hook: &mut dyn FnMut(Part, Stateful<Div>, &mut Context<V>) -> Stateful<Div>,
    cx: &mut Context<V>,
) -> AnyElement {
    let id = SharedString::from(match c.index {
        Some(n) => format!("control-{}-{}", c.name, n),
        None => format!("control-{}", c.name),
    });
    let (w, h) = if c.kind == ControlKind::Timer {
        (28.0, 28.0)
    } else {
        (c.rect.width.max(4.0), c.rect.height.max(4.0))
    };
    let (left, top) = look.origin.unwrap_or((c.rect.left, c.rect.top));
    let font = &c.font;
    let mut base = div()
        .id(id.clone())
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(w))
        .h(px(h))
        .text_color(rgb(if !c.enabled {
            DIM
        } else {
            c.fore_color.unwrap_or(TEXT)
        }));
    // `MousePointer` and `ToolTipText` at run time
    if !look.design {
        if c.mouse_pointer == 99 && !c.mouse_icon.is_empty() {
            // a custom pointer (`MouseIcon`): shown through the style GPUI
            // hands to macOS as DragLink, with the icon set as the pointer
            // enters (and cleared as it leaves)
            let icon = match &look.base_dir {
                Some(dir) => dir.join(&c.mouse_icon),
                None => std::path::PathBuf::from(&c.mouse_icon),
            };
            if crate::mac_cursor::icon_cursor_usable(&icon) {
                base = base.cursor(CursorStyle::DragLink);
            } else {
                base = base.cursor(CursorStyle::PointingHand);
            }
            // the icon is the pointer only while this control is under it
            base = base.on_hover(move |hovered, _w, _cx| {
                crate::mac_cursor::set_icon_cursor(hovered.then_some(icon.as_path()));
            });
        } else if let Some(cursor) = cursor_for(c.mouse_pointer) {
            base = base.cursor(cursor);
        }
        if !c.tool_tip.is_empty() {
            let tip = SharedString::from(c.tool_tip.clone());
            base = base.tooltip(move |_w, cx| cx.new(|_| Tip(tip.clone())).into());
        }
    }
    // `Font`: points to pixels at 96 dpi (VB's 8.25 pt MS Sans Serif is
    // 11 px); an unset size keeps the size controls are drawn at
    if c.kind.has_font() {
        if font.size != DEFAULT_FONT_SIZE && font.size > 0.0 {
            base = base.text_size(px(font.size * 96.0 / 72.0));
        }
        if !font.name.is_empty() {
            base = base.font_family(SharedString::from(font.name.clone()));
        }
        if font.bold {
            base = base.font_weight(FontWeight::BOLD);
        }
        if font.italic {
            base = base.italic();
        }
        if font.underline {
            base = base.underline();
        }
        if font.strikethrough {
            base = base.line_through();
        }
    }
    // `BackColor`, where the control has a background of its own
    let back = |usual: u32| rgb(c.color.unwrap_or(usual));
    let el = match c.kind {
        // VB6 Label: BorderStyle 0 (None), so it is just text — on the
        // form's background unless it has a BackColor.
        ControlKind::Label => {
            let mut body = base
                .overflow_hidden()
                .child(SharedString::from(c.caption.clone()));
            if let Some(bg) = c.color {
                body = body.bg(rgb(bg));
            }
            hook(Part::Body, body, cx)
        }
        ControlKind::CommandButton => {
            let mut body = base
                .flex()
                .items_center()
                .justify_center()
                .px_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .rounded_md()
                .bg(back(BUTTON_FACE))
                .border_1()
                .border_color(rgb(BUTTON_BORDER))
                .child(SharedString::from(c.caption.clone()));
            if !look.design && c.enabled {
                body = body
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(BUTTON_HOVER)))
                    .active(|s| s.bg(rgb(BUTTON_ACTIVE)));
            }
            if look.focused {
                body = body.border_color(rgb(ACCENT));
            }
            hook(Part::Body, body, cx)
        }
        ControlKind::TextBox => hook(Part::Body, text_field(base, c, look).bg(back(0xFFFFFF)), cx),
        // DataGrid: the visible cells, the current one framed, and scrollbars
        ControlKind::DataGrid => hook(Part::Body, crate::grid_paint::paint_grid(base, c, look), cx),
        // VB's TabControl: a strip of tabs across the top, the selected one
        // raised; the page below is the control's body (its controls draw on it)
        ControlKind::TabControl => {
            const STRIP: f32 = TAB_STRIP_H;
            let mut strip = div()
                .absolute()
                .left_0()
                .top_0()
                .right_0()
                .h(px(STRIP))
                .flex();
            for (i, caption) in c.tabs.iter().enumerate() {
                let selected = i as u8 == look.tab;
                // the selected tab opens into the page: its bottom edge is
                // open, and it is the page's colour and bold; the others are
                // greyed and sit back from the page
                let mut tab = div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(tab_width(caption)))
                    .whitespace_nowrap()
                    .border_1()
                    .border_color(rgb(BUTTON_BORDER))
                    .rounded_t_md()
                    .child(SharedString::from(caption.clone()));
                if selected {
                    // reaches 2px over the page's top border (painted before
                    // the strip, so the tab covers it) with its bottom edge
                    // open: it merges into the page it selects
                    tab = tab
                        .h(px(STRIP + 2.))
                        .bg(rgb(FORM_BACK))
                        .text_color(rgb(TEXT))
                        .font_weight(FontWeight::BOLD)
                        .border_b_0();
                } else {
                    tab = tab
                        .h(px(STRIP - 4.))
                        .mt(px(4.))
                        .bg(rgb(BUTTON_FACE))
                        .text_color(rgb(0x6E6E73));
                }
                strip = strip.child(tab);
            }
            let page = div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(STRIP))
                .bottom_0()
                .border_1()
                .border_color(rgb(BUTTON_BORDER))
                .bg(rgb(FORM_BACK));
            // the strip is painted after the page so the selected tab can
            // reach over the page's top border and open into it
            hook(Part::Body, base.child(page).child(strip), cx)
        }
        // VB6 Frame: a group box. The caption interrupts the top border.
        ControlKind::Frame => {
            let outline = div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(8.0))
                .bottom_0()
                .border_1()
                .border_color(rgb(BUTTON_BORDER))
                .rounded_sm();
            let mut body = base.child(outline);
            if !c.caption.is_empty() {
                body = body.child(
                    div()
                        .absolute()
                        .left(px(8.0))
                        .top_0()
                        .px_1()
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .bg(rgb(FORM_BACK))
                        .whitespace_nowrap()
                        .child(SharedString::from(c.caption.clone())),
                );
            }
            hook(Part::Body, body, cx)
        }
        // VB's Image: just the picture, no border; Stretch scales it to the
        // control, otherwise it is drawn at its own size
        ControlKind::Image => {
            let mut body = base.overflow_hidden();
            if !c.picture.is_empty() {
                let path = match &look.base_dir {
                    Some(dir) => dir.join(&c.picture),
                    None => std::path::PathBuf::from(&c.picture),
                };
                let fit = if c.stretch {
                    ObjectFit::Fill
                } else {
                    ObjectFit::None
                };
                let mut pic = img(path)
                    .debug_selector(|| format!("{}-picture", c.name))
                    .absolute()
                    .left_0()
                    .top_0()
                    .object_fit(fit);
                if c.stretch {
                    pic = pic.size_full();
                }
                body = body.child(pic);
            }
            hook(Part::Body, body, cx)
        }
        ControlKind::ListBox
        | ControlKind::VolumeListBox
        | ControlKind::DirListBox
        | ControlKind::FileListBox => {
            let rows = list_rows(
                SharedString::from(format!("{}-list", c.name)),
                c,
                look,
                Part::ListItem,
                hook,
                cx,
            )
            .size_full();
            let body = base
                .bg(back(0xFFFFFF))
                .border_1()
                .border_color(rgb(if look.focused { ACCENT } else { FIELD_BORDER }))
                .overflow_hidden()
                .child(rows);
            hook(Part::Body, body, cx)
        }
        ControlKind::ComboBox => paint_combo(c, look, base, hook, cx),
        ControlKind::PictureBox => {
            let mut body = base
                .bg(back(0xFFFFFF))
                .border_1()
                .border_color(rgb(FIELD_BORDER))
                .overflow_hidden();
            // `Picture`: the image, at its own size from the top-left corner
            // as VB draws it (AutoSize off)
            if !c.picture.is_empty() {
                let path = match &look.base_dir {
                    Some(dir) => dir.join(&c.picture),
                    None => std::path::PathBuf::from(&c.picture),
                };
                body = body.child(
                    img(path)
                        .debug_selector(|| format!("{}-picture", c.name))
                        .absolute()
                        .left_0()
                        .top_0()
                        .object_fit(ObjectFit::None),
                );
            }
            // what the program drew over it
            if !look.design && (!look.canvas.shapes.is_empty() || look.canvas.raster.id != 0) {
                body = body.child(
                    div()
                        .debug_selector(|| format!("{}-canvas", c.name))
                        .absolute()
                        .left_0()
                        .top_0()
                        .size_full()
                        .child(draw_canvas(&look.canvas)),
                );
            }
            hook(Part::Body, body, cx)
        }
        // VB6 CheckBox / OptionButton: a box (☑) or circle (◉) and the
        // caption beside it; `Value` decides the mark.
        ControlKind::CheckBox | ControlKind::OptionButton => {
            let radio = c.kind == ControlKind::OptionButton;
            let mark = match (radio, look.value) {
                (true, 0) => "",
                (true, _) => "●",
                (false, 1) => "✓",
                (false, 2) => "■",
                (false, _) => "",
            };
            let indicator = div()
                .flex_none()
                .size(px(14.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(10.0))
                .bg(rgb(0xFFFFFF))
                .border_1()
                .border_color(rgb(if look.focused { ACCENT } else { FIELD_BORDER }))
                .text_color(rgb(if c.kind == ControlKind::CheckBox && look.value == 2 {
                    DIM
                } else {
                    ACCENT
                }))
                .when(radio, |s| s.rounded_full())
                .when(!radio, |s| s.rounded_sm())
                .child(mark);
            let mut body = base
                .flex()
                .items_center()
                .gap_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(indicator)
                .child(SharedString::from(c.caption.clone()));
            if let Some(bg) = c.color {
                body = body.bg(rgb(bg));
            }
            if !look.design && c.enabled {
                body = body.cursor_pointer();
            }
            hook(Part::Body, body, cx)
        }
        // VB's scrollbars: a track with a thumb at `Value` within
        // `Min`..`Max`. The thumb's position is the spec's value here; the
        // live window moves it as the program sets `Value`.
        ControlKind::HScrollBar | ControlKind::VScrollBar => {
            let horizontal = c.kind == ControlKind::HScrollBar;
            let span = (c.scroll_max - c.scroll_min).max(1) as f32;
            // the running window's live Value (look.value), or the spec's in the designer
            let value = if look.design { c.value } else { look.value };
            let frac = ((value - c.scroll_min) as f32 / span).clamp(0.0, 1.0);
            let track = base
                .overflow_hidden()
                .bg(rgb(BUTTON_FACE))
                .border_1()
                .border_color(rgb(BUTTON_BORDER));
            let thumb = div().absolute().bg(rgb(BUTTON_BORDER)).rounded_sm();
            let thumb = if horizontal {
                thumb.top_0().bottom_0().w(px(12.0)).left(relative(frac))
            } else {
                thumb.left_0().right_0().h(px(12.0)).top(relative(frac))
            };
            hook(Part::Body, track.child(thumb), cx)
        }
        // VB's Slider: a track with a thumb at Value, horizontal or vertical
        // by its Orientation (0 horizontal, 1 vertical)
        ControlKind::Slider => {
            let horizontal = c.orientation == 0;
            let span = (c.scroll_max - c.scroll_min).max(1) as f32;
            let value = if look.design { c.value } else { look.value };
            let frac = ((value - c.scroll_min) as f32 / span).clamp(0.0, 1.0);
            let track = base
                .overflow_hidden()
                .bg(rgb(BUTTON_FACE))
                .border_1()
                .border_color(rgb(BUTTON_BORDER));
            let thumb = div().absolute().bg(rgb(ACCENT)).rounded_sm();
            let thumb = if horizontal {
                thumb
                    .top(px(2.0))
                    .bottom(px(2.0))
                    .w(px(10.0))
                    .left(relative(frac))
            } else {
                thumb
                    .left(px(2.0))
                    .right(px(2.0))
                    .h(px(10.0))
                    .top(relative(frac))
            };
            hook(Part::Body, track.child(thumb), cx)
        }
        // VB's ProgressBar: a track filled from its start to Value (not draggable)
        ControlKind::ProgressBar => {
            let span = (c.scroll_max - c.scroll_min).max(1) as f32;
            let value = if look.design { c.value } else { look.value };
            let frac = ((value - c.scroll_min) as f32 / span).clamp(0.0, 1.0);
            let track = base
                .overflow_hidden()
                .bg(rgb(BUTTON_FACE))
                .border_1()
                .border_color(rgb(BUTTON_BORDER));
            let fill = div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(relative(frac))
                .bg(rgb(ACCENT));
            hook(Part::Body, track.child(fill), cx)
        }
        // by the same code that paints a PictureBox's drawing
        ControlKind::Line | ControlKind::Shape => {
            let mut canvas = crate::canvas::Canvas::default();
            // kept on the form: a Shape or Line past the edge is drawn to it
            let (w, h) = match look.form_size {
                Some((fw, fh)) => {
                    let (left, top) = look.origin.unwrap_or((c.rect.left, c.rect.top));
                    crate::canvas::clamped_size(c.rect.width, c.rect.height, left, top, fw, fh)
                }
                None => (c.rect.width, c.rect.height),
            };
            if let Some(shape) = crate::canvas::control_shape(
                c.kind,
                w,
                h,
                c.fore_color.unwrap_or(TEXT),
                c.shape_type,
                c.fill_style,
                c.diagonal,
            ) {
                canvas.shapes.push(shape);
            }
            hook(
                Part::Body,
                base.overflow_hidden().child(draw_canvas(&canvas)),
                cx,
            )
        }
        // Invisible at run time; the designer shows an icon to grab.
        ControlKind::Timer => {
            let body = base
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(rgb(BUTTON_FACE))
                .border_1()
                .border_color(rgb(BUTTON_BORDER))
                .child("◷");
            hook(Part::Body, body, cx)
        }
    };
    el.into_any_element()
}

/// The VB6 ComboBox, by `Style`:
///
/// * 0 *Dropdown Combo* — an editable text field with a ▾ button that
///   drops the list down;
/// * 1 *Simple Combo* — an editable text field with the list always shown
///   below it (the control is as tall as both);
/// * 2 *Dropdown List* — like 0, but the text can only be picked.
///
/// A dropped list floats over the controls below and closes on a pick.
fn paint_combo<V: 'static>(
    c: &ControlSpec,
    look: &ControlLook,
    base: Stateful<Div>,
    hook: &mut dyn FnMut(Part, Stateful<Div>, &mut Context<V>) -> Stateful<Div>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let id = SharedString::from(format!("{}-combo", c.name));
    let field_h = ROW_H + 6.0;
    if c.style == 1 {
        // Text field on top, list below.
        let field = text_field(
            div().id((id.clone(), 0usize)).h(px(field_h)).w_full(),
            c,
            look,
        );
        let rows = list_rows(
            SharedString::from(format!("{}-list", c.name)),
            c,
            look,
            Part::ListItem,
            hook,
            cx,
        )
        .flex_1()
        .min_h_0();
        let list = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(0xFFFFFF))
            .border_1()
            .border_color(rgb(FIELD_BORDER))
            .overflow_hidden()
            .child(rows);
        let body = base
            .flex()
            .flex_col()
            .child(hook(Part::Field, field, cx))
            .child(list);
        return hook(Part::Body, body, cx);
    }

    let editable = c.style == 0;
    let shown = if editable {
        look.text.clone()
    } else {
        // a Dropdown List shows the picked item, or nothing yet
        look.list_index
            .and_then(|i| c.items.get(i).cloned())
            .unwrap_or_else(|| look.text.clone())
    };
    let arrow = hook(
        Part::Arrow,
        div()
            .id((id.clone(), 1usize))
            .flex_none()
            .w(px(ARROW_W))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(rgb(FIELD_BORDER))
            .bg(rgb(if look.dropped {
                BUTTON_ACTIVE
            } else {
                BUTTON_FACE
            }))
            .when(!look.design, |s| s.cursor_pointer())
            .child(if look.dropped { "▴" } else { "▾" }),
        cx,
    );
    let mut field = hook(
        Part::Field,
        div()
            .id((id.clone(), 2usize))
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .px_1()
            .overflow_hidden()
            .whitespace_nowrap()
            .when(!editable && !look.design, |s| s.cursor_pointer())
            .child(SharedString::from(shown)),
        cx,
    );
    if editable && look.focused {
        field = field.child(caret());
    }
    let mut body = base
        .flex()
        .items_center()
        .bg(rgb(0xFFFFFF))
        .border_1()
        .border_color(rgb(if look.focused || look.dropped {
            ACCENT
        } else {
            FIELD_BORDER
        }))
        .child(field)
        .child(arrow);
    if look.dropped && !look.design {
        let rows = list_rows(
            SharedString::from(format!("{}-drop", c.name)),
            c,
            look,
            Part::DropItem,
            hook,
            cx,
        );
        let visible = (c.items.len().max(1) as f32).min(8.0);
        body = body.child(deferred(
            div()
                .id((id, 3usize))
                .occlude()
                .absolute()
                .top(px(c.rect.height.max(4.0) - 1.0))
                .left(px(-1.0))
                .w(px(c.rect.width.max(4.0)))
                .h(px(visible * ROW_H + 2.0))
                .bg(rgb(0xFFFFFF))
                .border_1()
                .border_color(rgb(FIELD_BORDER))
                .shadow_md()
                .overflow_hidden()
                .child(rows.size_full()),
        ));
    }
    hook(Part::Body, body, cx)
}

#[cfg(test)]
mod cursor_tests {
    use super::*;

    #[test]
    fn mouse_pointers_map_to_gpui_cursors() {
        assert_eq!(cursor_for(0), None, "the default is the control's own");
        assert_eq!(cursor_for(1), Some(CursorStyle::Arrow));
        assert_eq!(cursor_for(3), Some(CursorStyle::IBeam));
        assert_eq!(cursor_for(99), Some(CursorStyle::PointingHand));
        assert_eq!(cursor_for(42), None);
        // the hourglass: the wait cursor where macOS has it, else the arrow
        let wait = if crate::mac_cursor::install_wait_cursor() {
            CursorStyle::DragCopy
        } else {
            CursorStyle::Arrow
        };
        assert_eq!(cursor_for(11), Some(wait));
        assert_eq!(cursor_for(13), Some(wait));
    }
}
