//! The IDE's About window: the app icon, its name and its version, as a Mac
//! app's About window shows them. Opened from the application menu's About.

use super::*;

use gpui::img;

/// The About window's content: the icon, the name and the version.
pub struct AboutView {
    /// The app icon's file, when one is found (see `package_command::app_icon`).
    pub icon: Option<std::path::PathBuf>,
}

impl AboutView {
    pub fn new(icon: Option<std::path::PathBuf>) -> Self {
        AboutView { icon }
    }
}

/// The name shown under the icon.
pub const APP_NAME: &str = "OxidizedBASIC";

/// The version line: the crate's version, as the About window shows it.
pub fn version_line() -> String {
    format!("Version {}", env!("CARGO_PKG_VERSION"))
}

impl gpui::Render for AboutView {
    fn render(&mut self, _win: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let icon = self.icon.clone().map(|path| {
            img(path)
                .w(px(128.))
                .h(px(128.))
                .into_any_element()
        });
        div()
            .size_full()
            .bg(rgb(0xF6F6F6))
            .text_color(rgb(0x1D1D1F))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .p_6()
            .children(icon)
            .child(
                div()
                    .text_xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(APP_NAME),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x3A3A3C))
                    .child(SharedString::from(version_line())),
            )
    }
}

impl Ide {
    /// Application menu > About OxidizedBASIC: open the About window.
    pub(super) fn open_about(&mut self, cx: &mut Context<Self>) {
        let icon = super::package_command::app_icon();
        let bounds = Bounds::centered(None, size(px(340.), px(300.)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(SharedString::from(format!("About {}", APP_NAME))),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_win, cx| cx.new(|_| AboutView::new(icon)),
        );
        if let Err(e) = opened {
            self.status = format!("cannot open About: {}", e);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_line_is_the_crate_version() {
        assert_eq!(version_line(), format!("Version {}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn the_app_is_named_oxidizedbasic() {
        assert_eq!(APP_NAME, "OxidizedBASIC");
    }
}
