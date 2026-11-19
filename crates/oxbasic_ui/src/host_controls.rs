//! The run-time methods that add, remove, move and restack a form's
//! controls (`Load Cmd(3)`, `Unload Cmd(3)`, `Move`, `ZOrder`) and their
//! `Container`.

use super::host::{err, FormHost};
use crate::{ControlSpec, Rect};
use oxbasic_runtime::{ExecResult, Variant};

/// `Frame1(2)` as (`Frame1`, `Some(2)`); `Frame1` as (`Frame1`, `None`).
pub fn split_element(name: &str) -> (&str, Option<i32>) {
    if let Some(open) = name.find('(') {
        if let Some(n) = name[open + 1..]
            .strip_suffix(')')
            .and_then(|n| n.trim().parse().ok())
        {
            return (&name[..open], Some(n));
        }
    }
    (name, None)
}

/// How a control is named as a container: `Frame1`, or `Frame1(2)`.
pub fn container_key(c: &ControlSpec) -> String {
    match c.index {
        Some(n) => format!("{}({})", c.name, n),
        None => c.name.clone(),
    }
}

/// Where index `f` ends up when the element at `from` moves to `to`.
fn moved_index(f: usize, from: usize, to: usize) -> usize {
    if f == from {
        to
    } else if from < to && f > from && f <= to {
        f - 1
    } else if to < from && f >= to && f < from {
        f + 1
    } else {
        f
    }
}

impl FormHost {
    /// `Load Cmd(n)`: a new element of control array `name`, a copy of its
    /// lowest-indexed element as designed (VB copies the design-time
    /// properties), hidden, with the text of a fresh control. 360 if the
    /// element is loaded already, 343 if `name` isn't an array.
    pub(crate) fn load_element(&mut self, name: &str, n: i32) -> ExecResult<Variant> {
        if !(0..=32767).contains(&n) {
            return err(340, "Control array element does not exist");
        }
        if self.control_index(name, Some(n)).is_some() {
            return err(360, "Object already loaded");
        }
        let first = self
            .form
            .controls
            .iter()
            .enumerate()
            .filter(|(_, c)| c.name.eq_ignore_ascii_case(name) && c.index.is_some())
            .min_by_key(|(_, c)| c.index)
            .map(|(i, _)| i);
        let Some(first) = first else {
            return err(343, "Object not an array");
        };
        let mut c = self.form.controls[first].clone();
        c.index = Some(n);
        c.visible = false;
        c.tab_index = None;
        // after the last element of its array, so arrays stay together
        let at = self
            .form
            .controls
            .iter()
            .rposition(|c| c.name.eq_ignore_ascii_case(name))
            .map_or(self.form.controls.len(), |p| p + 1);
        self.state.insert(at, crate::host::ControlState::new(&c));
        self.form.controls.insert(at, c);
        self.touched.insert(at, true);
        // the focus is an index into the controls: keep it on its control
        if let Some(f) = self.focus.as_mut() {
            if *f >= at {
                *f += 1;
            }
        }
        self.changed_all();
        Ok(Variant::Empty)
    }

    /// `Unload Cmd(n)`: remove element `n`. VB won't unload an element that
    /// was on the form at design time (362). What it contained moves to its
    /// own container.
    pub(crate) fn unload_element(&mut self, name: &str, n: i32) -> ExecResult<Variant> {
        let Some(i) = self.control_index(name, Some(n)) else {
            return err(340, "Control array element does not exist");
        };
        let designed = self
            .designed
            .iter()
            .any(|(d, x)| d.eq_ignore_ascii_case(name) && *x == Some(n));
        if designed {
            return err(362, "Can't unload controls created at design time");
        }
        let key = container_key(&self.form.controls[i]);
        let parent = self.form.controls[i].container.clone();
        for c in self.form.controls.iter_mut() {
            if c.container
                .as_deref()
                .is_some_and(|k| k.eq_ignore_ascii_case(&key))
            {
                c.container = parent.clone();
            }
        }
        self.form.controls.remove(i);
        self.state.remove(i);
        self.touched.remove(i);
        self.focus = match self.focus {
            Some(f) if f == i => None,
            Some(f) if f > i => Some(f - 1),
            other => other,
        };
        self.changed_all();
        Ok(Variant::Empty)
    }
}

impl FormHost {
    /// Every control changed (one was added, removed or restacked): the
    /// window takes the whole form again.
    pub(crate) fn changed_all(&mut self) {
        self.changed = true;
        self.touched = vec![true; self.form.controls.len()];
    }

    /// `Move Left [, Top [, Width [, Height]]]`: those given change. A
    /// size change raises `Resize` as setting `Width` would.
    pub(crate) fn move_control(&mut self, i: usize, args: &[Variant]) -> ExecResult<Variant> {
        if args.is_empty() || args.len() > 4 {
            return err(450, "wrong number of arguments");
        }
        let r = self.form.controls[i].rect;
        let mut vals = [r.left, r.top, r.width, r.height];
        for (slot, a) in vals.iter_mut().zip(args) {
            // a skipped optional argument arrives as Empty
            if !matches!(a, Variant::Empty) {
                *slot = a.as_f64()? as f32;
            }
        }
        if vals[2] <= 0.0 || vals[3] <= 0.0 {
            return err(380, "Invalid property value");
        }
        let before = self.watched(i);
        self.form.controls[i].rect = Rect::new(vals[0], vals[1], vals[2], vals[3]);
        self.changed = true;
        self.touched[i] = true;
        self.raise_changes(i, &before);
        Ok(Variant::Empty)
    }

    /// `ZOrder [position]`: 0 (`vbBringToFront`, the default) draws the
    /// control over the others, 1 (`vbSendToBack`) under them. Controls are
    /// drawn in their order, so it moves to the end or the start; the tab
    /// order is numbered first so restacking leaves it as it was.
    pub(crate) fn zorder(&mut self, i: usize, args: &[Variant]) -> ExecResult<Variant> {
        let pos = match args.first() {
            None | Some(Variant::Empty) => 0,
            Some(v) => v.as_i64()?,
        };
        if pos != 0 && pos != 1 {
            return err(5, "Invalid procedure call or argument");
        }
        let to = if pos == 0 {
            self.form.controls.len() - 1
        } else {
            0
        };
        if to != i {
            let order = self.form.tab_order();
            for (place, &c) in order.iter().enumerate() {
                self.form.controls[c].tab_index = Some(place as i32);
            }
            let c = self.form.controls.remove(i);
            let s = self.state.remove(i);
            self.form.controls.insert(to, c);
            self.state.insert(to, s);
            self.focus = self.focus.map(|f| moved_index(f, i, to));
            self.changed_all();
        }
        Ok(Variant::Empty)
    }

    /// `Set X.Container = Frame1`: move control `i` into a Frame or
    /// PictureBox, or back onto the form (`Me`, the form's name). As in VB,
    /// its `Left` and `Top` stay the same numbers, now measured from the
    /// new container. 380 for a control that can't contain, or for a
    /// container inside `i` itself.
    pub(crate) fn set_container(&mut self, i: usize, value: &Variant) -> ExecResult<()> {
        let name = value.as_string()?;
        let target = if self.is_form(&name) {
            None
        } else {
            let (base, index) = split_element(&name);
            let Some(j) = self.control_index(base, index) else {
                return err(424, "object required");
            };
            if !self.form.controls[j].kind.is_container() || j == i || self.form.contains(i, j) {
                return err(380, "Invalid property value");
            }
            Some(container_key(&self.form.controls[j]))
        };
        self.form.controls[i].container = target;
        Ok(())
    }
}
