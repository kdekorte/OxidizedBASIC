//! TabControl: a page is a container. A control on page n shows (and takes
//! input) only while the TabControl's Tab is n; Tab is range-checked and its
//! change raises Click.

use crate::form::{ControlKind, ControlSpec, FormSpec};
use crate::host::FormHost;
use crate::Rect;
use std::cell::RefCell;
use std::rc::Rc;

/// Form1 with a TabControl (two pages: General, Advanced) holding a TextBox
/// on page 0 and a CheckBox on page 1.
fn host() -> Rc<RefCell<FormHost>> {
    let mut f = FormSpec::new("Form1", "Tabs", 400.0, 300.0);
    let mut t = ControlSpec::new(
        "Tab1",
        ControlKind::TabControl,
        Rect::new(0.0, 0.0, 320.0, 200.0),
    );
    t.tabs = vec!["General".into(), "Advanced".into()];
    f.control(t);
    let mut text = ControlSpec::new(
        "Text1",
        ControlKind::TextBox,
        Rect::new(10.0, 40.0, 120.0, 24.0),
    );
    text.container = Some("Tab1".into());
    text.page = 0;
    f.control(text);
    let mut check = ControlSpec::new(
        "Check1",
        ControlKind::CheckBox,
        Rect::new(10.0, 40.0, 120.0, 22.0),
    );
    check.container = Some("Tab1".into());
    check.page = 1;
    f.control(check);
    Rc::new(RefCell::new(FormHost::new(f)))
}

#[test]
fn only_the_selected_pages_controls_are_shown() {
    let h = host();
    let h = h.borrow();
    // page 0 is selected at first: the General control shows, the other doesn't
    assert!(h.form.shown(1), "Text1 is on the selected page");
    assert!(!h.form.shown(2), "Check1 is on another page");
}

#[test]
fn switching_the_tab_shows_the_other_page() {
    let h = host();
    crate::host::tests::run("Tab1.Tab = 1\n", &h).unwrap();
    let h = h.borrow();
    assert!(!h.form.shown(1), "Text1 is hidden on page 1");
    assert!(h.form.shown(2), "Check1 shows on page 1");
}

#[test]
fn a_hidden_page_takes_no_input() {
    let h = host();
    assert!(h.borrow().form.usable(1));
    assert!(!h.borrow().form.usable(2));
}

#[test]
fn tab_is_range_checked_and_reads_back() {
    let h = host();
    let out = crate::host::tests::run("Tab1.Tab = 1\nPrint Tab1.Tab; Tab1.Tabs\n", &h).unwrap();
    assert_eq!(out, " 1  2 \n");
    let e = crate::host::tests::run("Tab1.Tab = 2\n", &h).unwrap_err();
    assert!(e.contains("380"), "{}", e);
}

#[test]
fn a_tab_change_raises_click() {
    let h = host();
    let before = h.borrow().watched(0);
    h.borrow_mut().state[0].tab = 1;
    h.borrow_mut().raise_changes(0, &before);
    assert!(
        h.borrow()
            .raised_handlers()
            .iter()
            .any(|e| e == "Tab1_Click"),
        "{:?}",
        h.borrow().raised_handlers()
    );
}

/// The program's `Tab` reaches the window's copy of the form: the strip
/// (its state) and the pages (the form) agree, as a window that was already
/// open shows them after the program's change.
#[test]
fn a_program_tab_change_reaches_an_open_windows_strip() {
    let h = three_page_host();
    // the window opened on General, as the user sees it
    let mut form = h.borrow().form.clone();
    let mut state = h.borrow().state.clone();
    assert_eq!(state[0].tab, 0);
    crate::host::tests::run("Tab1.Tab = 2\n", &h).unwrap();
    let snap = h.borrow_mut().take_changes().unwrap();
    snap.apply_to(&mut form, &mut state);
    assert_eq!(state[0].tab, 2, "the strip shows the program's page");
    assert_eq!(form.controls[0].tab, 2, "and the pages agree with it");
    assert!(form.shown(3), "Adv (page 2) shows");
}

#[test]
fn a_press_on_the_strip_picks_the_tab_under_it() {
    use crate::gpui_controls::{tab_at, tab_width};
    let tabs: Vec<String> = vec!["General".into(), "Options".into(), "Advanced".into()];
    let first = tab_width("General");
    assert_eq!(tab_at(&tabs, 0.0), Some(0));
    assert_eq!(tab_at(&tabs, first - 1.0), Some(0));
    assert_eq!(tab_at(&tabs, first + 1.0), Some(1));
    let past_last = tab_width("General") + tab_width("Options") + tab_width("Advanced");
    assert_eq!(tab_at(&tabs, past_last + 1.0), None, "past the last tab");
    assert_eq!(tab_at(&tabs, -1.0), None, "left of the control");
}

#[test]
fn a_tab_press_selects_that_page_and_raises_click() {
    let h = host();
    h.borrow_mut().apply_edit(crate::host::UserEdit::Tab {
        control: 0,
        tab: 1,
    });
    assert_eq!(h.borrow().state[0].tab, 1);
    assert!(h
        .borrow()
        .raised_handlers()
        .iter()
        .any(|e| e == "Tab1_Click"));
}

#[test]
fn a_tab_press_on_the_selected_or_a_missing_page_does_nothing() {
    let h = host();
    h.borrow_mut().apply_edit(crate::host::UserEdit::Tab {
        control: 0,
        tab: 0,
    });
    h.borrow_mut().apply_edit(crate::host::UserEdit::Tab {
        control: 0,
        tab: 9,
    });
    assert_eq!(h.borrow().state[0].tab, 0);
    assert!(h.borrow().raised_handlers().is_empty());
}

/// Three pages, as the showcase's Tabs form has them: a control on each.
fn three_page_host() -> Rc<RefCell<FormHost>> {
    let mut f = FormSpec::new("Form1", "Tabs", 400.0, 300.0);
    let mut t = ControlSpec::new(
        "Tab1",
        ControlKind::TabControl,
        Rect::new(0.0, 0.0, 320.0, 200.0),
    );
    t.tabs = vec!["General".into(), "Options".into(), "Advanced".into()];
    f.control(t);
    for (name, page) in [("Lvl", 0u8), ("Opt", 1), ("Adv", 2)] {
        let mut c = ControlSpec::new(name, ControlKind::Label, Rect::new(10.0, 40.0, 120.0, 24.0));
        c.container = Some("Tab1".into());
        c.page = page;
        f.control(c);
    }
    Rc::new(RefCell::new(FormHost::new(f)))
}

#[test]
fn the_window_snapshot_shows_the_page_the_program_selected() {
    let h = three_page_host();
    crate::host::tests::run("Tab1.Tab = 2\n", &h).unwrap();
    let snap = h.borrow().snapshot();
    assert_eq!(snap.state[0].tab, 2, "the program's Tab is page 3");
    assert_eq!(snap.form.controls[0].tab, 2, "the form's copy agrees");
    assert!(!snap.form.shown(1), "Lvl hidden");
    assert!(!snap.form.shown(2), "Opt hidden");
    assert!(snap.form.shown(3), "Adv shows on the selected page");
}
