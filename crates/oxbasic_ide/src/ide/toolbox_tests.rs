//! Toolbox tests: every control kind is added with VB6's defaults, drawn
//! the way VB6 draws it, and (in a running form) behaves the way VB6 does.

use super::*;

/// A designer with an empty 400x500 form, window sized for it.
pub(super) fn ide_window(
    cx: &mut gpui::TestAppContext,
) -> (Entity<Ide>, &mut gpui::VisualTestContext) {
    let (ide, cx) = cx.add_window_view(|_, cx| Ide::new(None, None, cx));
    cx.simulate_resize(size(px(1400.0), px(1000.0)));
    ide.update(cx, |ide, cx| {
        ide.form.controls.clear();
        ide.form.width = 400.0;
        ide.form.height = 500.0;
        cx.notify();
    });
    cx.run_until_parked();
    (ide, cx)
}

pub(super) const ALL: [ControlKind; 8] = [
    ControlKind::Label,
    ControlKind::TextBox,
    ControlKind::CommandButton,
    ControlKind::Frame,
    ControlKind::ListBox,
    ControlKind::ComboBox,
    ControlKind::PictureBox,
    ControlKind::Timer,
];

/// `debug_bounds` wants a `&'static str`; tests may leak a few.
pub(super) fn sel(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

#[gpui::test]
fn every_toolbox_button_adds_its_control_with_vb6_defaults(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    for kind in ALL {
        let button = cx
            .debug_bounds(sel(format!("tool-{}", kind.basic_name())))
            .unwrap_or_else(|| panic!("toolbox button for {:?}", kind));
        cx.simulate_click(button.center(), gpui::Modifiers::none());
        cx.run_until_parked();
    }
    ide.read_with(cx, |ide, _| {
        let c = &ide.form.controls;
        assert_eq!(c.len(), 8, "one control per toolbox button");
        let names: Vec<_> = c.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            ["Label1", "Text1", "Command1", "Frame1", "List1", "Combo1", "Picture1", "Timer1"]
        );
        for (spec, kind) in c.iter().zip(ALL) {
            assert_eq!(spec.kind, kind);
            let (w, h) = kind.default_size();
            assert_eq!((spec.rect.width, spec.rect.height), (w, h), "{}", spec.name);
            // captions / text as VB6 initialises them
            let caption = if kind.has_caption() {
                spec.name.as_str()
            } else {
                ""
            };
            assert_eq!(spec.caption, caption, "{}", spec.name);
            assert_eq!(spec.text.is_some(), kind.has_text(), "{}", spec.name);
            assert_eq!(spec.style, 0, "a new ComboBox is a Dropdown Combo");
        }
        assert_eq!(c[7].interval_ms, 1000, "Timer.Interval defaults to 1 s");
        assert!(!c[7].visible, "a Timer is invisible at run time");
        // nothing overlaps (apart from the size-less Timer)
        let sized: Vec<_> = c.iter().filter(|c| c.kind != ControlKind::Timer).collect();
        for (i, a) in sized.iter().enumerate() {
            for b in &sized[i + 1..] {
                let apart = a.rect.top + a.rect.height <= b.rect.top
                    || b.rect.top + b.rect.height <= a.rect.top;
                assert!(apart, "{} overlaps {}", a.name, b.name);
            }
        }
    });
    // every control — the Timer included — is on the designer surface
    for i in 0..8 {
        assert!(
            cx.debug_bounds(sel(format!("designer-control-{}", i)))
                .is_some(),
            "control {} is drawn",
            i
        );
    }
}

