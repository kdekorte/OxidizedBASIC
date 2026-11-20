//! The draggable bars between the main window's panes: the toolbox, the
//! Project / Properties column, the Project / Properties boundary and the
//! Immediate window. Dragging a bar resizes the pane it borders; the sizes
//! are kept within limits so no pane can be dragged out of sight or take the
//! whole window.

use super::*;

/// A bar, named for the pane it resizes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Split {
    /// Between the Toolbox and the designer: the Toolbox's width.
    Toolbox,
    /// Between the designer and the Project / Properties column: its width.
    Side,
    /// Between Project and Properties: the Project panel's height.
    Project,
    /// Between the main area and the Immediate window: its height.
    Output,
}

impl Split {
    /// Whether the bar is a vertical line, dragged left and right.
    pub(super) fn is_vertical(self) -> bool {
        matches!(self, Split::Toolbox | Split::Side)
    }

    /// +1 when dragging right or down makes the pane bigger, -1 when it
    /// makes it smaller (the pane is to the right of, or below, the bar).
    fn sign(self) -> f32 {
        match self {
            Split::Toolbox | Split::Project => 1.0,
            Split::Side | Split::Output => -1.0,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Split::Toolbox => "toolbox",
            Split::Side => "side",
            Split::Project => "project",
            Split::Output => "output",
        }
    }
}

/// A drag in progress.
#[derive(Clone, Copy, Debug)]
pub(super) struct SplitDrag {
    which: Split,
    /// The pointer's x (vertical bar) or y (horizontal bar) at the start.
    pointer_start: f32,
    /// The pane's size at the start.
    size_start: f32,
}

/// The sizes the bars set, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Panes {
    pub toolbox_w: f32,
    pub side_w: f32,
    pub project_h: f32,
    pub output_h: f32,
}

impl Default for Panes {
    fn default() -> Panes {
        Panes {
            toolbox_w: 192.0,
            side_w: 288.0,
            project_h: 160.0,
            output_h: 160.0,
        }
    }
}

/// The smallest and largest size a pane may have in a window of
/// `viewport` (width, height).
pub(super) fn limits(which: Split, viewport: (f32, f32)) -> (f32, f32) {
    let (w, h) = viewport;
    let (min, max) = match which {
        Split::Toolbox => (120.0, w * 0.4),
        Split::Side => (200.0, w * 0.6),
        Split::Project => (56.0, h * 0.5),
        Split::Output => (56.0, h * 0.7),
    };
    (min, max.max(min))
}

impl Panes {
    pub(super) fn get(&self, which: Split) -> f32 {
        match which {
            Split::Toolbox => self.toolbox_w,
            Split::Side => self.side_w,
            Split::Project => self.project_h,
            Split::Output => self.output_h,
        }
    }

    /// Set a pane's size, kept within [`limits`].
    pub(super) fn set(&mut self, which: Split, size: f32, viewport: (f32, f32)) {
        let (min, max) = limits(which, viewport);
        let size = size.clamp(min, max);
        match which {
            Split::Toolbox => self.toolbox_w = size,
            Split::Side => self.side_w = size,
            Split::Project => self.project_h = size,
            Split::Output => self.output_h = size,
        }
    }
}

/// The thickness of a bar, which is also the gap it leaves between panes.
pub(super) const BAR: f32 = 6.0;

impl Ide {
    pub(super) fn viewport(&self) -> (f32, f32) {
        (
            f32::from(self.viewport.width),
            f32::from(self.viewport.height),
        )
    }

    /// A bar between two panes: it shows a resize cursor, lights up under
    /// the pointer, and starts a drag when pressed.
    pub(super) fn splitter(
        &self,
        which: Split,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let dragging = self.split_drag.is_some_and(|d| d.which == which);
        let bar = div()
            .id(SharedString::from(format!("split-{}", which.name())))
            .debug_selector(move || format!("split-{}", which.name()))
            .flex_none()
            .rounded_full()
            .hover(|s| s.bg(rgb(0xC7C7CC)))
            .when(dragging, |s| s.bg(rgb(ACCENT)));
        let bar = if which.is_vertical() {
            bar.w(px(BAR)).h_full().cursor_col_resize()
        } else {
            bar.h(px(BAR)).w_full().cursor_row_resize()
        };
        bar.on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |this: &mut Ide, ev: &MouseDownEvent, _win, cx| {
                cx.stop_propagation();
                let pointer = if which.is_vertical() {
                    f32::from(ev.position.x)
                } else {
                    f32::from(ev.position.y)
                };
                this.split_drag = Some(SplitDrag {
                    which,
                    pointer_start: pointer,
                    size_start: this.panes.get(which),
                });
                cx.notify();
            }),
        )
    }

    /// The pointer moved: while a bar is held, resize its pane to follow.
    pub(super) fn update_split_drag(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(d) = self.split_drag else { return };
        if !ev.dragging() {
            self.split_drag = None;
            cx.notify();
            return;
        }
        let pointer = if d.which.is_vertical() {
            f32::from(ev.position.x)
        } else {
            f32::from(ev.position.y)
        };
        let size = d.size_start + d.which.sign() * (pointer - d.pointer_start);
        let viewport = self.viewport();
        self.panes.set(d.which, size, viewport);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: (f32, f32) = (1200.0, 800.0);

    #[test]
    fn a_pane_stays_between_its_limits() {
        let mut p = Panes::default();
        p.set(Split::Side, 5000.0, VIEW);
        assert_eq!(p.side_w, 720.0, "at most 60% of the width");
        p.set(Split::Side, 3.0, VIEW);
        assert_eq!(p.side_w, 200.0, "at least 200");
        p.set(Split::Output, 10_000.0, VIEW);
        assert_eq!(p.output_h, 560.0);
        p.set(Split::Output, -5.0, VIEW);
        assert_eq!(p.output_h, 56.0, "it can't be dragged out of sight");
        p.set(Split::Toolbox, 300.0, VIEW);
        assert_eq!(p.toolbox_w, 300.0, "a size in range is kept");
    }

    #[test]
    fn a_tiny_window_never_makes_the_limits_cross() {
        for which in [Split::Toolbox, Split::Side, Split::Project, Split::Output] {
            let (min, max) = limits(which, (100.0, 100.0));
            assert!(min <= max, "{:?}", which);
        }
    }

    #[test]
    fn dragging_right_grows_a_pane_on_the_left_and_shrinks_one_on_the_right() {
        assert_eq!(Split::Toolbox.sign(), 1.0);
        assert_eq!(Split::Side.sign(), -1.0);
        assert_eq!(Split::Project.sign(), 1.0);
        assert_eq!(Split::Output.sign(), -1.0);
    }
}
