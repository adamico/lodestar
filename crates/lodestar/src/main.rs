mod fixtures;
mod leader;
mod main_window;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use lodestar_core::session::SessionStore;

use crate::fixtures::FixtureSource;
use crate::main_window::MainWindow;

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        leader::init(cx);

        let sessions = SessionStore::load(&mut FixtureSource);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1280.), px(800.)), cx)),
            app_id: Some("lodestar".into()),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            let view = cx.new(|cx| MainWindow::new(sessions, cx));
            window.focus(&view.read(cx).focus_handle(), cx);
            view
        })
        .expect("failed to open the Lodestar window");
        cx.activate(true);
    });
}
