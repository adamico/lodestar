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

/// Set to `1` (or `true`) to run on the bundled fixture sessions instead of real Agents.
const FIXTURES_VAR: &str = "LODESTAR_FIXTURES";

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        transcript_view::bind_keys(cx);
        leader::init(cx);

        let mut source: Box<dyn SessionSource> = if fixtures_wanted(std::env::var_os(FIXTURES_VAR).as_deref()) {
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
        let (_, view) = gpui_kit::open_window(options, cx, |window, cx| {
            let view = cx.new(|cx| MainWindow::new(sessions, source, cx));
            window.focus(&view.read(cx).focus_handle(), cx);
            view
        })
        .expect("failed to open the Lodestar window");
        // Quitting (here, closing the last window) stops every Agent before the process
        // goes; the source's Drop is the backstop should the window not get this far.
        cx.on_app_quit(move |cx| {
            view.update(cx, |main_window, _| main_window.source.shutdown());
            async {}
        })
        .detach();
        cx.activate(true);
    });
}

/// Whether [`FIXTURES_VAR`] set to `value` asks for fixtures: only `1` or `true`, in any case.
fn fixtures_wanted(value: Option<&std::ffi::OsStr>) -> bool {
    value
        .and_then(|value| value.to_str())
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

#[cfg(test)]
mod tests {
    use super::fixtures_wanted;

    #[test]
    fn fixtures_are_on_only_for_one_or_true() {
        for on in ["1", "true", "TRUE", "True"] {
            assert!(fixtures_wanted(Some(on.as_ref())), "{on}");
        }
        for off in ["0", "false", "", "yes"] {
            assert!(!fixtures_wanted(Some(off.as_ref())), "{off}");
        }
        assert!(!fixtures_wanted(None));
    }
}
