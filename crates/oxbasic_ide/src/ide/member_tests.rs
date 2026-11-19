//! The control members of the live window, driven by real input: text
//! editing (`MaxLength`, `Locked`, `MultiLine`, the selection), MultiSelect
//! lists, run-time array elements (`Load Cmd(n)`), and option groups in a
//! Frame.

use super::toolbox_tests::{ide_window, open_live_window, settle};
use super::*;
use std::sync::mpsc::channel;

/// The live window of `form` running `src`.
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
        let (ide, cx) = ide_window(cx);
        let (tx, rx) = channel();
        let session = DebugSession::start_with_form(
            src.to_string(),
            Some(form.clone()),
            Arc::new(Mutex::new(HashSet::new())),
            tx,
        );
        session.send(DebugCommand::Run);
        let (handle, win) = open_live_window(&ide, form, &session, &rx, cx);
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

    fn click(&mut self, sel: &'static str) {
        let b = self
            .win
            .debug_bounds(sel)
            .unwrap_or_else(|| panic!("{}", sel));
        self.win.simulate_click(b.center(), gpui::Modifiers::none());
        self.settle();
    }

    fn click_with(&mut self, sel: &'static str, m: gpui::Modifiers) {
        let b = self
            .win
            .debug_bounds(sel)
            .unwrap_or_else(|| panic!("{}", sel));
        self.win.simulate_click(b.center(), m);
        self.settle();
    }

    fn keys(&mut self, k: &str) {
        self.win.simulate_keystrokes(k);
        self.settle();
    }

    fn type_text(&mut self, t: &str) {
        self.win.simulate_input(t);
        self.settle();
    }

    fn read<T>(&self, f: impl FnOnce(&FormRunner) -> T) -> T {
        self.handle.read_with(&self.win, |r, _| f(r)).unwrap()
    }

    fn caption(&self, i: usize) -> String {
        self.read(|r| r.form.controls[i].caption.clone())
    }

    fn stop(self) {
        self.session.send(DebugCommand::Stop);
    }
}

#[gpui::test]
fn typing_obeys_max_length_and_locked_and_replaces_the_selection(cx: &mut gpui::TestAppContext) {
    let mut f = FormSpec::new("Form1", "Text", 400.0, 300.0);
    f.label("Label1", "", Rect::new(8.0, 8.0, 380.0, 20.0));
    f.text_box("Text1", "", Rect::new(16.0, 40.0, 160.0, 24.0));
    f.controls[1].max_length = 4;
    f.button("Command1", "Lock", Rect::new(200.0, 40.0, 90.0, 28.0));
    let src =
        "Sub Text1_Change()\n  Label1.Caption = Text1.Text & \"|\" & Text1.SelStart\nEnd Sub\n\
               Sub Command1_Click()\n  Text1.Locked = True\nEnd Sub\n";
    let mut live = Live::start(f, src, cx);
    live.click("run-Text1");
    live.type_text("abcdef");
    // MaxLength stops typing at 4 characters; the program sees the caret
    assert_eq!(live.read(|r| r.state[1].text.clone()), "abcd");
    assert_eq!(live.caption(0), "abcd|4");
    // Shift+Left twice selects "cd"; typing replaces it
    live.keys("shift-left shift-left");
    live.type_text("Z");
    assert_eq!(live.read(|r| r.state[1].text.clone()), "abZ");
    assert_eq!(live.caption(0), "abZ|3");
    // Locked: the caret moves, the text doesn't change
    live.click("run-Command1");
    live.click("run-Text1");
    live.type_text("q");
    live.keys("backspace");
    assert_eq!(live.read(|r| r.state[1].text.clone()), "abZ");
    live.stop();
}