/// Simulate a left-button drag from `from` by `(dx, dy)` px.
pub(super) fn drag_by(
    cx: &mut gpui::VisualTestContext,
    from: gpui::Point<gpui::Pixels>,
    dx: f32,
    dy: f32,
) {
    let to = gpui::Point::new(from.x + px(dx), from.y + px(dy));
    cx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Left,
        position: from,
        modifiers: gpui::Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(gpui::MouseMoveEvent {
        position: to,
        pressed_button: Some(gpui::MouseButton::Left),
        modifiers: gpui::Modifiers::none(),
    });
    cx.simulate_event(gpui::MouseUpEvent {
        button: gpui::MouseButton::Left,
        position: to,
        modifiers: gpui::Modifiers::none(),
        click_count: 1,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn resize_handles_resize_the_selected_control(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, cx| {
        ide.add_control(ControlKind::CommandButton);
        cx.notify();
    });
    cx.run_until_parked();
    let before = ide.read_with(cx, |ide, _| {
        let r = ide.form.controls[0].rect;
        (r.left, r.top, r.width, r.height)
    });

    // select the control by clicking its body, so the handles appear
    let body = cx
        .debug_bounds(sel("designer-control-0".into()))
        .expect("control is drawn");
    cx.simulate_click(body.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    ide.read_with(cx, |ide, _| {
        assert_eq!(ide.selected, Some(0), "the click selects the control");
    });

    // drag the south-east handle 30 right and 10 down: it must grow the
    // control and move no edges
    let se = cx
        .debug_bounds(sel("designer-handle-0-1-1".into()))
        .expect("SE handle is drawn");
    drag_by(cx, se.center(), 30.0, 10.0);
    ide.read_with(cx, |ide, _| {
        let r = ide.form.controls[0].rect;
        assert_eq!(
            (r.left, r.top),
            (before.0, before.1),
            "the SE handle moves no edges"
        );
        assert_eq!(
            (r.width, r.height),
            (before.2 + 30.0, before.3 + 10.0),
            "the SE handle grows the control"
        );
    });

    // then the north-west handle 8 left and 4 up: it moves left/top and
    // keeps the bottom-right corner fixed
    let before = ide.read_with(cx, |ide, _| {
        let r = ide.form.controls[0].rect;
        (r.left, r.top, r.width, r.height)
    });
    let nw = cx
        .debug_bounds(sel("designer-handle-0--1--1".into()))
        .expect("NW handle is drawn");
    drag_by(cx, nw.center(), -8.0, -4.0);
    ide.read_with(cx, |ide, _| {
        let r = ide.form.controls[0].rect;
        assert_eq!(
            (r.left, r.top),
            (before.0 - 8.0, before.1 - 4.0),
            "the NW handle moves the top-left corner"
        );
        assert_eq!(
            (r.width, r.height),
            (before.2 + 8.0, before.3 + 4.0),
            "the NW handle keeps the bottom-right corner fixed"
        );
    });
}

#[gpui::test]
fn the_forms_own_handles_resize_the_form(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    cx.run_until_parked();

    // with no control selected the form is selected (VB's convention here);
    // drag its bottom-right handle 50 right and 40 down
    let se = cx
        .debug_bounds(sel("designer-handle-form-1-1".into()))
        .expect("the form's SE handle is drawn");
    drag_by(cx, se.center(), 50.0, 40.0);
    ide.read_with(cx, |ide, _| {
        assert_eq!(
            (ide.form.width, ide.form.height),
            (400.0 + 50.0, 500.0 + 40.0),
            "the SE handle grows the form"
        );
    });
}

#[gpui::test]
fn inspector_offers_only_the_properties_a_control_has(cx: &mut gpui::TestAppContext) {
    let geometry = ["left", "top", "width", "height"];
    // every control has Enabled and Index; all but a Timer BackColor;
    // those that show text, ForeColor and Font
    let font = [
        "forecolor",
        "fontname",
        "fontsize",
        "fontbold",
        "fontitalic",
        "fontunderline",
    ];
    let with = |first: &[&'static str], shows_text: bool| -> Vec<&'static str> {
        let mut v: Vec<&'static str> = first.iter().copied().chain(geometry).collect();
        v.extend(["enabled", "index", "backcolor"]);
        if shows_text {
            v.extend(font);
        }
        v
    };
    let expect: [(ControlKind, Vec<&str>); 10] = [
        (ControlKind::Label, with(&["name", "caption"], true)),
        (ControlKind::TextBox, with(&["name", "text"], true)),
        (ControlKind::CommandButton, with(&["name", "caption"], true)),
        (ControlKind::Frame, with(&["name", "caption"], true)),
        (ControlKind::ListBox, with(&["name", "items"], true)),
        (
            ControlKind::ComboBox,
            with(&["name", "text", "items", "style"], true),
        ),
        (ControlKind::PictureBox, with(&["name"], false)),
        (
            ControlKind::Timer,
            vec!["name", "interval", "enabled", "index"],
        ),
        (
            ControlKind::CheckBox,
            with(&["name", "caption", "value"], true),
        ),
        (
            ControlKind::OptionButton,
            with(&["name", "caption", "value"], true),
        ),
    ];

    let (ide, cx) = ide_window(cx);
    for (kind, props) in expect {
        ide.update(cx, |ide, _| {
            ide.add_control(kind);
            let idx = ide.form.controls.len() - 1;
            let keys: Vec<_> = ide
                .inspector_rows(idx)
                .into_iter()
                .map(|(k, _)| k)
                .collect();
            assert_eq!(keys, props, "{:?}", kind);
        });
    }
    // and the rows of the selected control are really drawn
    ide.update(cx, |ide, cx| {
        ide.select(Some(5)); // Combo1
        cx.notify();
    });
    cx.run_until_parked();
    for p in ["name", "text", "items", "style", "left", "height"] {
        assert!(
            cx.debug_bounds(sel(format!("prop-{}", p))).is_some(),
            "prop-{}",
            p
        );
    }
}

#[test]
fn style_round_trips_through_oxform_and_old_files_default_to_zero() {
    let mut f = FormSpec::new("Form1", "F", 100.0, 100.0);
    let mut combo = ControlSpec::new(
        "Combo1",
        ControlKind::ComboBox,
        Rect::new(0.0, 0.0, 100.0, 24.0),
    );
    combo.text = Some("a".into());
    combo.items = vec!["a".into(), "b".into()];
    combo.style = 2;
    f.control(combo);
    let back = FormSpec::from_oxform(&f.to_oxform()).unwrap();
    assert_eq!(back.controls[0].style, 2);
    // a pre-Style file has no "style" key
    let old = r#"{"name":"F","caption":"F","width":10,"height":10,"controls":[
        {"name":"C","kind":"ComboBox","rect":{"left":0,"top":0,"width":9,"height":9}}]}"#;
    assert_eq!(FormSpec::from_oxform(old).unwrap().controls[0].style, 0);
}

#[gpui::test]
fn combo_style_property_is_validated_and_a_simple_combo_gets_room(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    ide.update(cx, |ide, _| {
        ide.add_control(ControlKind::ComboBox);
        ide.apply_prop(0, "style", "2");
        assert_eq!(ide.form.controls[0].style, 2);
        ide.apply_prop(0, "style", "7");
        assert_eq!(
            ide.form.controls[0].style, 2,
            "out-of-range Style is ignored"
        );
        ide.apply_prop(0, "style", "x");
        assert_eq!(ide.form.controls[0].style, 2);
        let h = ide.form.controls[0].rect.height;
        ide.apply_prop(0, "style", "1");
        assert!(
            ide.form.controls[0].rect.height > h,
            "Simple Combo shows its list"
        );
    });
}

/// A form with one of each interactive control, laid out in a column.
fn sample_form() -> FormSpec {
    let mut f = FormSpec::new("Form1", "Sample", 400.0, 500.0);
    let spec = |name: &str, kind: ControlKind, top: f32| {
        let (w, h) = kind.default_size();
        let mut c = ControlSpec::new(name, kind, Rect::new(16.0, top, w, h));
        if kind.has_caption() {
            c.caption = name.into();
        }
        c.text = kind.has_text().then(String::new);
        c
    };
    f.control(spec("Label1", ControlKind::Label, 16.0));
    f.control(spec("Text1", ControlKind::TextBox, 44.0));
    f.control(spec("Command1", ControlKind::CommandButton, 80.0));
    let mut list = spec("List1", ControlKind::ListBox, 120.0);
    list.items = ["Apple", "Banana", "Cherry"].map(String::from).to_vec();
    f.control(list);
    let mut combo = spec("Combo1", ControlKind::ComboBox, 220.0);
    combo.items = ["Red", "Green", "Blue"].map(String::from).to_vec();
    f.control(combo);
    f
}

/// A live program that records each event it receives with `Print`.
fn live_session() -> (DebugSession, std::sync::mpsc::Receiver<DebugEvent>) {
    let src = "Sub Command1_Click()\n Print \"Command1_Click\"\nEnd Sub\n\
               Sub Text1_Change()\n Print \"Text1_Change\"\nEnd Sub\n\
               Sub List1_Click()\n Print \"List1_Click\"\nEnd Sub\n\
               Sub Combo1_Click()\n Print \"Combo1_Click\"\nEnd Sub\n\
               Sub Combo1_Change()\n Print \"Combo1_Change\"\nEnd Sub\n\
               Sub Combo1_DropDown()\n Print \"Combo1_DropDown\"\nEnd Sub\n";
    let (tx, rx) = channel();
    let session = DebugSession::start(src.into(), Arc::new(Mutex::new(HashSet::new())), tx);
    session.send(DebugCommand::Run);
    loop {
        match rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("live")
        {
            DebugEvent::Live => break,
            DebugEvent::Failed(e) => panic!("{}", e),
            _ => {}
        }
    }
    (session, rx)
}

/// The events the program has printed so far.
fn events(rx: &std::sync::mpsc::Receiver<DebugEvent>) -> Vec<String> {
    let mut out = String::new();
    while let Ok(ev) = rx.recv_timeout(std::time::Duration::from_millis(300)) {
        if let DebugEvent::Output(s) = ev {
            out.push_str(&s);
        }
    }
    out.lines().map(String::from).collect()
}

fn run_window(
    cx: &mut gpui::TestAppContext,
    form: FormSpec,
    session: DebugSession,
) -> (Entity<FormRunner>, &mut gpui::VisualTestContext) {
    let (runner, cx) = cx.add_window_view(|_, cx| FormRunner::new(form, 0, session, cx));
    cx.simulate_resize(size(px(500.0), px(560.0)));
    cx.run_until_parked();
    (runner, cx)
}

#[gpui::test]
fn running_list_and_combo_select_items_and_fire_events(cx: &mut gpui::TestAppContext) {
    let (session, rx) = live_session();
    let (runner, cx) = run_window(cx, sample_form(), session);

    // ListBox: click an item to select it
    let banana = cx.debug_bounds("run-List1-item-1").expect("Banana row");
    cx.simulate_click(banana.center(), gpui::Modifiers::none());
    runner.read_with(cx, |r, _| assert_eq!(r.state[3].list_index, Some(1)));

    // ComboBox (Dropdown Combo): closed by default, the arrow drops the list
    // (debug_bounds keeps entries from earlier frames, so "closed" is
    // checked through the control's state, not by a missing element)
    runner.read_with(cx, |r, _| {
        assert!(!r.state[4].dropped, "list starts closed")
    });
    let arrow = cx.debug_bounds("run-Combo1-arrow").expect("combo arrow");
    cx.simulate_click(arrow.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    runner.read_with(cx, |r, _| assert!(r.state[4].dropped));
    let green = cx
        .debug_bounds("run-Combo1-drop-1")
        .expect("Green in the list");
    cx.simulate_click(green.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    runner.read_with(cx, |r, _| {
        assert_eq!(r.state[4].text, "Green", "picking sets Text");
        assert!(!r.state[4].dropped, "the list closes after a pick");
    });
    assert_eq!(
        events(&rx),
        [
            "List1_Click",
            "Combo1_DropDown",
            "Combo1_Change",
            "Combo1_Click"
        ]
    );
}

#[gpui::test]
fn running_textbox_types_and_button_and_label_are_plain(cx: &mut gpui::TestAppContext) {
    let (session, rx) = live_session();
    let (runner, cx) = run_window(cx, sample_form(), session);
    let text = cx.debug_bounds("run-Text1").expect("Text1");
    cx.simulate_click(text.center(), gpui::Modifiers::none());
    cx.simulate_input("hi");
    cx.simulate_keystrokes("backspace");
    runner.read_with(cx, |r, _| assert_eq!(r.state[1].text, "h"));
    let button = cx.debug_bounds("run-Command1").expect("Command1");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    let label = cx.debug_bounds("run-Label1").expect("Label1");
    cx.simulate_click(label.center(), gpui::Modifiers::none()); // no handler: a no-op
    assert_eq!(
        events(&rx),
        [
            "Text1_Change",
            "Text1_Change",
            "Text1_Change",
            "Command1_Click"
        ]
    );
}

#[gpui::test]
fn dropdown_list_style_cannot_be_typed_into(cx: &mut gpui::TestAppContext) {
    let (session, rx) = live_session();
    let mut form = sample_form();
    form.controls[4].style = 2; // Dropdown List
    let (runner, cx) = run_window(cx, form, session);

    // clicking the text area drops the list; typing does nothing
    let field = cx.debug_bounds("run-Combo1-field").expect("field");
    cx.simulate_click(field.center(), gpui::Modifiers::none());
    runner.read_with(cx, |r, _| assert!(r.state[4].dropped));
    cx.simulate_input("xyz");
    runner.read_with(cx, |r, _| assert_eq!(r.state[4].text, ""));
    let blue = cx.debug_bounds("run-Combo1-drop-2").expect("Blue");
    cx.simulate_click(blue.center(), gpui::Modifiers::none());
    runner.read_with(cx, |r, _| assert_eq!(r.state[4].text, "Blue"));
    assert_eq!(
        events(&rx),
        ["Combo1_DropDown", "Combo1_Change", "Combo1_Click"]
    );
}

#[gpui::test]
fn simple_combo_shows_its_list_and_dropdown_combo_is_editable(cx: &mut gpui::TestAppContext) {
    let (session, rx) = live_session();
    let mut form = sample_form();
    form.controls[4].style = 1; // Simple Combo
    form.controls[4].rect.height = 100.0;
    let (runner, cx) = run_window(cx, form, session);

    assert!(
        cx.debug_bounds("run-Combo1-arrow").is_none(),
        "a Simple Combo has no arrow"
    );
    let item = cx
        .debug_bounds("run-Combo1-item-0")
        .expect("list is always visible");
    cx.simulate_click(item.center(), gpui::Modifiers::none());
    runner.read_with(cx, |r, _| assert_eq!(r.state[4].text, "Red"));
    drop(events(&rx));

    // Style 0: typing edits Text and fires Change per keystroke
    runner.update(cx, |r, cx| {
        r.form.controls[4].style = 0;
        r.form.controls[4].rect.height = 24.0;
        r.state[4] = ControlState::new(&r.form.controls[4]);
        cx.notify();
    });
    cx.run_until_parked();
    let field = cx.debug_bounds("run-Combo1-field").expect("field");
    cx.simulate_click(field.center(), gpui::Modifiers::none());
    cx.simulate_input("ab");
    runner.read_with(cx, |r, _| assert_eq!(r.state[4].text, "ab"));
    assert_eq!(events(&rx), ["Combo1_Change", "Combo1_Change"]);
}

#[gpui::test]
fn dropped_list_closes_when_clicking_elsewhere(cx: &mut gpui::TestAppContext) {
    let (session, _rx) = live_session();
    let (runner, cx) = run_window(cx, sample_form(), session);
    let arrow = cx.debug_bounds("run-Combo1-arrow").expect("arrow");
    cx.simulate_click(arrow.center(), gpui::Modifiers::none());
    runner.read_with(cx, |r, _| assert!(r.state[4].dropped));
    // a second click on the arrow toggles it shut
    let arrow = cx.debug_bounds("run-Combo1-arrow").expect("arrow");
    cx.simulate_click(arrow.center(), gpui::Modifiers::none());
    runner.read_with(cx, |r, _| assert!(!r.state[4].dropped));
    // clicking another control closes it too
    cx.simulate_click(arrow.center(), gpui::Modifiers::none());
    let button = cx.debug_bounds("run-Command1").expect("Command1");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    runner.read_with(cx, |r, _| assert!(!r.state[4].dropped));
}

/// A live program that writes to the form: loading retitles it, and a
/// click shows the typed text in `Label1` and adds it to `List1`.
fn writing_session(form: FormSpec) -> (DebugSession, std::sync::mpsc::Receiver<DebugEvent>) {
    let src = "Dim Clicks As Long\n\
               Sub Form_Load()\n Me.Caption = \"Ready\"\nEnd Sub\n\
               Sub Command1_Click()\n Clicks = Clicks + 1\n \
               Label1.Caption = CStr(Clicks) & \":\" & Text1.Text\n \
               List1.AddItem Text1.Text\nEnd Sub\n";
    let (tx, rx) = channel();
    let session = DebugSession::start_with_form(
        src.into(),
        Some(form),
        Arc::new(Mutex::new(HashSet::new())),
        tx,
    );
    session.send(DebugCommand::Run);
    (session, rx)
}

/// Feed the session's events to the IDE (as its poll task does) until a
/// form change or a form window has arrived.
pub(super) fn pump_until_form(
    ide: &Entity<Ide>,
    rx: &std::sync::mpsc::Receiver<DebugEvent>,
    cx: &mut gpui::VisualTestContext,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut forms = 0;
    while forms == 0 && std::time::Instant::now() < deadline {
        while let Ok(ev) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
            if matches!(ev, DebugEvent::Form(_) | DebugEvent::FormShown(_)) {
                forms += 1;
            }
            ide.update(cx, |ide, cx| ide.on_debug_event(ev, cx));
        }
    }
    cx.run_until_parked();
}

#[gpui::test]
fn program_changes_reach_the_live_window(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let form = sample_form();
    let (session, rx) = writing_session(form.clone());
    ide.update(cx, |ide, cx| {
        ide.form = form;
        ide.session = Some(session.clone());
        cx.notify();
    });

    // the program loads and shows its form: the window opens from the
    // events the session sends (Form_Load has already retitled the form)
    pump_until_form(&ide, &rx, cx);
    cx.run_until_parked();
    let handle = ide
        .read_with(cx, |ide, _| ide.form_window())
        .expect("the form window is open");
    let mut win = gpui::VisualTestContext::from_window(handle.into(), cx);
    win.simulate_resize(size(px(500.0), px(560.0)));
    win.run_until_parked();
    handle
        .read_with(&win, |r, _| assert_eq!(r.form.caption, "Ready"))
        .unwrap();

    // the user types in Text1 and clicks the button
    let text = win.debug_bounds("run-Text1").expect("Text1");
    win.simulate_click(text.center(), gpui::Modifiers::none());
    win.simulate_input("hi");
    let button = win.debug_bounds("run-Command1").expect("Command1");
    win.simulate_click(button.center(), gpui::Modifiers::none());
    pump_until_form(&ide, &rx, cx);
    win.run_until_parked();

    // the program saw the typed text and wrote to Label1 and List1; the
    // window shows it, and still holds what the user typed
    handle
        .read_with(&win, |r, _| {
            assert_eq!(r.form.controls[0].caption, "1:hi");
            assert_eq!(
                r.form.controls[3].items,
                ["Apple", "Banana", "Cherry", "hi"]
            );
            assert_eq!(r.state[1].text, "hi", "the typed text stays");
        })
        .unwrap();
    session.send(DebugCommand::Stop);
}

/// A form with a Label, CheckBox, two OptionButtons, a Timer, a two-button
/// array and a TextBox, and a program that reacts to each.
fn interactive_session() -> (
    FormSpec,
    DebugSession,
    std::sync::mpsc::Receiver<DebugEvent>,
) {
    let mut f = FormSpec::new("Form1", "Interactive", 400.0, 300.0);
    f.label("Label1", "start", Rect::new(16.0, 8.0, 200.0, 20.0));
    f.control(ControlSpec::new(
        "Check1",
        ControlKind::CheckBox,
        Rect::new(16.0, 40.0, 110.0, 22.0),
    ));
    for n in 1..=2 {
        f.control(ControlSpec::new(
            &format!("Option{}", n),
            ControlKind::OptionButton,
            Rect::new(16.0, 40.0 + 24.0 * n as f32, 110.0, 22.0),
        ));
    }
    let mut timer = ControlSpec::new("Timer1", ControlKind::Timer, Rect::new(0.0, 0.0, 0.0, 0.0));
    timer.interval_ms = 100;
    timer.visible = false;
    f.control(timer);
    for i in 0..2 {
        let mut b = ControlSpec::new(
            "Cmd",
            ControlKind::CommandButton,
            Rect::new(150.0, 40.0 + 34.0 * i as f32, 90.0, 28.0),
        );
        b.index = Some(i);
        f.control(b);
    }
    f.text_box("Text1", "", Rect::new(16.0, 150.0, 120.0, 24.0));

    let src = "Dim Ticks As Long\n\
        Sub Check1_Click()\n Label1.Caption = \"check=\" & Check1.Value\nEnd Sub\n\
        Sub Option2_Click()\n Label1.Caption = \"opt2=\" & Option2.Value & Option1.Value\nEnd Sub\n\
        Sub Cmd_Click(Index As Integer)\n Label1.Caption = \"cmd\" & Index\n Text1.SetFocus\nEnd Sub\n\
        Sub Timer1_Timer()\n Ticks = Ticks + 1\n Label1.Caption = \"tick\" & Ticks\n \
        If Ticks = 2 Then Timer1.Enabled = False\nEnd Sub\n";
    let (tx, rx) = channel();
    let session = DebugSession::start_with_form(
        src.into(),
        Some(f.clone()),
        Arc::new(Mutex::new(HashSet::new())),
        tx,
    );
    session.send(DebugCommand::Run);
    (f, session, rx)
}

/// Open the IDE's live window for `form` and `session`.
pub(super) fn open_live_window(
    ide: &Entity<Ide>,
    form: FormSpec,
    session: &DebugSession,
    rx: &std::sync::mpsc::Receiver<DebugEvent>,
    cx: &mut gpui::VisualTestContext,
) -> (WindowHandle<FormRunner>, gpui::VisualTestContext) {
    ide.update(cx, |ide, cx| {
        ide.form = form;
        ide.session = Some(session.clone());
        cx.notify();
    });
    // the session announces itself and shows its startup form
    pump_until_form(ide, rx, cx);
    cx.run_until_parked();
    let handle = ide
        .read_with(cx, |ide, _| ide.form_window())
        .expect("the form window is open");
    let mut win = gpui::VisualTestContext::from_window(handle.into(), cx);
    win.simulate_resize(size(px(500.0), px(400.0)));
    win.run_until_parked();
    (handle, win)
}

#[gpui::test]
fn checkbox_option_and_control_array_events(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let (form, session, rx) = interactive_session();
    let (handle, mut win) = open_live_window(&ide, form, &session, &rx, cx);
    let click = |win: &mut gpui::VisualTestContext, sel: &'static str| {
        let b = win.debug_bounds(sel).unwrap_or_else(|| panic!("{}", sel));
        win.simulate_click(b.center(), gpui::Modifiers::none());
    };
    let label = |win: &gpui::VisualTestContext| -> String {
        handle
            .read_with(win, |r, _| r.form.controls[0].caption.clone())
            .unwrap()
    };

    // a click checks the box, and the handler reads Value = 1
    click(&mut win, "run-Check1");
    pump_until_form(&ide, &rx, cx);
    win.run_until_parked();
    assert_eq!(label(&win), "check=1");
    handle
        .read_with(&win, |r, _| assert_eq!(r.state[1].value, 1))
        .unwrap();
    // a second click unchecks it
    click(&mut win, "run-Check1");
    pump_until_form(&ide, &rx, cx);
    win.run_until_parked();
    assert_eq!(label(&win), "check=0");

    // choosing Option1 then Option2 leaves only Option2 chosen
    click(&mut win, "run-Option1");
    click(&mut win, "run-Option2");
    pump_until_form(&ide, &rx, cx);
    win.run_until_parked();
    assert_eq!(label(&win), "opt2=TrueFalse");
    handle
        .read_with(&win, |r, _| {
            assert_eq!((r.state[2].value, r.state[3].value), (0, 1));
        })
        .unwrap();

    // the second element of the array raises Cmd_Click with Index = 1, and
    // its handler moves the focus to Text1
    click(&mut win, "run-Cmd(1)");
    pump_until_form(&ide, &rx, cx);
    win.run_until_parked();
    assert_eq!(label(&win), "cmd1");
    // Cmd_Click called Text1.SetFocus: the text box now has the keyboard
    let runner = handle.entity(&win).expect("the runner");
    let focused = runner.update_in(&mut win, |r, window, _| r.focus[&7].is_focused(window));
    assert!(focused, "SetFocus gave Text1 the focus");
    // the first element is told it is element 0
    click(&mut win, "run-Cmd(0)");
    pump_until_form(&ide, &rx, cx);
    win.run_until_parked();
    assert_eq!(label(&win), "cmd0");
    session.send(DebugCommand::Stop);
}

/// Let the program run what the window asked of it, and show the result.
pub(super) fn settle(
    ide: &Entity<Ide>,
    rx: &std::sync::mpsc::Receiver<DebugEvent>,
    win: &mut gpui::VisualTestContext,
    cx: &mut gpui::VisualTestContext,
) {
    std::thread::sleep(std::time::Duration::from_millis(150));
    while let Ok(ev) = rx.try_recv() {
        ide.update(cx, |ide, cx| ide.on_debug_event(ev, cx));
    }
    cx.run_until_parked();
    win.run_until_parked();
}

#[gpui::test]
fn timers_fire_in_the_live_window_and_obey_enabled(cx: &mut gpui::TestAppContext) {
    let (ide, cx) = ide_window(cx);
    let (form, session, rx) = interactive_session();
    let (handle, mut win) = open_live_window(&ide, form, &session, &rx, cx);
    let label = |win: &gpui::VisualTestContext| -> String {
        handle
            .read_with(win, |r, _| r.form.controls[0].caption.clone())
            .unwrap()
    };
    handle
        .read_with(&win, |r, _| assert_eq!(r.timers.len(), 1, "Timer1 runs"))
        .unwrap();

    // one interval: Timer1_Timer runs once
    win.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    settle(&ide, &rx, &mut win, cx);
    assert_eq!(label(&win), "tick1");

    // the second tick sets Timer1.Enabled = False, which stops the timer
    win.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    settle(&ide, &rx, &mut win, cx);
    assert_eq!(label(&win), "tick2");
    handle
        .read_with(&win, |r, _| {
            assert!(!r.form.controls[4].enabled);
            assert!(r.timers.is_empty(), "a disabled timer is stopped");
        })
        .unwrap();

    // more time brings no more ticks
    win.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    settle(&ide, &rx, &mut win, cx);
    assert_eq!(label(&win), "tick2");
    session.send(DebugCommand::Stop);
}
