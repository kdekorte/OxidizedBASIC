//! The native file dialogs that a BASIC program opens with `OpenFileDialog`
//! and `SaveFileDialog`. The program waits on a channel while the UI shows the
//! picker; the choice is sent back there, filtered the way the program asked.
//! A cancelled dialog, or a choice the filter rejects, is `None` (VB's "").

use super::*;
use std::sync::mpsc::Sender;

/// Whether `path`'s file name matches one of the `;`-separated patterns in
/// `filter` (`*.bas;*.oxproj`), by the rule the runtime and the file lists
/// share. An empty filter accepts any file.
pub(super) fn matches_filter(path: &std::path::Path, filter: &str) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    oxbasic_runtime::pattern_matches(&name, filter)
}

/// Show the native picker for an open (`save` false) or save dialog, then
/// answer the program on `reply` with the chosen path, or `None`.
pub(super) fn show_file_dialog(
    filter: String,
    save: bool,
    reply: Sender<Option<String>>,
    cx: &mut App,
) {
    if save {
        let dir = std::env::current_dir().unwrap_or_default();
        let rx = cx.prompt_for_new_path(&dir, None);
        cx.spawn(async move |_cx| {
            let chosen = match rx.await {
                Ok(Ok(Some(path))) => Some(path),
                _ => None,
            };
            let _ = reply.send(answer(chosen, &filter));
        })
        .detach();
    } else {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |_cx| {
            let chosen = match rx.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let _ = reply.send(answer(chosen, &filter));
        })
        .detach();
    }
}

/// The path the program gets: the choice if the filter accepts it.
fn answer(chosen: Option<std::path::PathBuf>, filter: &str) -> Option<String> {
    chosen
        .filter(|p| matches_filter(p, filter))
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_accepts_listed_extensions_only() {
        let bas = std::path::Path::new("/x/Game.BAS");
        assert!(matches_filter(bas, "*.bas;*.oxproj"));
        assert!(!matches_filter(
            std::path::Path::new("/x/notes.txt"),
            "*.bas;*.oxproj"
        ));
    }

    #[test]
    fn an_empty_filter_accepts_anything() {
        assert!(matches_filter(std::path::Path::new("/x/a.zip"), ""));
    }

    #[test]
    fn a_rejected_choice_is_none() {
        assert_eq!(answer(Some("/x/a.txt".into()), "*.bas"), None);
        assert_eq!(
            answer(Some("/x/a.bas".into()), "*.bas"),
            Some("/x/a.bas".to_string())
        );
        assert_eq!(answer(None, "*.bas"), None);
    }
}