#[gpui::test]
fn a_multi_line_text_box_takes_enter_as_a_new_line(cx: &mut gpui::TestAppContext) {
    let mut f = FormSpec::new("Form1", "Text", 400.0, 300.0);
    f.text_box("Text1", "", Rect::new(16.0, 40.0, 200.0, 80.0));
    f.controls[0].multi_line = true;
    f.controls[0].scroll_bars = 2;
    f.text_box("Text2", "", Rect::new(16.0, 140.0, 200.0, 24.0));
    let mut live = Live::start(f, "Sub Text1_Change()\nEnd Sub\n", cx);
    live.click("run-Text1");
    live.type_text("one");
    live.keys("enter");
    live.type_text("two");
    // up goes to the first line, at the same column
    live.keys("up");
    live.type_text("!");
    assert_eq!(live.read(|r| r.state[0].text.clone()), "one!\ntwo");
    // a one-line box doesn't take Enter
    live.click("run-Text2");
    live.type_text("x");
    live.keys("enter");
    assert_eq!(live.read(|r| r.state[1].text.clone()), "x");
    live.stop();
}

#[gpui::test]
fn a_multi_select_list_selects_several_rows(cx: &mut gpui::TestAppContext) {
    let mut f = FormSpec::new("Form1", "List", 400.0, 300.0);
    f.label("Label1", "", Rect::new(8.0, 8.0, 380.0, 20.0));
    let mut list = ControlSpec::new(
        "List1",
        ControlKind::ListBox,
        Rect::new(16.0, 40.0, 160.0, 100.0),
    );
    list.items = ["A", "B", "C", "D"].map(String::from).to_vec();
    list.multi_select = 2; // Extended
    f.control(list);
    let src =
        "Sub List1_Click()\n  Dim i As Integer, s As String\n  For i = 0 To List1.ListCount - 1\n\
               \x20   If List1.Selected(i) Then s = s & List1.List(i)\n  Next\n\
               \x20 Label1.Caption = s & \" \" & List1.SelCount\nEnd Sub\n";
    let mut live = Live::start(f, src, cx);
    live.click("run-List1-item-0");
    assert_eq!(live.caption(0), "A 1");
    // Shift-click: the rows from A to C
    live.click_with("run-List1-item-2", gpui::Modifiers::shift());
    assert_eq!(live.caption(0), "ABC 3");
    // Cmd/Ctrl-click toggles one row
    live.click_with("run-List1-item-1", gpui::Modifiers::command());
    assert_eq!(live.caption(0), "AC 2");
    // a plain click selects just that row again
    live.click("run-List1-item-3");
    assert_eq!(live.caption(0), "D 1");
    live.stop();
}

#[gpui::test]
fn loading_an_array_element_draws_it_and_it_clicks_with_its_index(cx: &mut gpui::TestAppContext) {
    let mut f = FormSpec::new("Form1", "Array", 400.0, 300.0);
    f.label("Label1", "", Rect::new(8.0, 8.0, 380.0, 20.0));
    f.button("Cmd", "First", Rect::new(16.0, 40.0, 90.0, 28.0));
    f.controls[1].index = Some(0);
    f.button("Command1", "Add", Rect::new(200.0, 40.0, 90.0, 28.0));
    let src = "Sub Command1_Click()\n  Load Cmd(1)\n  Cmd(1).Move 16, 80\n  Cmd(1).Caption = \"Second\"\n\
               \x20 Cmd(1).Visible = True\nEnd Sub\n\
               Sub Cmd_Click(Index As Integer)\n  Label1.Caption = \"cmd \" & Index\n  If Index = 1 Then Unload Cmd(1)\nEnd Sub\n";
    let mut live = Live::start(f, src, cx);
    live.click("run-Command1");
    // the new element is drawn below the first, and clicks as element 1
    assert_eq!(live.read(|r| r.form.controls.len()), 4);
    let b = live
        .win
        .debug_bounds("run-Cmd(1)")
        .expect("the loaded element");
    let first = live.win.debug_bounds("run-Cmd(0)").unwrap();
    assert!(b.origin.y > first.origin.y + px(30.0), "moved to 16,80");
    live.click("run-Cmd(1)");
    assert_eq!(live.caption(0), "cmd 1");
    // ...which unloaded it; the first still clicks as element 0
    assert_eq!(live.read(|r| r.form.controls.len()), 3);
    live.click("run-Cmd(0)");
    assert_eq!(live.caption(0), "cmd 0");
    live.stop();
}

