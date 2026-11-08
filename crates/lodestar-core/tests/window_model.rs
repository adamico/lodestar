use lodestar_core::geometry::{Rect, Size};
use lodestar_core::window::{DIVIDER_THICKNESS, WindowModel};

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

mod open_session {
    use lodestar_core::geometry::Size;
    use lodestar_core::session::SessionId;
    use lodestar_core::window::{PaneId, WindowModel};

    const AREA: Size = Size::new(1001.0, 600.0);

    fn panes(model: &WindowModel) -> Vec<PaneId> {
        model.layout(AREA).panes.iter().map(|p| p.pane).collect()
    }

    #[test]
    fn opening_a_session_in_an_empty_focused_pane_fills_it() {
        let mut model = WindowModel::new();
        let left = model.focused_pane();

        model.open_session(SessionId(1), left);

        assert_eq!(model.session_in(left), Some(SessionId(1)));
        assert_eq!(model.focused_pane(), left);
        let right = panes(&model)[1];
        assert_eq!(model.session_in(right), None);
    }

    #[test]
    fn opening_a_session_in_an_occupied_pane_replaces_its_session() {
        let mut model = WindowModel::new();
        let left = model.focused_pane();
        model.open_session(SessionId(1), left);

        model.open_session(SessionId(2), left);

        assert_eq!(model.session_in(left), Some(SessionId(2)));
        let right = panes(&model)[1];
        assert_eq!(model.session_in(right), None);
    }

    #[test]
    fn a_replaced_session_is_no_longer_shown_anywhere() {
        let mut model = WindowModel::new();
        let left = model.focused_pane();
        model.open_session(SessionId(1), left);
        model.open_session(SessionId(2), left);

        let all = [SessionId(1), SessionId(2), SessionId(3)];
        assert_eq!(model.not_shown(&all), vec![SessionId(1), SessionId(3)]);
        assert_eq!(model.sessions_shown(), vec![SessionId(2)]);
        assert_eq!(model.pane_showing(SessionId(1)), None);
        assert_eq!(model.pane_showing(SessionId(2)), Some(left));
    }

    #[test]
    fn opening_a_session_already_shown_elsewhere_focuses_that_pane_instead() {
        let mut model = WindowModel::new();
        let [left, right] = panes(&model)[..] else {
            unreachable!()
        };
        model.open_session(SessionId(1), left);
        model.open_session(SessionId(2), right);
        model.focus(right);

        model.open_session(SessionId(1), right);

        assert_eq!(model.focused_pane(), left);
        assert_eq!(model.session_in(left), Some(SessionId(1)));
        assert_eq!(model.session_in(right), Some(SessionId(2)));
    }

    #[test]
    fn reopening_a_session_in_its_own_pane_changes_nothing() {
        let mut model = WindowModel::new();
        let left = model.focused_pane();
        model.open_session(SessionId(1), left);

        model.open_session(SessionId(1), left);

        assert_eq!(model.focused_pane(), left);
        assert_eq!(model.sessions_shown(), vec![SessionId(1)]);
    }

    /// Every session appears in at most one pane, judged only through what the model reports.
    fn assert_one_pane_per_session(model: &WindowModel) {
        for session in model.sessions_shown() {
            let panes_showing = panes(model)
                .into_iter()
                .filter(|pane| model.session_in(*pane) == Some(session))
                .count();
            assert_eq!(
                panes_showing, 1,
                "{session:?} is shown in {panes_showing} panes"
            );
            assert_eq!(
                model.pane_showing(session).map(|p| model.session_in(p)),
                Some(Some(session))
            );
        }
    }

    #[test]
    fn a_session_is_in_at_most_one_pane_after_every_command() {
        let mut model = WindowModel::new();
        let [left, right] = panes(&model)[..] else {
            unreachable!()
        };
        let commands: Vec<Box<dyn Fn(&mut WindowModel)>> = vec![
            Box::new(move |m| m.open_session(SessionId(1), left)),
            Box::new(move |m| m.focus(right)),
            Box::new(move |m| m.open_session(SessionId(2), right)),
            Box::new(move |m| m.open_session(SessionId(1), right)),
            Box::new(move |m| m.open_session(SessionId(2), left)),
            Box::new(move |m| m.open_session(SessionId(3), left)),
            Box::new(move |m| m.open_session(SessionId(2), left)),
            Box::new(move |m| m.open_session(SessionId(3), right)),
            Box::new(move |m| m.focus(left)),
            Box::new(move |m| m.open_session(SessionId(3), left)),
        ];

        assert_one_pane_per_session(&model);
        for command in commands {
            command(&mut model);
            assert_one_pane_per_session(&model);
        }
    }
}
