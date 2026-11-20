//! A scrollbar in the running window: a real drag of its thumb sets the
//! program's `Value` and raises `Change`, and the thumb is drawn where the
//! value is.

use super::toolbox_tests::settle;
use super::*;
use std::sync::mpsc::channel;

/// The live window of `form` running `src`, as member_tests builds it.
struct Live<'a> {
    ide: Entity<Ide>,
    session: DebugSession,
    rx: std::sync::mpsc::Receiver<DebugEvent>,
    handle: WindowHandle<FormRunner>,
    win: gpui::VisualTestContext,
    cx: &'a mut gpui::VisualTestContext,
}

impl<'a> Live<'a> {
    fn start(form: FormSpec, src: &str, cx: &'a mut gpui::TestAppContext) -> Live<'a> {
        let (ide, cx) = super::toolbox_tests::ide_window(cx);
        let (tx, rx) = channel();
        let session = DebugSession::start_with_form(
            src.to_string(),
            Some(form.clone()),
            Arc::new(Mutex::new(HashSet::new())),
            tx,
        );
        session.send(DebugCommand::Run);
        let (handle, win) = super::toolbox_tests::open_live_window(&ide, form, &session, &rx, cx);
        let mut live = Live {
            ide,
            session,
            rx,
            handle,
            win,
            cx,
        };
        live.settle();
        live
    }

    fn settle(&mut self) {
        self.win.run_until_parked();
        settle(&self.ide, &self.rx, &mut self.win, self.cx);
    }

    fn read<T>(&self, f: impl FnOnce(&FormRunner) -> T) -> T {
        self.handle.read_with(&self.win, |r, _| f(r)).unwrap()
    }

    fn caption(&self, i: usize) -> String {
        self.read(|r| r.form.controls[i].caption.clone())
    }

    fn stop(&mut self) {
        self.session.send(DebugCommand::Stop);
    }
}

#[gpui::test]
fn dragging_a_scrollbar_thumb_sets_value_and_the_program_sees_it(cx: &mut gpui::TestAppContext) {
    let mut f = FormSpec::new("Form1", "Scroll", 400.0, 300.0);
    f.label("Label1", "", Rect::new(8.0, 8.0, 380.0, 20.0));
    let mut bar = ControlSpec::new(
        "HScroll1",
        ControlKind::HScrollBar,
        Rect::new(16.0, 60.0, 300.0, 17.0),
    );
    bar.scroll_min = 0;
    bar.scroll_max = 100;
    f.control(bar);
    let src = "Sub HScroll1_Change()\n  Label1.Caption = \"at \" & HScroll1.Value\nEnd Sub\n";
    let mut live = Live::start(f, src, cx);
    let b = live
        .win
        .debug_bounds("run-HScroll1")
        .expect("the scrollbar is drawn");
    // press a quarter of the way along, then drag to three quarters: the
    // drag adds the pointer's travel (half the track), so Value goes 0 -> 50
    let quarter = gpui::point(b.origin.x + b.size.width * 0.25, b.center().y);
    let three_quarters = gpui::point(b.origin.x + b.size.width * 0.75, b.center().y);
    live.win
        .simulate_mouse_down(quarter, gpui::MouseButton::Left, gpui::Modifiers::none());
    live.settle();
    assert_eq!(
        live.read(|r| r.state[1].value),
        0,
        "a press alone doesn't move it"
    );
    live.win.simulate_mouse_move(
        three_quarters,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::none(),
    );
    live.settle();
    assert_eq!(
        live.read(|r| r.state[1].value),
        50,
        "the drag follows the pointer's travel"
    );
    live.win.simulate_mouse_up(
        three_quarters,
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    live.settle();
    assert_eq!(
        live.caption(0),
        "at 50",
        "the program's Change handler saw the drag"
    );
    live.stop();
}
