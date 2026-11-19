//! The events a user's input raises in a running program, driven through
//! the IDE's live window with real mouse and key events. Each test's
//! program logs every event it gets to `Label1`, so the log shows the
//! order VB would raise them in.

use super::toolbox_tests::{ide_window, open_live_window, settle};
use super::*;
use std::sync::mpsc::channel;

/// Form1: `Label1` (the log), `Command1`, `Text1`, `Text2`, `Picture1`.
fn event_form() -> FormSpec {
    let mut f = FormSpec::new("Form1", "Events", 400.0, 300.0);
    f.label("Label1", "", Rect::new(8.0, 8.0, 380.0, 20.0));
    f.button("Command1", "Go", Rect::new(16.0, 40.0, 96.0, 28.0));
    f.text_box("Text1", "", Rect::new(16.0, 80.0, 160.0, 24.0));
    f.text_box("Text2", "", Rect::new(16.0, 112.0, 160.0, 24.0));
    f.control(ControlSpec::new(
        "Picture1",
        ControlKind::PictureBox,
        Rect::new(200.0, 40.0, 120.0, 100.0),
    ));
    f
}

/// The live window of a program whose handlers call `Log` to record each
/// event, opened from the IDE with an empty log.
struct Run<'a> {
    ide: Entity<Ide>,
    session: DebugSession,
    rx: std::sync::mpsc::Receiver<DebugEvent>,
    handle: WindowHandle<FormRunner>,
    win: gpui::VisualTestContext,
    cx: &'a mut gpui::VisualTestContext,
}

