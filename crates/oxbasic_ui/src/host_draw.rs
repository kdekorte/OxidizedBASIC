//! Drawing on a PictureBox (and the form): `Line`, `PSet`, `Circle`, `Cls`,
//! `CurrentX` / `CurrentY`, and the picture's own size for `AutoSize`.

use super::host::{err, FormHost, Target};
use crate::ControlKind;
use oxbasic_runtime::{DrawCmd, ExecResult, ObjectRef, Variant};

/// The pixel size of the image file at `path`, read from its header.
pub fn picture_size(path: &std::path::Path) -> Option<(f32, f32)> {
    let s = imagesize::size(path).ok()?;
    Some((s.width as f32, s.height as f32))
}

impl FormHost {
    /// The control a drawing statement targets: the named PictureBox
    /// (`Picture1`, `Pic(2)`, or `Form2.Picture1` once the host has
    /// stripped the form), or `None` for the form itself.
    fn canvas_index(&self, object: Option<&ObjectRef>) -> ExecResult<Option<usize>> {
        let Some(o) = object else { return Ok(None) };
        match self.target(o)? {
            Target::Form => Ok(None),
            Target::Control(i) if self.form.controls[i].kind == ControlKind::PictureBox => {
                Ok(Some(i))
            }
            // drawing is only for a PictureBox (and the form), as in VB
            Target::Control(_) => err(438, "object doesn't support this property or method"),
        }
    }

    pub(crate) fn draw_on(&mut self, object: Option<&ObjectRef>, cmd: DrawCmd) -> ExecResult<()> {
        let Some(i) = self.canvas_index(object)? else {
            // the form has no surface here yet: its controls cover it
            return err(438, "object doesn't support this property or method");
        };
        let fore = self.form.controls[i]
            .fore_color
            .unwrap_or(super::host::DEFAULT_FORE);
        let resolve = |vb: u32| super::host::from_vb_color(vb as i32 as i64);
        // the bitmap behind `PSet` covers the control's inside
        let r = self.form.controls[i].rect;
        self.state[i].canvas.extent = (r.width.max(0.0) as usize, r.height.max(0.0) as usize);
        self.state[i].canvas.draw(cmd, fore, resolve)?;
        self.changed = true;
        self.touched[i] = true;
        Ok(())
    }

    pub(crate) fn current_of(&self, object: Option<&ObjectRef>) -> (f32, f32) {
        match self.canvas_index(object) {
            Ok(Some(i)) => self.state[i].canvas.current,
            _ => (0.0, 0.0),
        }
    }

    /// `Cls`: clear what was drawn on PictureBox `i`.
    pub(crate) fn cls(&mut self, i: usize) -> ExecResult<Variant> {
        self.state[i].canvas.clear();
        self.changed = true;
        self.touched[i] = true;
        Ok(Variant::Empty)
    }

    /// `AutoSize`: a PictureBox takes its picture's size (plus its border).
    /// Called when the picture or the setting changes.
    pub(crate) fn auto_size_to_picture(&mut self, i: usize, base: Option<&std::path::Path>) {
        let c = &self.form.controls[i];
        if !(c.kind == ControlKind::PictureBox && c.auto_size) || c.picture.is_empty() {
            return;
        }
        let path = match base {
            Some(dir) => dir.join(&c.picture),
            None => std::path::PathBuf::from(&c.picture),
        };
        if let Some((w, h)) = picture_size(&path) {
            // VB's 2-pixel border on each side of the picture
            self.form.controls[i].rect.width = w + 4.0;
            self.form.controls[i].rect.height = h + 4.0;
        }
    }
}
