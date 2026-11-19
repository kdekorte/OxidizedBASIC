//! The showcase's control array, timer, second form and `End`: the parts
//! of `examples/showcase` that cross controls and forms.

use super::debug_session::{DebugCommand, DebugEvent};
use super::showcase_run::{caption, Run};

#[test]
fn the_control_array_handler_learns_which_button_was_pressed() {
    let run = Run::start();
    let swatch = |i: i32| run.control(0, "cmdSwatch", Some(i));
    let snap = run.call(0, "cmdSwatch_Click", &[1]);
    assert_eq!(caption(&snap, "lblStatus"), "Swatch 2 chosen (Index = 1)");
    // the chosen button is bold and disabled, the others are the opposite
    for (i, chosen) in [(0, false), (1, true), (2, false)] {
        let c = &snap.form.controls[swatch(i)];
        assert_eq!(c.font.bold, chosen, "Swatch {} bold", i + 1);
        assert_eq!(c.enabled, !chosen, "Swatch {} enabled", i + 1);
    }
    // and it picked the matching colour in the combo box
    let combo = run.control(0, "cboColour", None);
    assert_eq!(snap.state[combo].list_index, Some(1));
    assert_eq!(snap.state[combo].text, "Green");

    let snap = run.call(0, "cmdSwatch_Click", &[2]);
    assert_eq!(caption(&snap, "lblStatus"), "Swatch 3 chosen (Index = 2)");
    assert!(!snap.form.controls[swatch(2)].enabled);
    assert!(
        snap.form.controls[swatch(1)].enabled,
        "Swatch 2 is free again"
    );
}

#[test]
fn the_timer_starts_stopped_and_the_button_toggles_it() {
    let run = Run::start();
    let timer = run.control(0, "tmrClock", None);
    assert!(!run.first.form.controls[timer].enabled, "off at the start");

    let snap = run.call(0, "cmdClock_Click", &[]);
    assert!(snap.form.controls[timer].enabled);
    assert_eq!(caption(&snap, "lblClock"), "Clock: running");

    // each tick counts a second
    let snap = run.call(0, "tmrClock_Timer", &[]);
    assert_eq!(caption(&snap, "lblClock"), "Clock: 1s");
    let snap = run.call(0, "tmrClock_Timer", &[]);
    assert_eq!(caption(&snap, "lblClock"), "Clock: 2s");

    let snap = run.call(0, "cmdClock_Click", &[]);
    assert!(!snap.form.controls[timer].enabled);
    assert_eq!(caption(&snap, "lblClock"), "Clock: stopped at 2s");
}

#[test]
fn about_opens_a_second_window_that_knows_what_the_first_did() {
    let run = Run::start();
    run.call(0, "cmdGreet_Click", &[]);
    run.call(0, "cmdGreet_Click", &[]);
    run.session
        .send(DebugCommand::Call(0, "cmdAbout_Click".into(), vec![]));
    let seen = run.until(|e| matches!(e, DebugEvent::FormShown(s) if s.form_index == 1));
    let Some(DebugEvent::FormShown(about)) = seen.last() else {
        unreachable!()
    };
    // Form1 wrote to frmAbout's label before showing it, and frmAbout's own
    // Form_Load wrote its note
    assert_eq!(caption(about, "lblVersion"), "Greetings so far: 2");
    assert_eq!(
        caption(about, "lblNote"),
        "Form_Load ran before this window opened."
    );
}

#[test]
fn the_about_form_hides_with_its_state_and_closes_for_good() {
    let run = Run::start();
    run.session
        .send(DebugCommand::Call(0, "cmdAbout_Click".into(), vec![]));
    run.until(|e| matches!(e, DebugEvent::FormShown(s) if s.form_index == 1));

    // Hide closes the window; About… shows it again as it was
    run.session
        .send(DebugCommand::Call(1, "cmdHide_Click".into(), vec![]));
    run.until(|e| matches!(e, DebugEvent::FormHidden(1)));
    run.session
        .send(DebugCommand::Call(0, "cmdAbout_Click".into(), vec![]));
    run.until(|e| matches!(e, DebugEvent::FormShown(s) if s.form_index == 1));

    // Close unloads it
    run.session
        .send(DebugCommand::Call(1, "cmdClose_Click".into(), vec![]));
    run.until(|e| matches!(e, DebugEvent::FormClosed(1)));
}

#[test]
fn quit_ends_the_whole_program() {
    let run = Run::start();
    run.session
        .send(DebugCommand::Call(0, "cmdQuit_Click".into(), vec![]));
    let seen = run.until(|e| matches!(e, DebugEvent::Finished(_)));
    let Some(DebugEvent::Finished(msg)) = seen.last() else {
        unreachable!()
    };
    assert_eq!(msg, "Program ended");
}