#[gpui::test]
fn option_buttons_in_a_frame_are_a_group_of_their_own(cx: &mut gpui::TestAppContext) {
    let mut f = FormSpec::new("Form1", "Groups", 400.0, 300.0);
    let mut frame = ControlSpec::new(
        "Frame1",
        ControlKind::Frame,
        Rect::new(150.0, 20.0, 200.0, 120.0),
    );
    frame.caption = "Size".into();
    f.control(frame);
    for (n, top, inside) in [
        (1, 20.0, false),
        (2, 50.0, false),
        (3, 30.0, true),
        (4, 60.0, true),
    ] {
        let mut o = ControlSpec::new(
            &format!("Option{}", n),
            ControlKind::OptionButton,
            Rect::new(10.0, top, 110.0, 22.0),
        );
        if inside {
            o.container = Some("Frame1".into());
        }
        f.control(o);
    }
    let mut live = Live::start(f, "Sub Option1_Click()\nEnd Sub\n", cx);
    // Option3 is drawn inside the frame: 150 + 10 across
    let o3 = live.win.debug_bounds("run-Option3").unwrap();
    let frame = live.win.debug_bounds("run-Frame1").unwrap();
    assert_eq!(o3.origin.x - frame.origin.x, px(10.0));
    live.click("run-Option1");
    live.click("run-Option3");
    live.click("run-Option2");
    // one chosen on the form (Option2) and one in the frame (Option3)
    let values = live.read(|r| (1..=4).map(|i| r.state[i].value).collect::<Vec<_>>());
    assert_eq!(values, [0, 1, 1, 0]);
    live.stop();
}

#[gpui::test]
fn tooltip_mouse_pointer_picture_and_a_system_colour_are_drawn(cx: &mut gpui::TestAppContext) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase");
    let mut f = FormSpec::new("Form1", "Looks", 400.0, 300.0);
    f.label("Label1", "Hover me", Rect::new(8.0, 8.0, 200.0, 20.0));
    f.control(ControlSpec::new(
        "Picture1",
        ControlKind::PictureBox,
        Rect::new(16.0, 40.0, 120.0, 120.0),
    ));
    let src = format!(
        "Sub Form_Load()\n  Label1.ToolTipText = \"A tip\"\n  Label1.MousePointer = vbCrosshair\n\
         \x20 Label1.BackColor = vbInfoBackground\n\
         \x20 Picture1.Picture = LoadPicture(\"{}\")\nEnd Sub\n",
        dir.join("logo.png").display()
    );
    let mut live = Live::start(f, &src, cx);
    live.read(|r| {
        let c = &r.form.controls[0];
        assert_eq!(c.tool_tip, "A tip");
        assert_eq!(c.mouse_pointer, 2);
        assert_eq!(
            c.color,
            Some(0xFFFFE1),
            "vbInfoBackground is drawn pale yellow"
        );
        assert!(r.form.controls[1].picture.ends_with("logo.png"));
    });
    // the picture element is drawn inside the PictureBox
    live.win.run_until_parked();
    assert!(
        live.win.debug_bounds("Picture1-picture").is_some(),
        "the image is drawn"
    );
    // hovering for half a second shows the tooltip
    let l = live.win.debug_bounds("run-Label1").unwrap();
    live.win
        .simulate_mouse_move(l.center(), None, gpui::Modifiers::none());
    live.win
        .executor()
        .advance_clock(std::time::Duration::from_millis(600));
    live.win.run_until_parked();
    live.handle
        .update(&mut live.win, |_, w, _| w.refresh())
        .unwrap();
    live.win.run_until_parked();
    assert!(
        live.win.debug_bounds("tooltip").is_some(),
        "the tooltip shows"
    );
    live.stop();
}

