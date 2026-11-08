// The `Action` derive emits `gpui::` paths; the kit is our GPUI.
extern crate gpui_kit as gpui;

mod claude;
mod drag_drop;
mod fixtures;
mod leader;
mod main_window;
mod picker;
mod sidebar_view;
mod transcript_view;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use lodestar_core::session::SessionStore;

use lodestar_core::session::SessionSource;

use crate::claude::ClaudeSource;
use crate::fixtures::FixtureSource;
use crate::main_window::MainWindow;

/// Set to `1` to run on the bundled fixture sessions instead of real Agents.
const FIXTURES_VAR: &str = "LODESTAR_FIXTURES";

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        transcript_view::bind_keys(cx);
        leader::init(cx);

        let mut source: Box<dyn SessionSource> = if std::env::var_os(FIXTURES_VAR).is_some() {
            Box::new(FixtureSource::new())
        } else {
            Box::new(ClaudeSource::from_env())
        };
        let sessions = SessionStore::load(source.as_mut());
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1280.), px(800.)), cx)),
            app_id: Some("lodestar".into()),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            let view = cx.new(|cx| MainWindow::new(sessions, source, cx));
            window.focus(&view.read(cx).focus_handle(), cx);
            view
        })
        .expect("failed to open the Lodestar window");
        cx.activate(true);
    });
}
