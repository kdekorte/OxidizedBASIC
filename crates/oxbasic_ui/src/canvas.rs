//! What a PictureBox (or the form) has drawn on it by `Line`, `PSet`,
//! `Circle` and `Cls`, kept as a list of shapes in the drawing's own
//! pixels, in the order they were drawn, so the window can paint them over
//! the control's background and picture. No GUI is needed here.

use oxbasic_runtime::DrawCmd;

/// One thing drawn, its colour settled (`0xRRGGBB`).
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Line {
        from: (f32, f32),
        to: (f32, f32),
        color: u32,
    },
    /// A box, outlined or filled.
    Rect {
        from: (f32, f32),
        to: (f32, f32),
        color: u32,
        filled: bool,
    },
    /// An ellipse (a circle for `aspect` 1) or an arc of one.
    Ellipse {
        center: (f32, f32),
        radius: f32,
        aspect: f32,
        color: u32,
        /// An arc, in radians from 3 o'clock, counter-clockwise (VB).
        arc: Option<(f32, f32)>,
        /// The radii to the arc's ends are drawn too (a negative start or
        /// end in VB).
        spokes: (bool, bool),
    },
}

/// The pixels `PSet` set, kept as a real bitmap: a picture drawn a pixel at
/// a time (a fractal) has hundreds of thousands of them, far too many to
/// keep as shapes. It is painted under the shapes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Raster {
    /// Names this bitmap for the window's image; stays the same for as
    /// long as the canvas lives (0 until the first pixel).
    pub id: u64,
    /// Counts the changes, so the window knows when to rebuild its image.
    pub rev: u64,
    pub width: usize,
    pub height: usize,
    /// Row by row; 0 where nothing was drawn, else `0xFF000000 | 0xRRGGBB`.
    pub pixels: std::sync::Arc<Vec<u32>>,
}

impl Raster {
    /// Make the bitmap at least `w` by `h` pixels, keeping what was drawn.
    fn ensure(&mut self, w: usize, h: usize) {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        if self.id == 0 {
            self.id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let (w, h) = (w.max(self.width), h.max(self.height));
        if (w, h) == (self.width, self.height) {
            return;
        }
        let mut pixels = vec![0u32; w * h];
        let old = self.width;
        for row in 0..self.height {
            pixels[row * w..row * w + old]
                .copy_from_slice(&self.pixels[row * old..(row + 1) * old]);
        }
        self.pixels = std::sync::Arc::new(pixels);
        self.width = w;
        self.height = h;
        self.rev += 1;
    }

    /// Set the pixel under `at`; a point outside the bitmap is clipped, as
    /// in VB.
    fn set(&mut self, at: (f32, f32), color: u32) {
        let (x, y) = (at.0.floor(), at.1.floor());
        if x < 0.0 || y < 0.0 || x >= self.width as f32 || y >= self.height as f32 {
            return;
        }
        let i = y as usize * self.width + x as usize;
        let px = 0xFF00_0000 | (color & 0xFF_FFFF);
        if self.pixels[i] != px {
            std::sync::Arc::make_mut(&mut self.pixels)[i] = px;
            self.rev += 1;
        }
    }

    /// The `0xRRGGBB` of the pixel at `(x, y)`, or `None` where nothing was
    /// drawn.
    pub fn pixel(&self, x: usize, y: usize) -> Option<u32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let p = self.pixels[y * self.width + x];
        (p != 0).then_some(p & 0xFF_FFFF)
    }

    /// How many pixels were drawn.
    pub fn count(&self) -> usize {
        self.pixels.iter().filter(|p| **p != 0).count()
    }

    fn clear(&mut self) {
        if self.pixels.iter().any(|p| *p != 0) {
            std::sync::Arc::make_mut(&mut self.pixels).fill(0);
            self.rev += 1;
        }
    }
}

