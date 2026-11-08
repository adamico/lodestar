mod fixtures;
mod main_window;
mod picker;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use lodestar_core::session::SessionStore;

use crate::fixtures::FixtureSource;
use crate::main_window::MainWindow;

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);

        let sessions = SessionStore::load(&mut FixtureSource);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1280.), px(800.)), cx)),
            app_id: Some("lodestar".into()),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| MainWindow::new(sessions, window, cx))
        })
        .expect("failed to open the Lodestar window");
        cx.activate(true);
    });
}
