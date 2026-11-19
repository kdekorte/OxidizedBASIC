//! The running program's windows: one OS window per form it has shown.
//!
//! In classic VB a form *is* a window of the application. The program
//! thread tells the IDE which forms to show, hide and close
//! ([`DebugEvent::FormShown`] and friends); this file opens a
//! [`FormRunner`] window for each, keeps it up to date with what the
//! program does to the form, and ends the program when its last window is
//! closed.

use super::*;

impl Ide {
    /// The program changed form `snap.form_index`: remember it, and show it
    /// in that form's window if one is open.
    pub(super) fn on_form_changed(&mut self, snap: FormSnapshot, cx: &mut Context<Self>) {
        let index = snap.form_index;
        // the program's copy is authoritative: it has the user's edits too
        self.live_forms
            .insert(index, (snap.form.clone(), snap.state.clone()));
        if let Some(handle) = self.form_windows.get(&index).copied() {
            let _ = handle.update(cx, |runner, win, cx| {
                runner.show(&snap, win, cx);
                cx.notify();
            });
        }
    }

    /// The program shows form `snap.form_index`: open its window, starting
    /// from what the program has made of the form so far.
    pub(super) fn open_form_window(&mut self, snap: FormSnapshot, cx: &mut Context<Self>) {
        let index = snap.form_index;
        self.live_forms
            .insert(index, (snap.form.clone(), snap.state.clone()));
        if self.form_windows.contains_key(&index) {
            return;
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        let (form, state) = (snap.form, snap.state);
        let handlers = self.live_handlers.get(index).cloned();
        let base_dir = self
            .workspace
            .file
            .as_deref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
        let ide = cx.entity().downgrade();
        // later windows are offset so they don't hide the first
        let mut bounds = Bounds::centered(
            None,
            size(px(form.width + 16.0), px(form.height + 16.0)),
            cx,
        );
        bounds.origin.x += px(28.0 * self.form_windows.len() as f32);
        bounds.origin.y += px(28.0 * self.form_windows.len() as f32);
        let title = SharedString::from(form.caption.clone());
        match cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(title),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |win, cx| {
                let runner = cx.new(|cx| {
                    let mut runner = FormRunner::new(form.clone(), index, session.clone(), cx);
                    runner.state = state.clone();
                    runner.handlers = handlers.clone();
                    runner.base_dir = base_dir.clone();
                    // Form_Activate / Form_Deactivate
                    let sub =
                        cx.observe_window_activation(win, |runner: &mut FormRunner, win, _cx| {
                            runner.activation_changed(win.is_window_active());
                        });
                    runner._subscriptions.push(sub);
                    runner
                });
                // Closing a form's window asks the program first: its
                // QueryUnload and Unload may cancel (Cancel = True). The
                // window goes when the program unloads the form.
                let weak = ide.clone();
                win.on_window_should_close(cx, move |_win, cx| match weak.upgrade() {
                    Some(ide) => ide.update(cx, |ide, cx| ide.form_window_close_asked(index, cx)),
                    None => true,
                });
                runner
            },
        ) {
            Ok(handle) => {
                self.form_windows.insert(index, handle);
            }
            Err(e) => self
                .output
                .push_str(&format!("[form window failed: {}]\n", e)),
        }
    }

    /// The user clicked form `index`'s close box. A live program is asked
    /// first (its `QueryUnload` / `Unload` may set `Cancel`): the window
    /// stays until the program unloads the form, which closes it through
    /// [`DebugEvent::FormClosed`], and the program ends with its last
    /// form. Without a live program the window just closes. Returns
    /// whether the window may close now.
    fn form_window_close_asked(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        match &self.session {
            Some(s) if self.live => {
                s.send(DebugCommand::Close(index));
                false
            }
            _ => {
                self.form_window_closed(index, cx);
                true
            }
        }
    }

    /// Form `index`'s window is gone. With forms still open the program
    /// goes on; with none left it ends.
    fn form_window_closed(&mut self, index: usize, cx: &mut Context<Self>) {
        self.form_windows.remove(&index);
        self.live_forms.remove(&index);
        // the program unloads the form (and ends with its last one); with
        // no window left it is over either way
        if let Some(s) = &self.session {
            s.send(DebugCommand::Closed(index));
        }
        if self.live && self.form_windows.is_empty() {
            self.stop_program(cx);
        }
    }

    /// Take down form `index`'s window (the program hid or unloaded it).
    pub(super) fn close_form_window_at(
        &mut self,
        index: usize,
        forget: bool,
        cx: &mut Context<Self>,
    ) {
        if forget {
            self.live_forms.remove(&index);
        }
        if let Some(handle) = self.form_windows.remove(&index) {
            let _ = handle.update(cx, |_, win, _| win.remove_window());
        }
    }

    /// Take down every window (program finished, failed, or was stopped).
    pub(super) fn close_form_window(&mut self, cx: &mut Context<Self>) {
        self.live_forms.clear();
        for (_, handle) in std::mem::take(&mut self.form_windows) {
            let _ = handle.update(cx, |_, win, _| win.remove_window());
        }
    }

    /// The program answered a key sent from one of the form windows.
    pub(super) fn on_key_result(
        &mut self,
        serial: u64,
        keep: bool,
        typed: Option<char>,
        cx: &mut Context<Self>,
    ) {
        for handle in self.form_windows.values() {
            let _ = handle.update(cx, |runner, _, cx| {
                if runner.key_answered(serial, keep, typed) {
                    cx.notify();
                }
            });
        }
    }

    /// The first form's window, if open (the one a single-form program has).
    #[cfg(test)]
    pub(super) fn form_window(&self) -> Option<WindowHandle<FormRunner>> {
        self.form_windows.get(&0).copied()
    }
}