/// A drawing surface's shapes and its current point.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Canvas {
    /// The pixels set with `PSet`.
    pub raster: Raster,
    /// The drawable area in pixels (the control's inside), which the bitmap
    /// is made to cover; set by the host from the control's size.
    pub extent: (usize, usize),
    pub shapes: Vec<Shape>,
    /// Where `Step` counts from and `Line -(x, y)` starts: the end of the
    /// last thing drawn (`CurrentX`, `CurrentY`).
    pub current: (f32, f32),
}

/// The most shapes kept; the oldest go first (a program that draws every
/// frame can't grow without bound).
pub const MAX_SHAPES: usize = 20_000;

impl Canvas {
    /// `Cls`: clear it, and the current point with it.
    pub fn clear(&mut self) {
        self.shapes.clear();
        self.raster.clear();
        self.current = (0.0, 0.0);
    }

    /// Draw `cmd`, a colour of `None` being `fore` (the control's
    /// `ForeColor`, `0xRRGGBB`). `resolve` turns the program's VB colour
    /// `Long` into `0xRRGGBB` (system colours included), or says it's
    /// invalid.
    pub fn draw<E>(
        &mut self,
        cmd: DrawCmd,
        fore: u32,
        resolve: impl Fn(u32) -> Result<u32, E>,
    ) -> Result<(), E> {
        let pick = |c: Option<u32>| -> Result<u32, E> {
            match c {
                Some(vb) => resolve(vb),
                None => Ok(fore),
            }
        };
        match cmd {
            DrawCmd::Line {
                from,
                to,
                color,
                boxed,
            } => {
                let color = pick(color)?;
                self.push(match boxed {
                    0 => Shape::Line { from, to, color },
                    n => Shape::Rect {
                        from,
                        to,
                        color,
                        filled: n == 2,
                    },
                });
                self.current = to;
            }
            DrawCmd::PSet { at, color } => {
                let color = pick(color)?;
                let (w, h) = self.extent;
                self.raster.ensure(w, h);
                self.raster.set(at, color);
                self.current = at;
            }
            DrawCmd::Circle {
                center,
                radius,
                color,
                start,
                end,
                aspect,
            } => {
                let color = pick(color)?;
                // a negative start or end also draws the radius to it, and
                // counts as its absolute value
                let spoke = |v: Option<f32>| v.is_some_and(|v| v < 0.0);
                let arc = match (start, end) {
                    (None, None) => None,
                    (s, e) => Some((
                        s.map_or(0.0, f32::abs),
                        e.map_or(std::f32::consts::TAU, f32::abs),
                    )),
                };
                self.push(Shape::Ellipse {
                    center,
                    radius: radius.abs(),
                    aspect: if aspect > 0.0 { aspect } else { 1.0 },
                    color,
                    arc,
                    spokes: (spoke(start), spoke(end)),
                });
                self.current = center;
            }
        }
        Ok(())
    }

    fn push(&mut self, s: Shape) {
        if self.shapes.len() >= MAX_SHAPES {
            self.shapes.remove(0);
        }
        self.shapes.push(s);
    }
}