impl<'a> Run<'a> {
    fn start(form: FormSpec, src: &str, cx: &'a mut gpui::TestAppContext) -> Run<'a> {
        let (ide, cx) = ide_window(cx);
        let program = format!(
            "Sub Log(s As String)\n  Label1.Caption = Label1.Caption & s & \";\"\nEnd Sub\n\
             Sub ClearLog()\n  Label1.Caption = \"\"\nEnd Sub\n{}",
            src
        );
        let (tx, rx) = channel();
        let session = DebugSession::start_with_form(
            program,
            Some(form.clone()),
            Arc::new(Mutex::new(HashSet::new())),
            tx,
        );
        session.send(DebugCommand::Run);
        let (handle, win) = open_live_window(&ide, form, &session, &rx, cx);
        let mut run = Run {
            ide,
            session,
            rx,
            handle,
            win,
            cx,
        };
        run.settle();
        run.clear();
        run
    }

    /// Let the program run what the window sent, and show the result. The
    /// window's own pending work (an activation, a resize) goes first, so
    /// the events it raises are in what the program is given time for.
    fn settle(&mut self) {
        self.win.run_until_parked();
        settle(&self.ide, &self.rx, &mut self.win, self.cx);
    }

    /// Empty the log (the program's, and so the window's).
    fn clear(&mut self) {
        self.session
            .send(DebugCommand::Call(0, "ClearLog".into(), vec![]));
        self.settle();
    }

    /// What the program has logged since the last clear.
    fn log(&self) -> String {
        self.handle
            .read_with(&self.win, |r, _| r.form.controls[0].caption.clone())
            .unwrap()
    }

    fn bounds(&mut self, sel: &'static str) -> gpui::Bounds<gpui::Pixels> {
        self.win
            .debug_bounds(sel)
            .unwrap_or_else(|| panic!("{}", sel))
    }

    fn down(&mut self, p: gpui::Point<gpui::Pixels>, b: gpui::MouseButton, clicks: usize) {
        self.win.simulate_event(gpui::MouseDownEvent {
            button: b,
            position: p,
            modifiers: gpui::Modifiers::none(),
            click_count: clicks,
            first_mouse: false,
        });
    }

    fn up(&mut self, p: gpui::Point<gpui::Pixels>, b: gpui::MouseButton) {
        self.win.simulate_event(gpui::MouseUpEvent {
            button: b,
            position: p,
            modifiers: gpui::Modifiers::none(),
            click_count: 1,
        });
    }

    /// A move with Shift held (so the program sees `Shift` = 1).
    fn moved(&mut self, p: gpui::Point<gpui::Pixels>, held: Option<gpui::MouseButton>) {
        self.win.simulate_event(gpui::MouseMoveEvent {
            position: p,
            pressed_button: held,
            modifiers: gpui::Modifiers::shift(),
        });
    }

    fn stop(self) {
        self.session.send(DebugCommand::Stop);
    }
}

/// A point `dx`, `dy` into `r`.
fn at(r: gpui::Bounds<gpui::Pixels>, dx: f32, dy: f32) -> gpui::Point<gpui::Pixels> {
    gpui::point(r.origin.x + px(dx), r.origin.y + px(dy))
}

/// A mouse handler that logs `name`, its button, shift and position.
fn mouse_handler(object: &str, event: &str, name: &str) -> String {
    format!(
        "Sub {}_{}(Button As Integer, Shift As Integer, X As Single, Y As Single)\n\
         \x20 Log \"{} \" & Button & \" \" & Shift & \" \" & X & \",\" & Y\nEnd Sub\n",
        object, event, name
    )
}

/// A handler without arguments that logs `name`.
fn plain_handler(object: &str, event: &str, name: &str) -> String {
    format!("Sub {}_{}()\n  Log \"{}\"\nEnd Sub\n", object, event, name)
}

#[gpui::test]
fn a_click_is_got_focus_mouse_down_click_mouse_up_with_vbs_arguments(
    cx: &mut gpui::TestAppContext,
) {
    let src = [
        mouse_handler("Command1", "MouseDown", "down"),
        mouse_handler("Command1", "MouseUp", "up"),
        plain_handler("Command1", "Click", "click"),
        plain_handler("Command1", "GotFocus", "got"),
    ]
    .concat();
    let mut run = Run::start(event_form(), &src, cx);
    let b = run.bounds("run-Command1");
    // pressed 10,5 into the button: X and Y are the button's own
    run.down(at(b, 10.0, 5.0), gpui::MouseButton::Left, 1);
    run.up(at(b, 10.0, 5.0), gpui::MouseButton::Left);
    run.settle();
    assert_eq!(run.log(), "got;down 1 0 10,5;click;up 1 0 10,5;");

    // a right press: Button 2 and no Click; the focus is already there
    run.clear();
    run.down(at(b, 3.0, 4.0), gpui::MouseButton::Right, 1);
    run.up(at(b, 3.0, 4.0), gpui::MouseButton::Right);
    run.settle();
    assert_eq!(run.log(), "down 2 0 3,4;up 2 0 3,4;");
    run.stop();
}

#[gpui::test]
fn the_pressed_control_captures_the_mouse_and_a_release_elsewhere_doesnt_click(
    cx: &mut gpui::TestAppContext,
) {
    let src = [
        mouse_handler("Picture1", "MouseMove", "move"),
        mouse_handler("Picture1", "MouseUp", "up"),
        plain_handler("Picture1", "Click", "click"),
    ]
    .concat();
    let mut run = Run::start(event_form(), &src, cx);
    let p = run.bounds("run-Picture1");
    // a move over the picture with no button held
    run.moved(at(p, 20.0, 30.0), None);
    // pressed on it and dragged off to its left: the picture still gets
    // the move (X is negative) and the release, but no Click
    run.down(at(p, 20.0, 30.0), gpui::MouseButton::Left, 1);
    run.moved(at(p, -50.0, 30.0), Some(gpui::MouseButton::Left));
    run.up(at(p, -50.0, 30.0), gpui::MouseButton::Left);
    run.settle();
    assert_eq!(run.log(), "move 0 1 20,30;move 1 1 -50,30;up 1 0 -50,30;");
    run.stop();
}

#[gpui::test]
fn a_double_click_raises_dbl_click_in_vbs_order(cx: &mut gpui::TestAppContext) {
    let src = [
        mouse_handler("Picture1", "MouseDown", "down"),
        mouse_handler("Picture1", "MouseUp", "up"),
        plain_handler("Picture1", "Click", "click"),
        plain_handler("Picture1", "DblClick", "dbl"),
    ]
    .concat();
    let mut run = Run::start(event_form(), &src, cx);
    let c = at(run.bounds("run-Picture1"), 10.0, 10.0);
    run.down(c, gpui::MouseButton::Left, 1);
    run.up(c, gpui::MouseButton::Left);
    run.down(c, gpui::MouseButton::Left, 2);
    run.up(c, gpui::MouseButton::Left);
    run.settle();
    // VB: MouseDown, Click, MouseUp, then DblClick, MouseUp
    assert_eq!(
        run.log(),
        "down 1 0 10,10;click;up 1 0 10,10;dbl;up 1 0 10,10;"
    );
    run.stop();
}

#[gpui::test]
fn the_form_gets_the_mouse_where_no_control_is(cx: &mut gpui::TestAppContext) {
    let src = [
        mouse_handler("Form", "MouseDown", "fdown"),
        plain_handler("Form", "Click", "fclick"),
    ]
    .concat();
    let mut run = Run::start(event_form(), &src, cx);
    let form = run.bounds("run-Label1"); // the form's origin is the window's
    let empty = gpui::point(form.origin.x + px(300.0), form.origin.y + px(250.0));
    run.down(empty, gpui::MouseButton::Left, 1);
    run.up(empty, gpui::MouseButton::Left);
    run.settle();
    assert_eq!(run.log(), "fdown 1 0 308,258;fclick;");
    run.stop();
}

#[gpui::test]
fn a_disabled_control_raises_nothing_and_the_form_gets_its_mouse(cx: &mut gpui::TestAppContext) {
    let mut form = event_form();
    form.controls[1].enabled = false; // Command1
    let src = [
        plain_handler("Command1", "Click", "click"),
        mouse_handler("Command1", "MouseDown", "down"),
        plain_handler("Form", "Click", "fclick"),
    ]
    .concat();
    let mut run = Run::start(form, &src, cx);
    let b = run.bounds("run-Command1");
    run.down(at(b, 5.0, 5.0), gpui::MouseButton::Left, 1);
    run.up(at(b, 5.0, 5.0), gpui::MouseButton::Left);
    run.settle();
    assert_eq!(run.log(), "fclick;");
    run.stop();
}

/// A key handler (`KeyDown`/`KeyUp`) that logs `name` and its `KeyCode`
/// and `Shift`, then runs `body`.
fn key_handler(object: &str, event: &str, name: &str, body: &str) -> String {
    format!(
        "Sub {}_{}(KeyCode As Integer, Shift As Integer)\n\
         \x20 Log \"{} \" & KeyCode & \" \" & Shift\n  {}\nEnd Sub\n",
        object, event, name, body
    )
}

#[gpui::test]
fn typing_raises_key_down_key_press_change_key_up_and_key_ascii_changes_the_text(
    cx: &mut gpui::TestAppContext,
) {
    let src = [
        key_handler("Text1", "KeyDown", "down", ""),
        key_handler("Text1", "KeyUp", "up", ""),
        // KeyPress upper-cases letters and swallows digits
        "Sub Text1_KeyPress(KeyAscii As Integer)\n\
         \x20 Log \"press \" & KeyAscii\n\
         \x20 If KeyAscii >= 48 And KeyAscii <= 57 Then KeyAscii = 0\n\
         \x20 If KeyAscii >= 97 And KeyAscii <= 122 Then KeyAscii = KeyAscii - 32\n\
         End Sub\n"
            .to_string(),
        "Sub Text1_Change()\n  Log \"change \" & Text1.Text\nEnd Sub\n".to_string(),
    ]
    .concat();
    let mut run = Run::start(event_form(), &src, cx);
    let t = run.bounds("run-Text1");
    run.win.simulate_click(t.center(), gpui::Modifiers::none());
    run.settle();
    run.clear();
    run.win.simulate_keystrokes("a");
    run.settle();
    run.win.simulate_keystrokes("5");
    run.settle();
    // the release of the key, which GPUI delivers on its own
    run.win.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("a").unwrap(),
    });
    run.settle();
    assert_eq!(
        run.log(),
        "down 65 0;press 97;change A;down 53 0;press 53;up 65 0;",
        "a: KeyDown, KeyPress, Change of the upper-cased A; 5: swallowed"
    );
    run.handle
        .read_with(&run.win, |r, _| assert_eq!(r.state[2].text, "A"))
        .unwrap();
    run.stop();
}

