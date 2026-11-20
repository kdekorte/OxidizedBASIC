//! Slider and ProgressBar: range controls like the scrollbars. A Slider is
//! dragged by the user; a ProgressBar only shows the program's Value.

use crate::form::ControlKind;
use crate::form::{ControlSpec, FormSpec};
use crate::host::{FormHost, UserEdit};
use crate::Rect;
use std::cell::RefCell;
use std::rc::Rc;

fn host() -> Rc<RefCell<FormHost>> {
    let mut f = FormSpec::new("Form1", "Range", 400.0, 300.0);
    let mut s = ControlSpec::new("Slider1", ControlKind::Slider, Rect::new(0.0, 0.0, 160.0, 40.0));
    s.scroll_min = 0;
    s.scroll_max = 100;
    f.control(s);
    let mut p = ControlSpec::new("Progress1", ControlKind::ProgressBar, Rect::new(0.0, 60.0, 200.0, 20.0));
    p.scroll_min = 0;
    p.scroll_max = 50;
    f.control(p);
    Rc::new(RefCell::new(FormHost::new(f)))
}

#[test]
fn a_slider_has_a_range_a_value_and_an_orientation() {
    let h = host();
    let out = crate::host::tests::run(
        "Print Slider1.Min; Slider1.Max; Slider1.Value; Slider1.Orientation\n\
         Slider1.Value = 40\nSlider1.Orientation = 1\nPrint Slider1.Value; Slider1.Orientation\n",
        &h,
    )
    .unwrap();
    assert_eq!(out, " 0  100  0  0 \n 40  1 \n");
}

#[test]
fn a_progress_bar_is_set_by_the_program_within_its_range() {
    let h = host();
    crate::host::tests::run("Progress1.Value = 30\nPrint Progress1.Value\n", &h).unwrap();
    let e = crate::host::tests::run("Progress1.Value = 60\n", &h).unwrap_err();
    assert!(e.contains("380"), "{}", e);
}

#[test]
fn a_user_drag_moves_a_slider_but_never_a_progress_bar() {
    let h = host();
    h.borrow_mut().apply_edit(UserEdit::Scroll { control: 0, value: 75 });
    assert_eq!(h.borrow().state[0].value, 75, "the slider takes the drag");
    h.borrow_mut().apply_edit(UserEdit::Scroll { control: 1, value: 25 });
    assert_eq!(h.borrow().state[1].value, 0, "the progress bar does not");
}

#[test]
fn a_slider_raises_change_when_its_value_moves() {
    let h = host();
    // the raised events are taken as each statement ends, so the rule is
    // checked on the change itself
    let before = h.borrow().watched(0);
    h.borrow_mut().form.controls[0].value = 0;
    h.borrow_mut().state[0].value = 10;
    h.borrow_mut().raise_changes(0, &before);
    assert!(
        h.borrow().raised_handlers().iter().any(|e| e == "Slider1_Change"),
        "{:?}",
        h.borrow().raised_handlers()
    );
}