/// The points of an ellipse (or arc of it) as a polyline for the window to
/// stroke. Angles run counter-clockwise from 3 o'clock, as in VB. `aspect`
/// is height over width, as in VB: under 1 the ellipse is wider than tall,
/// over 1 taller than wide; `radius` is the longer half-axis.
pub fn ellipse_points(
    center: (f32, f32),
    radius: f32,
    aspect: f32,
    arc: Option<(f32, f32)>,
    spokes: (bool, bool),
) -> Vec<(f32, f32)> {
    let (rx, ry) = if aspect <= 1.0 {
        (radius, radius * aspect)
    } else {
        (radius / aspect, radius)
    };
    let (a0, a1) = arc.unwrap_or((0.0, std::f32::consts::TAU));
    // an arc whose end is before its start goes the long way round
    let a1 = if a1 <= a0 {
        a1 + std::f32::consts::TAU
    } else {
        a1
    };
    let steps = (((a1 - a0) / std::f32::consts::TAU * 96.0).ceil() as usize).max(2);
    let at = |a: f32| (center.0 + rx * a.cos(), center.1 - ry * a.sin());
    let mut pts = Vec::with_capacity(steps + 3);
    if spokes.0 {
        pts.push(center);
    }
    for i in 0..=steps {
        pts.push(at(a0 + (a1 - a0) * i as f32 / steps as f32));
    }
    if spokes.1 {
        pts.push(center);
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORE: u32 = 0x112233;

    fn draw(cv: &mut Canvas, cmd: DrawCmd) {
        // the program's colours are VB Longs; this resolver keeps them
        cv.draw::<()>(cmd, FORE, Ok).unwrap();
    }

    #[test]
    fn shapes_are_kept_in_order_with_the_fore_colour_by_default() {
        let mut cv = Canvas {
            extent: (20, 20),
            ..Canvas::default()
        };
        draw(
            &mut cv,
            DrawCmd::Line {
                from: (0.0, 0.0),
                to: (10.0, 5.0),
                color: None,
                boxed: 0,
            },
        );
        draw(
            &mut cv,
            DrawCmd::Line {
                from: (1.0, 1.0),
                to: (4.0, 4.0),
                color: Some(0xABCDEF),
                boxed: 2,
            },
        );
        draw(
            &mut cv,
            DrawCmd::PSet {
                at: (7.0, 8.0),
                color: None,
            },
        );
        assert_eq!(
            cv.shapes,
            [
                Shape::Line {
                    from: (0.0, 0.0),
                    to: (10.0, 5.0),
                    color: FORE
                },
                Shape::Rect {
                    from: (1.0, 1.0),
                    to: (4.0, 4.0),
                    color: 0xABCDEF,
                    filled: true
                },
            ]
        );
        assert_eq!(
            cv.raster.pixel(7, 8),
            Some(FORE),
            "PSet sets a bitmap pixel"
        );
        assert_eq!(cv.raster.count(), 1);
        assert_eq!(cv.current, (7.0, 8.0), "the last point drawn");
        cv.clear();
        assert!(cv.shapes.is_empty() && cv.current == (0.0, 0.0));
        assert_eq!(cv.raster.count(), 0, "Cls clears the pixels too");
    }

    #[test]
    fn a_circle_remembers_its_centre_and_an_arc_its_spokes() {
        let mut cv = Canvas::default();
        draw(
            &mut cv,
            DrawCmd::Circle {
                center: (50.0, 40.0),
                radius: 20.0,
                color: None,
                start: Some(-0.5),
                end: Some(1.5),
                aspect: 0.5,
            },
        );
        let Shape::Ellipse {
            arc,
            spokes,
            aspect,
            ..
        } = cv.shapes[0].clone()
        else {
            panic!()
        };
        assert_eq!(arc, Some((0.5, 1.5)), "a negative start is its magnitude");
        assert_eq!(spokes, (true, false), "and draws the radius to it");
        assert_eq!(aspect, 0.5);
        assert_eq!(cv.current, (50.0, 40.0));
    }

    #[test]
    fn a_bad_colour_is_the_resolvers_error_and_draws_nothing() {
        let mut cv = Canvas::default();
        let r = cv.draw(
            DrawCmd::PSet {
                at: (1.0, 1.0),
                color: Some(99),
            },
            0,
            |_| Err("bad"),
        );
        assert_eq!(r, Err("bad"));
        assert!(cv.shapes.is_empty());
    }

    #[test]
    fn the_oldest_shapes_go_when_there_are_too_many() {
        let mut cv = Canvas::default();
        for i in 0..(MAX_SHAPES + 5) {
            draw(
                &mut cv,
                DrawCmd::Line {
                    from: (i as f32, 0.0),
                    to: (i as f32, 1.0),
                    color: None,
                    boxed: 0,
                },
            );
        }
        assert_eq!(cv.shapes.len(), MAX_SHAPES);
        assert_eq!(
            cv.shapes[0],
            Shape::Line {
                from: (5.0, 0.0),
                to: (5.0, 1.0),
                color: FORE
            }
        );
    }

    #[test]
    fn pset_fills_a_bitmap_of_any_size_without_a_shape_limit() {
        // a whole 640 x 480 picture, a pixel at a time (the fractal sample)
        let mut cv = Canvas {
            extent: (640, 480),
            ..Canvas::default()
        };
        for y in 0..480 {
            for x in 0..640 {
                draw(
                    &mut cv,
                    DrawCmd::PSet {
                        at: (x as f32, y as f32),
                        color: Some((x * 7 + y) as u32 & 0xFF_FFFF),
                    },
                );
            }
        }
        assert!(cv.shapes.is_empty(), "pixels are not shapes");
        assert_eq!(cv.raster.count(), 640 * 480);
        assert_eq!(cv.raster.pixel(639, 479), Some(639 * 7 + 479));
        // outside the bitmap is clipped; setting the same colour again
        // isn't a change
        let rev = cv.raster.rev;
        draw(
            &mut cv,
            DrawCmd::PSet {
                at: (-1.0, 700.0),
                color: None,
            },
        );
        draw(
            &mut cv,
            DrawCmd::PSet {
                at: (0.0, 0.0),
                color: Some(0),
            },
        );
        assert_eq!(cv.raster.rev, rev);
        assert_eq!(cv.raster.count(), 640 * 480);
    }

    #[test]
    fn the_bitmap_grows_with_the_control_and_keeps_its_pixels() {
        let mut cv = Canvas {
            extent: (4, 4),
            ..Canvas::default()
        };
        draw(
            &mut cv,
            DrawCmd::PSet {
                at: (3.0, 3.0),
                color: Some(0xFF0000),
            },
        );
        let id = cv.raster.id;
        cv.extent = (10, 8);
        draw(
            &mut cv,
            DrawCmd::PSet {
                at: (9.5, 7.2),
                color: Some(0x00FF00),
            },
        );
        assert_eq!((cv.raster.width, cv.raster.height), (10, 8));
        assert_eq!(cv.raster.pixel(3, 3), Some(0xFF0000));
        assert_eq!(cv.raster.pixel(9, 7), Some(0x00FF00));
        assert_eq!(cv.raster.id, id, "the same bitmap, grown");
    }

    fn near(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn an_ellipse_is_a_closed_ring_and_an_arc_stops_where_it_ends() {
        let ring = ellipse_points((10.0, 10.0), 5.0, 1.0, None, (false, false));
        assert!(near(ring[0], (15.0, 10.0)), "starts at 3 o'clock");
        assert!(near(*ring.last().unwrap(), ring[0]), "and closes");
        // a quarter arc, counter-clockwise: 3 o'clock to 12 o'clock
        let quarter = Some((0.0, std::f32::consts::FRAC_PI_2));
        let arc = ellipse_points((10.0, 10.0), 5.0, 1.0, quarter, (false, false));
        assert!(near(*arc.last().unwrap(), (10.0, 5.0)));
        // the spokes add the centre at the ends
        let pie = ellipse_points((10.0, 10.0), 5.0, 1.0, Some((0.0, 1.0)), (true, true));
        assert_eq!((pie[0], *pie.last().unwrap()), ((10.0, 10.0), (10.0, 10.0)));
        // an aspect under 1 squashes the height, over 1 the width
        let flat = ellipse_points((0.0, 0.0), 10.0, 0.5, None, (false, false));
        let top = flat.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        assert!((top + 5.0).abs() < 0.2, "{}", top);
        let tall = ellipse_points((0.0, 0.0), 10.0, 2.0, None, (false, false));
        let right = tall.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        assert!((right - 5.0).abs() < 0.2, "{}", right);
    }
}