#[gpui::test]
fn key_preview_lets_the_form_see_and_cancel_keys_first(cx: &mut gpui::TestAppContext) {
    let mut form = event_form();
    form.key_preview = true;
    let src = [
        // the form cancels Delete for every control
        key_handler(
            "Form",
            "KeyDown",
            "fdown",
            "If KeyCode = 46 Then KeyCode = 0",
        ),
        key_handler("Text1", "KeyDown", "tdown", ""),
        "Sub Form_KeyPress(KeyAscii As Integer)\n  Log \"fpress \" & KeyAscii\nEnd Sub\n"
            .to_string(),
    ]
    .concat();
    let mut run = Run::start(form, &src, cx);
    let t = run.bounds("run-Text1");
    run.win.simulate_click(t.center(), gpui::Modifiers::none());
    run.settle();
    run.clear();
    run.win.simulate_keystrokes("shift-b");
    run.settle();
    run.win.simulate_keystrokes("delete");
    run.settle();
    // b: the form's KeyDown, then the control's, then the form's KeyPress.
    // Delete (46) — the form's KeyDown cancels it with KeyCode = 0, so the
    // control's KeyDown doesn't run; it types nothing, so no KeyPress
    assert_eq!(run.log(), "fdown 66 1;tdown 66 1;fpress 66;fdown 46 0;");
    run.handle
        .read_with(&run.win, |r, _| assert_eq!(r.state[2].text, "B"))
        .unwrap();
    run.stop();
}

