use lodestar_core::geometry::{Rect, Size};
use lodestar_core::session::SessionId;
use lodestar_core::window::{DIVIDER_THICKNESS, PaneId, WindowModel};

#[test]
fn launch_shows_two_empty_panes_side_by_side() {
    let model = WindowModel::new();

    let layout = model.layout(Size::new(1001.0, 600.0));

    let rects: Vec<Rect> = layout.panes.iter().map(|p| p.rect).collect();
    assert_eq!(
        rects,
        vec![
            Rect::new(0.0, 0.0, 500.0, 600.0),
            Rect::new(501.0, 0.0, 500.0, 600.0),
        ]
    );
    for pane in &layout.panes {
        assert_eq!(model.session_in(pane.pane), None);
    }
}

#[test]
fn launch_has_one_vertical_divider_between_the_panes() {
    let model = WindowModel::new();

    let layout = model.layout(Size::new(1001.0, 600.0));

    let rects: Vec<Rect> = layout.dividers.iter().map(|d| d.rect).collect();
    assert_eq!(rects, vec![Rect::new(500.0, 0.0, DIVIDER_THICKNESS, 600.0)]);
}

#[test]
fn launch_focuses_the_left_pane() {
    let model = WindowModel::new();

    let left = model.layout(Size::new(1001.0, 600.0)).panes[0].pane;

    assert_eq!(model.focused_pane(), left);
}

#[test]
fn clicking_a_pane_focuses_it() {
    let mut model = WindowModel::new();
    let right = model.layout(Size::new(1001.0, 600.0)).panes[1].pane;

    model.focus(right);

    assert_eq!(model.focused_pane(), right);
}

// Opening sessions

const AREA: Size = Size::new(1001.0, 600.0);

fn panes(model: &WindowModel) -> Vec<PaneId> {
    model.layout(AREA).panes.iter().map(|p| p.pane).collect()
}

/// A session is in at most one pane. Checked after every command in these tests.
fn assert_each_session_in_at_most_one_pane(model: &WindowModel) {
    let mut shown: Vec<SessionId> = panes(model)
        .into_iter()
        .filter_map(|pane| model.session_in(pane))
        .collect();
    let count = shown.len();
    shown.sort();
    shown.dedup();
    assert_eq!(shown.len(), count, "a session is shown in two panes");
}

#[test]
fn opening_a_session_fills_the_empty_focused_pane() {
    let mut model = WindowModel::new();
    let [left, right] = panes(&model)[..] else {
        panic!()
    };

    model.open_session(SessionId(7), model.focused_pane());
    assert_each_session_in_at_most_one_pane(&model);

    assert_eq!(model.session_in(left), Some(SessionId(7)));
    assert_eq!(model.session_in(right), None);
    assert_eq!(model.pane_showing(SessionId(7)), Some(left));
}

#[test]
fn opening_a_session_replaces_the_focused_panes_session() {
    let mut model = WindowModel::new();
    let left = model.focused_pane();
    model.open_session(SessionId(1), left);

    model.open_session(SessionId(2), left);
    assert_each_session_in_at_most_one_pane(&model);

    assert_eq!(model.session_in(left), Some(SessionId(2)));
    assert_eq!(model.pane_showing(SessionId(1)), None);
}

#[test]
fn opening_a_shown_session_focuses_its_pane_instead() {
    let mut model = WindowModel::new();
    let [left, right] = panes(&model)[..] else {
        panic!()
    };
    model.open_session(SessionId(1), left);
    model.focus(right);
    model.open_session(SessionId(2), right);

    model.open_session(SessionId(1), right);
    assert_each_session_in_at_most_one_pane(&model);

    assert_eq!(model.focused_pane(), left);
    assert_eq!(model.session_in(left), Some(SessionId(1)));
    assert_eq!(model.session_in(right), Some(SessionId(2)));
}
