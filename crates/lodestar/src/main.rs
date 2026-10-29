mod fixtures;
mod workspace;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use lodestar_core::session::SessionStore;

use crate::fixtures::FixtureSource;
use crate::workspace::Workspace;

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
        gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Workspace::new(sessions)))
            .expect("failed to open the Lodestar window");
        cx.activate(true);
    });
}