#[gpui::test]
fn the_focus_moves_with_clicks_and_tab_raising_lost_and_got_focus(cx: &mut gpui::TestAppContext) {
    let src = [
        plain_handler("Text1", "GotFocus", "got1"),
        plain_handler("Text1", "LostFocus", "lost1"),
        plain_handler("Text2", "GotFocus", "got2"),
        plain_handler("Text2", "LostFocus", "lost2"),
        plain_handler("Command1", "GotFocus", "gotC"),
        plain_handler("Command1", "LostFocus", "lostC"),
        plain_handler("Picture1", "GotFocus", "gotP"),
        plain_handler("Picture1", "LostFocus", "lostP"),
    ]
    .concat();
    let mut run = Run::start(event_form(), &src, cx);
    let t1 = run.bounds("run-Text1");
    run.win.simulate_click(t1.center(), gpui::Modifiers::none());
    run.settle();
    // Tab goes Text1 -> Text2 -> Picture1 -> (wraps) Command1; Shift-Tab
    // goes back
    for keys in ["tab", "tab", "tab", "shift-tab"] {
        run.win.simulate_keystrokes(keys);
        run.settle();
    }
    assert_eq!(
        run.log(),
        "got1;lost1;got2;lost2;gotP;lostP;gotC;lostC;gotP;",
        "the tab order is the order the controls were added"
    );
    // a click on the label (which takes no focus) leaves it where it is
    run.clear();
    let l = run.bounds("run-Label1");
    run.win.simulate_click(l.center(), gpui::Modifiers::none());
    run.settle();
    assert_eq!(run.log(), "");
    run.stop();
}