#[gpui::test]
fn drawing_stretching_and_auto_sizing_show_in_the_live_window(cx: &mut gpui::TestAppContext) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase");
    let logo = dir.join("logo.png");
    let mut f = FormSpec::new("Form1", "Art", 400.0, 300.0);
    f.control(ControlSpec::new(
        "Picture1",
        ControlKind::PictureBox,
        Rect::new(10.0, 10.0, 120.0, 100.0),
    ));
    f.control(ControlSpec::new(
        "Image1",
        ControlKind::Image,
        Rect::new(150.0, 10.0, 80.0, 60.0),
    ));
    f.control(ControlSpec::new(
        "Picture2",
        ControlKind::PictureBox,
        Rect::new(10.0, 130.0, 50.0, 50.0),
    ));
    let src = format!(
        "Sub Form_Load()\n\
         \x20 Picture1.Line (5, 5)-(60, 40), vbRed\n\
         \x20 Picture1.Circle (60, 60), 20\n\
         \x20 Picture1.PSet (10, 90), vbBlue\n\
         \x20 Picture1.PSet (11, 90), vbBlue\n\
         \x20 Image1.Picture = LoadPicture(\"{p}\")\n\
         \x20 Image1.Stretch = True\n\
         \x20 Picture2.AutoSize = True\n\
         \x20 Picture2.Picture = LoadPicture(\"{p}\")\n\
         End Sub\n",
        p = logo.display()
    );
    let mut live = Live::start(f, &src, cx);
    // the drawing reached the window, and is painted over the picture box
    let shapes = live.read(|r| r.state[0].canvas.shapes.len());
    assert_eq!(shapes, 2, "a line and a circle");
    // PSet reached the window as bitmap pixels, not shapes
    let (count, blue) = live.read(|r| {
        let raster = &r.state[0].canvas.raster;
        (raster.count(), raster.pixel(10, 90))
    });
    assert_eq!(count, 2, "two PSet pixels");
    assert_eq!(blue, Some(0x0000FF), "vbBlue");
    // a stretched Image fills its control; the picture is drawn at its size
    let image = live.win.debug_bounds("run-Image1").unwrap();
    let pic = live
        .win
        .debug_bounds("Image1-picture")
        .expect("the Image's picture");
    assert_eq!(pic.size, image.size, "Stretch scales it to the control");
    // AutoSize: the PictureBox took the picture's size (512x512 plus border)
    let size = live.read(|r| {
        (
            r.form.controls[2].rect.width,
            r.form.controls[2].rect.height,
        )
    });
    assert_eq!(size, (516.0, 516.0));
    live.stop();
}

