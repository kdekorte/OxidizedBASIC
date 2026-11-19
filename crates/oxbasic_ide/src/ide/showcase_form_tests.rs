//! The showcase's first form, control by control: each handler in
//! `examples/showcase/Form1.bas` is run for real and its effect checked.

use super::showcase_run::{caption, Run};

#[test]
fn form_load_runs_before_the_window_opens() {
    let run = Run::start();
    assert_eq!(
        caption(&run.first, "lblStatus"),
        "Form_Load ran: the combo box starts on Red"
    );
}

#[test]
fn greet_uses_the_text_the_options_and_the_checkbox() {
    let run = Run::start();
    // formal is the default option
    run.type_into(0, "txtName", "Ada");
    let snap = run.call(0, "cmdGreet_Click", &[]);
    assert_eq!(caption(&snap, "lblGreeting"), "Good day, Ada.");
    assert_eq!(caption(&snap, "lblStatus"), "Greeted 1 time(s)");

    // Casual: picking an option button clears the other
    run.set_value(0, "optCasual", 1);
    let snap = run.call(0, "optCasual_Click", &[]);
    assert_eq!(caption(&snap, "lblStatus"), "Casual style");
    let snap = run.call(0, "cmdGreet_Click", &[]);
    assert_eq!(caption(&snap, "lblGreeting"), "Hey Ada!");

    // the checkbox shouts it
    run.set_value(0, "chkShout", 1);
    let snap = run.call(0, "chkShout_Click", &[]);
    assert_eq!(caption(&snap, "lblStatus"), "Shout is on");
    let snap = run.call(0, "cmdGreet_Click", &[]);
    assert_eq!(caption(&snap, "lblGreeting"), "HEY ADA!");
    assert_eq!(caption(&snap, "lblStatus"), "Greeted 3 time(s)");
}

#[test]
fn an_empty_name_greets_a_stranger_and_typing_is_counted() {
    let run = Run::start();
    let snap = run.call(0, "cmdGreet_Click", &[]);
    assert_eq!(caption(&snap, "lblGreeting"), "Good day, stranger.");
    run.type_into(0, "txtName", "Bo");
    let snap = run.call(0, "txtName_Change", &[]);
    assert_eq!(caption(&snap, "lblStatus"), "Typing: 2 character(s)");
}

#[test]
fn the_combo_box_colours_the_picture_box() {
    let run = Run::start();
    let swatch = run.control(0, "picSwatch", None);
    for (row, colour) in [(0, 0xFF0000), (2, 0x0000FF), (3, 0xFFC800)] {
        run.pick(0, "cboColour", row);
        let snap = run.call(0, "cboColour_Click", &[]);
        assert_eq!(
            snap.form.controls[swatch].color,
            Some(colour),
            "row {}",
            row
        );
    }
    let snap = run.call(0, "cboColour_Click", &[]);
    assert_eq!(caption(&snap, "lblStatus"), "Colour: Gold");
}

#[test]
fn the_list_box_adds_removes_and_clears_with_the_module_helpers() {
    let run = Run::start();
    let list = run.control(0, "lstFruit", None);
    let snap = run.call(0, "cmdAdd_Click", &[]);
    assert_eq!(snap.form.controls[list].items.len(), 4);
    assert_eq!(snap.form.controls[list].items[3], "Date");
    // Module1's Plural and Describe are shared by the form
    assert_eq!(
        caption(&snap, "lblStatus"),
        "4 fruits in the basket (several)"
    );

    // the new row is selected; picking it and removing it
    let snap = run.call(0, "cmdRemove_Click", &[]);
    assert_eq!(snap.form.controls[list].items.len(), 3);
    assert_eq!(caption(&snap, "lblStatus"), "3 fruits left");

    // nothing selected now: Remove explains instead of failing
    let snap = run.call(0, "cmdRemove_Click", &[]);
    assert_eq!(
        caption(&snap, "lblStatus"),
        "Select a fruit to remove first"
    );
}

#[test]
fn clearing_the_list_and_picking_a_row() {
    let run = Run::start();
    run.pick(0, "lstFruit", 1);
    let snap = run.call(0, "lstFruit_Click", &[]);
    assert_eq!(caption(&snap, "lblStatus"), "You picked Banana");
    let list = run.control(0, "lstFruit", None);
    let snap = run.call(0, "cmdClear_Click", &[]);
    assert!(snap.form.controls[list].items.is_empty());
    assert_eq!(caption(&snap, "lblStatus"), "The basket is empty");
}