/// Hand the IDE what the program sent since the last look.
fn pump(run: &mut Run) {
    std::thread::sleep(std::time::Duration::from_millis(150));
    while let Ok(ev) = run.rx.try_recv() {
        run.ide.update(run.cx, |ide, cx| ide.on_debug_event(ev, cx));
    }
    run.cx.run_until_parked();
}

#[gpui::test]
fn closing_the_window_asks_query_unload_which_may_cancel(cx: &mut gpui::TestAppContext) {
    let src = "Dim Tries As Integer\n\
         Sub Form_QueryUnload(Cancel As Integer, UnloadMode As Integer)\n\
         \x20 Tries = Tries + 1\n\
         \x20 Log \"query \" & UnloadMode\n\
         \x20 If Tries = 1 Then Cancel = True\n\
         End Sub\n\
         Sub Form_Unload(Cancel As Integer)\n  Print \"unloaded\"\nEnd Sub\n";
    let mut run = Run::start(event_form(), src, cx);
    // the first close is refused: the window stays and the program runs
    assert!(!run.win.simulate_close(), "the window stays open for now");
    run.settle();
    assert_eq!(run.log(), "query 0;");
    assert!(run.ide.read_with(run.cx, |ide, _| ide.live));
    assert!(run
        .ide
        .read_with(run.cx, |ide, _| ide.form_window().is_some()));
    // the second goes ahead: Unload runs, the window closes, the program
    // (whose last form that was) ends
    let _ = run.win.simulate_close();
    pump(&mut run);
    pump(&mut run);
    run.ide.read_with(run.cx, |ide, _| {
        assert!(ide.form_window().is_none(), "the program closed the window");
        assert!(ide.output.contains("unloaded"), "{}", ide.output);
        assert!(!ide.live, "its last form went, so the program ended");
    });
}

#[gpui::test]
fn the_window_raises_resize_activate_and_deactivate(cx: &mut gpui::TestAppContext) {
    let src = [
        "Sub Form_Resize()\n  Log \"resize \" & Me.Width\nEnd Sub\n".to_string(),
        plain_handler("Form", "Activate", "activate"),
        plain_handler("Form", "Deactivate", "deactivate"),
    ]
    .concat();
    let mut run = Run::start(event_form(), &src, cx);
    run.win.simulate_resize(size(px(450.0), px(320.0)));
    run.settle();
    assert_eq!(run.log(), "resize 450;", "the program reads the new Width");
    run.clear();
    // the window becomes the active one, then another window is
    run.handle
        .update(&mut run.win, |_, win, _| win.activate_window())
        .unwrap();
    run.settle();
    run.win.deactivate_window();
    run.settle();
    assert_eq!(run.log(), "activate;deactivate;");
    run.stop();
}

#[gpui::test]
fn a_property_the_program_sets_raises_its_event_inside_the_statement(
    cx: &mut gpui::TestAppContext,
) {
    let src = "Sub Command1_Click()\n  Text1.Text = \"set\"\n  Log \"after\"\nEnd Sub\n\
               Sub Text1_Change()\n  Log \"change \" & Text1.Text\nEnd Sub\n";
    let mut run = Run::start(event_form(), src, cx);
    let b = run.bounds("run-Command1");
    run.win.simulate_click(b.center(), gpui::Modifiers::none());
    run.settle();
    assert_eq!(run.log(), "change set;after;");
    run.stop();
}

#[gpui::test]
fn only_handled_events_are_sent_to_the_program(cx: &mut gpui::TestAppContext) {
    // the program handles no mouse event: moving over the form sends none
    let src = plain_handler("Command1", "Click", "click");
    let mut run = Run::start(event_form(), &src, cx);
    run.handle
        .read_with(&run.win, |r, _| {
            let set = r.handlers.as_ref().expect("the program said");
            assert!(set.contains(&("command1".into(), "click".into())));
            assert!(!r.handles(Some(1), "MouseMove"));
            assert!(!r.handles(None, "MouseMove"));
        })
        .unwrap();
    run.stop();
}