#[gpui::test]
fn a_click_puts_the_caret_where_it_lands_and_a_drag_selects(cx: &mut gpui::TestAppContext) {
    let mut f = FormSpec::new("Form1", "Text", 400.0, 300.0);
    f.text_box(
        "Text1",
        "hello brave new world",
        Rect::new(16.0, 40.0, 300.0, 24.0),
    );
    f.label("Label1", "", Rect::new(16.0, 8.0, 360.0, 20.0));
    let src = "Sub Text1_Click()\n  Label1.Caption = Text1.SelStart & \",\" & Text1.SelLength\nEnd Sub\n\
               Sub Text1_MouseUp(Button As Integer, Shift As Integer, X As Single, Y As Single)\n\
               \x20 Label1.Caption = Text1.SelStart & \",\" & Text1.SelLength & \"=\" & Text1.SelText\nEnd Sub\n";
    let mut live = Live::start(f, src, cx);
    let text = live.win.debug_bounds("run-Text1").unwrap();
    let at = |dx: f32| gpui::point(text.origin.x + px(dx), text.center().y);
    let sel = |live: &Live| live.read(|r| r.state[0].selection());

    // a click at the far left puts the caret before everything, far right
    // after it, and between them somewhere in the middle
    for (dx, want) in [(1.0, Some(0)), (299.0, Some(21)), (60.0, None)] {
        live.win.simulate_click(at(dx), gpui::Modifiers::none());
        live.settle();
        let (start, len) = sel(&live);
        assert_eq!(len, 0, "a click selects nothing");
        match want {
            Some(n) => assert_eq!(start, n, "click at {}", dx),
            None => assert!((1..21).contains(&start), "mid-text, got {}", start),
        }
    }
    // clicks further right land further along the text
    let mut last = 0;
    for dx in [20.0, 50.0, 80.0, 110.0, 140.0] {
        live.win.simulate_click(at(dx), gpui::Modifiers::none());
        live.settle();
        let (start, _) = sel(&live);
        assert!(start > last, "{} should be right of {}", start, last);
        last = start;
    }

    // dragging from one point to another selects the text between
    live.win.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Left,
        position: at(1.0),
        modifiers: gpui::Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    live.win.simulate_event(gpui::MouseMoveEvent {
        position: at(299.0),
        pressed_button: Some(gpui::MouseButton::Left),
        modifiers: gpui::Modifiers::none(),
    });
    live.win.simulate_event(gpui::MouseUpEvent {
        button: gpui::MouseButton::Left,
        position: at(299.0),
        modifiers: gpui::Modifiers::none(),
        click_count: 1,
    });
    live.settle();
    assert_eq!(sel(&live), (0, 21), "all of it");
    // and the program sees the selection as the drag left it
    assert_eq!(live.caption(1), "0,21=hello brave new world");

    // a double click selects the word under it
    for n in 1..=2 {
        live.win.simulate_event(gpui::MouseDownEvent {
            button: gpui::MouseButton::Left,
            position: at(1.0),
            modifiers: gpui::Modifiers::none(),
            click_count: n,
            first_mouse: false,
        });
        live.win.simulate_event(gpui::MouseUpEvent {
            button: gpui::MouseButton::Left,
            position: at(1.0),
            modifiers: gpui::Modifiers::none(),
            click_count: n,
        });
    }
    live.settle();
    assert_eq!(sel(&live), (0, 5), "hello");
    assert_eq!(live.caption(1), "0,5=hello");
    live.stop();
}

#[gpui::test]
fn a_custom_pointer_and_the_wait_cursor_are_set_on_a_live_control(cx: &mut gpui::TestAppContext) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase");
    let mut f = FormSpec::new("Form1", "Pointers", 400.0, 300.0);
    f.label("Label1", "custom", Rect::new(8.0, 8.0, 100.0, 20.0));
    f.label("Label2", "busy", Rect::new(8.0, 40.0, 100.0, 20.0));
    let src = format!(
        "Sub Form_Load()\n  Label1.MouseIcon = LoadPicture(\"{}\")\n  Label1.MousePointer = vbCustom\n\
         \x20 Label2.MousePointer = vbHourglass\nEnd Sub\n",
        dir.join("logo.png").display()
    );
    let mut live = Live::start(f, &src, cx);
    live.read(|r| {
        assert_eq!(r.form.controls[0].mouse_pointer, 99);
        assert!(r.form.controls[0].mouse_icon.ends_with("logo.png"));
        assert_eq!(r.form.controls[1].mouse_pointer, 11);
    });
    // hovering each draws the window, which installs the cursors; the wait
    // cursor is the macOS one where this Mac has it
    for sel in ["run-Label1", "run-Label2"] {
        let b = live.win.debug_bounds(sel).unwrap();
        live.win
            .simulate_mouse_move(b.center(), None, gpui::Modifiers::none());
        live.settle();
    }
    #[cfg(target_os = "macos")]
    assert!(oxbasic_ui::mac_cursor::install_wait_cursor());
    live.stop();
}
