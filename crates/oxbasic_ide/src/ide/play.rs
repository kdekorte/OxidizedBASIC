//! The run-only app: `oxbasic play <project.oxproj>` runs a project's program
//! with no editor or designer, showing only the forms the program shows. It is
//! the packaged app's window. The program runs on the same session the IDE's
//! Run uses, and each form window is the IDE's own `FormRunner`.
//!
//! The app ends when the program finishes, or when its last form window is
//! closed (the program is asked first, as in the IDE).

use super::debug_session::{DebugCommand, DebugEvent, DebugSession};
use super::project_program::project_program;
use super::*;
use std::cell::RefCell;
use std::rc::Rc;

gpui::actions!(play, [AboutApp, QuitApp]);

/// The packaged app's own menu bar: About, a separator and Quit. Shown at
/// start-up, and again when a form window is not in front.
fn set_app_menus(cx: &mut App) {
    cx.set_menus(vec![gpui::Menu {
        name: "App".into(),
        items: vec![
            gpui::MenuItem::action("About", AboutApp),
            gpui::MenuItem::separator(),
            gpui::MenuItem::action("Quit", QuitApp),
        ],
    }]);
}

/// The run-only app's state: the running session and the form windows it has
/// opened, by form index.
pub(super) struct PlayApp {
    session: Option<DebugSession>,
    evt_rx: Arc<Mutex<std::sync::mpsc::Receiver<DebugEvent>>>,
    /// The form windows, by the program's form index.
    windows: HashMap<usize, WindowHandle<FormRunner>>,
    /// The forms the program has shown at least once.
    shown: usize,
}

impl PlayApp {
    /// Start the project's program. A project that won't open or build is an
    /// error the caller reports.
    pub(super) fn start(file: &std::path::Path) -> Result<PlayApp, String> {
        let (spec, _program) = project_program(file)?;
        let (evt_tx, evt_rx) = std::sync::mpsc::channel::<DebugEvent>();
        let session =
            DebugSession::start_program(spec, Arc::new(Mutex::new(HashSet::new())), evt_tx);
        session.send(DebugCommand::Run);
        Ok(PlayApp {
            session: Some(session),
            evt_rx: Arc::new(Mutex::new(evt_rx)),
            windows: HashMap::new(),
            shown: 0,
        })
    }

    /// The events waiting from the program.
    pub(super) fn drain(&self) -> Vec<DebugEvent> {
        let rx = self.evt_rx.lock().unwrap();
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            out.push(ev);
        }
        out
    }

    /// Stop the program (the app is ending).
    pub(super) fn stop(&mut self) {
        if let Some(s) = self.session.take() {
            s.send(DebugCommand::Stop);
        }
    }
}

/// Run the project in `file` as an app. Returns once the app has ended; a
/// project that can't be run is reported and the process exits with status 1.
pub fn run_play(file: &std::path::Path) {
    let app = match PlayApp::start(file) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("oxbasic play: {}", e);
            std::process::exit(1);
        }
    };
    Application::new().run(move |cx: &mut App| {
        // the app's menu bar: About and Quit, as any Mac app has
        cx.on_action(|_: &AboutApp, cx| {
            let version = env!("CARGO_PKG_VERSION");
            let window = cx
                .active_window()
                .or_else(|| cx.windows().into_iter().next());
            if let Some(window) = window {
                let _ = window.update(cx, |_, win, cx| {
                    let _ = win.prompt(
                        gpui::PromptLevel::Info,
                        &format!("OxidizedBASIC {}", version),
                        Some("A Rust-native classic BASIC suite."),
                        &["OK"],
                        cx,
                    );
                });
            }
        });
        cx.on_action(|_: &QuitApp, cx| cx.quit());
        set_app_menus(cx);
        let app = Rc::new(RefCell::new(app));
        // poll the program's events and act on them, as the IDE does
        cx.spawn(async move |cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(40))
                .await;
            let events = app.borrow().drain();
            for ev in events {
                let _ = cx.update(|cx| handle_event(&app, ev, cx));
            }
        })
        .detach();
    });

    /// Act on one event from the program: open, update or close a form window,
    /// and report output or a failure. The app ends with the program, or with its
    /// last form window.
    fn handle_event(app: &Rc<RefCell<PlayApp>>, ev: DebugEvent, cx: &mut App) {
        match ev {
            DebugEvent::FormShown(snap) => {
                app.borrow_mut().shown += 1;
                open_window(app, snap, cx);
            }
            DebugEvent::Form(snap) => {
                let index = snap.form_index;
                let handle = app.borrow().windows.get(&index).copied();
                if let Some(handle) = handle {
                    let _ = handle.update(cx, |runner, win, cx| {
                        runner.show(&snap, win, cx);
                        cx.notify();
                    });
                }
            }
            DebugEvent::FormHidden(i) | DebugEvent::FormClosed(i) => {
                let handle = app.borrow_mut().windows.remove(&i);
                if let Some(handle) = handle {
                    let _ = handle.update(cx, |_, win, _| win.remove_window());
                }
                if app.borrow().windows.is_empty() && app.borrow().shown > 0 {
                    finish(app, cx);
                }
            }
            DebugEvent::Output(text) => eprint!("{}", text),
            DebugEvent::ChooseFile {
                filter,
                save,
                reply,
            } => super::choose_file::show_file_dialog(filter, save, reply, cx),
            DebugEvent::Failed(msg) => {
                eprintln!("oxbasic play: {}", msg);
                finish(app, cx);
            }
            DebugEvent::Finished(_) => finish(app, cx),
            _ => {}
        }
    }

    /// End the app: stop the program if it is still running, then quit.
    fn finish(app: &Rc<RefCell<PlayApp>>, cx: &mut App) {
        app.borrow_mut().stop();
        cx.quit();
    }

    /// Open the window for the form in `snap`, starting from what the program has
    /// made of it. The window is the IDE's own form window (`FormRunner`).
    fn open_window(app: &Rc<RefCell<PlayApp>>, snap: FormSnapshot, cx: &mut App) {
        let index = snap.form_index;
        if app.borrow().windows.contains_key(&index) {
            return;
        }
        let Some(session) = app.borrow().session.clone() else {
            return;
        };
        let form = snap.form.clone();
        let state = snap.state.clone();
        let bounds = Bounds::centered(
            None,
            size(px(form.width + 16.0), px(form.height + 16.0)),
            cx,
        );
        let title = SharedString::from(form.caption.clone());
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(title),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |win, cx| {
                cx.new(|cx| {
                    let mut runner = FormRunner::new(form.clone(), index, session.clone(), cx);
                    runner.state = state.clone();
                    // the form's menu is the application's bar while its window
                    // is in front (as in the IDE); otherwise the app's own bar
                    // (About and Quit) shows
                    let menus = form.menus.clone();
                    let sub = cx.observe_window_activation(win, move |runner, win, cx| {
                        if win.is_window_active() {
                            oxbasic_ui::menu_bar::install(&menus, cx);
                            // the menu's items are enabled only while the form
                            // holds the focus: its handler is on that path
                            win.focus(&runner.form_focus);
                        } else {
                            set_app_menus(cx);
                        }
                    });
                    runner._subscriptions.push(sub);
                    runner
                })
            },
        );
        match opened {
            Ok(handle) => {
                // the new window starts with its form focused, so its menu
                // items are enabled (activation alone doesn't focus the form)
                let _ = handle.update(cx, |runner, win, _| {
                    win.focus(&runner.form_focus);
                });
                app.borrow_mut().windows.insert(index, handle);
            }
            Err(e) => eprintln!("oxbasic play: form window failed: {}", e),
        }
    }
}
