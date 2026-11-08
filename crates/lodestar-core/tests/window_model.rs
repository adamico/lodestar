use lodestar_core::geometry::{Rect, Size};
use lodestar_core::session::SessionId;
use lodestar_core::window::{DIVIDER_THICKNESS, Direction, PaneLayout, Refused, WindowModel};

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

/// A pane area whose two launch panes are wide (800×600 each).
const WIDE_AREA: Size = Size::new(1601.0, 600.0);
/// A pane area whose two launch panes are tall (500×600 each).
const TALL_AREA: Size = Size::new(1001.0, 600.0);

fn rects(model: &WindowModel, area: Size) -> Vec<Rect> {
    model.layout(area).panes.iter().map(|p| p.rect).collect()
}

#[test]
fn split_a_wide_pane_puts_a_new_empty_focused_pane_on_its_right() {
    let mut model = WindowModel::new();
    let left = model.focused_pane();

    let new_pane = model.split_auto(left, WIDE_AREA).expect("the halves fit");

    let layout = model.layout(WIDE_AREA);
    assert_eq!(
        layout.panes[0],
        PaneLayout {
            pane: left,
            rect: Rect::new(0.0, 0.0, 399.5, 600.0)
        }
    );
    assert_eq!(
        layout.panes[1],
        PaneLayout {
            pane: new_pane,
            rect: Rect::new(400.5, 0.0, 399.5, 600.0)
        }
    );
    assert_eq!(layout.panes.len(), 3);
    assert_eq!(model.focused_pane(), new_pane);
    assert_eq!(model.session_in(new_pane), None);
}

#[test]
fn split_a_tall_pane_puts_the_new_pane_below_it() {
    let mut model = WindowModel::new();
    let left = model.focused_pane();

    let new_pane = model.split_auto(left, TALL_AREA).expect("the halves fit");

    let layout = model.layout(TALL_AREA);
    assert_eq!(layout.panes[0].rect, Rect::new(0.0, 0.0, 500.0, 299.5));
    assert_eq!(
        layout.panes[1],
        PaneLayout {
            pane: new_pane,
            rect: Rect::new(0.0, 300.5, 500.0, 299.5)
        }
    );
}

#[test]
fn split_right_forces_a_tall_pane_to_split_side_by_side() {
    let mut model = WindowModel::new();
    // Tall panes of 800×1000: an automatic split would go down.
    let area = Size::new(1601.0, 1000.0);
    let left = model.focused_pane();

    let new_pane = model
        .split(left, Direction::Right, area)
        .expect("the halves fit");

    let layout = model.layout(area);
    assert_eq!(layout.panes[0].rect, Rect::new(0.0, 0.0, 399.5, 1000.0));
    assert_eq!(
        layout.panes[1],
        PaneLayout {
            pane: new_pane,
            rect: Rect::new(400.5, 0.0, 399.5, 1000.0)
        }
    );
}

#[test]
fn split_down_forces_a_wide_pane_to_split_top_and_bottom() {
    let mut model = WindowModel::new();
    let left = model.focused_pane();

    let new_pane = model
        .split(left, Direction::Down, WIDE_AREA)
        .expect("the halves fit");

    let layout = model.layout(WIDE_AREA);
    assert_eq!(layout.panes[0].rect, Rect::new(0.0, 0.0, 800.0, 299.5));
    assert_eq!(
        layout.panes[1],
        PaneLayout {
            pane: new_pane,
            rect: Rect::new(0.0, 300.5, 800.0, 299.5)
        }
    );
}

#[test]
fn split_that_would_make_a_half_too_narrow_is_refused_and_changes_nothing() {
    let mut model = WindowModel::new();
    let left = model.focused_pane();
    let before = rects(&model, TALL_AREA);

    let result = model.split(left, Direction::Right, TALL_AREA);

    assert_eq!(result, Err(Refused::BelowMinimumSize));
    assert_eq!(rects(&model, TALL_AREA), before);
    assert_eq!(model.focused_pane(), left);
}

#[test]
fn split_that_would_make_a_half_too_short_is_refused() {
    let mut model = WindowModel::new();
    // Panes of 800×399: halving the height leaves 199, just under the minimum.
    let area = Size::new(1601.0, 399.0);
    let left = model.focused_pane();

    let result = model.split(left, Direction::Down, area);

    assert_eq!(result, Err(Refused::BelowMinimumSize));
}

#[test]
fn each_split_adds_exactly_one_pane_and_the_halves_tile_the_old_rect() {
    let mut model = WindowModel::new();
    let left = model.focused_pane();
    let old = rects(&model, WIDE_AREA)[0];

    let new_pane = model.split_auto(left, WIDE_AREA).expect("the halves fit");
    let new_pane = model
        .split_auto(new_pane, WIDE_AREA)
        .expect("the halves fit");

    let layout = model.layout(WIDE_AREA);
    assert_eq!(layout.panes.len(), 4);
    let halves: Vec<Rect> = layout
        .panes
        .iter()
        .filter(|p| p.pane == left || p.pane == new_pane)
        .map(|p| p.rect)
        .collect();
    assert_eq!(halves[0].x, old.x);
    assert_eq!(halves[0].width, 399.5);
    assert_eq!(halves[1].y, 300.5);
    assert_eq!(halves[1].height, 299.5);
    assert_eq!(halves[1].x + halves[1].width, old.x + old.width);
}

#[test]
fn close_a_pane_and_its_sibling_takes_the_space() {
    let mut model = WindowModel::new();
    let layout = model.layout(TALL_AREA);
    let (left, right) = (layout.panes[0].pane, layout.panes[1].pane);

    model.close(right);

    let layout = model.layout(TALL_AREA);
    assert_eq!(
        layout.panes,
        vec![PaneLayout {
            pane: left,
            rect: Rect::new(0.0, 0.0, 1001.0, 600.0)
        }]
    );
    assert_eq!(layout.dividers, vec![]);
}

#[test]
fn close_the_focused_pane_and_focus_moves_to_its_sibling() {
    let mut model = WindowModel::new();
    let layout = model.layout(TALL_AREA);
    let (left, right) = (layout.panes[0].pane, layout.panes[1].pane);
    model.focus(right);

    model.close(right);

    assert_eq!(model.focused_pane(), left);
}

#[test]
fn close_an_unfocused_pane_and_focus_stays_put() {
    let mut model = WindowModel::new();
    let layout = model.layout(TALL_AREA);
    let (left, right) = (layout.panes[0].pane, layout.panes[1].pane);

    model.close(right);

    assert_eq!(model.focused_pane(), left);
}

#[test]
fn close_a_nested_pane_and_only_its_sibling_grows() {
    let mut model = WindowModel::new();
    let layout = model.layout(WIDE_AREA);
    let (left, right) = (layout.panes[0].pane, layout.panes[1].pane);
    let bottom_right = model
        .split(right, Direction::Down, WIDE_AREA)
        .expect("fits");

    model.close(right);

    let layout = model.layout(WIDE_AREA);
    assert_eq!(
        layout.panes,
        vec![
            PaneLayout {
                pane: left,
                rect: Rect::new(0.0, 0.0, 800.0, 600.0)
            },
            PaneLayout {
                pane: bottom_right,
                rect: Rect::new(801.0, 0.0, 800.0, 600.0)
            },
        ]
    );
}

#[test]
fn close_the_last_pane_and_one_new_empty_pane_remains() {
    let mut model = WindowModel::new();
    let layout = model.layout(TALL_AREA);
    let (left, right) = (layout.panes[0].pane, layout.panes[1].pane);
    model.close(right);

    model.close(left);

    let layout = model.layout(TALL_AREA);
    assert_eq!(layout.panes.len(), 1);
    let remaining = layout.panes[0].pane;
    assert_ne!(remaining, left);
    assert_eq!(layout.panes[0].rect, Rect::new(0.0, 0.0, 1001.0, 600.0));
    assert_eq!(model.session_in(remaining), None);
    assert_eq!(model.focused_pane(), remaining);
}

#[test]
fn close_a_pane_that_no_longer_exists_and_nothing_changes() {
    let mut model = WindowModel::new();
    let right = model.layout(TALL_AREA).panes[1].pane;
    model.close(right);
    let before = rects(&model, TALL_AREA);

    model.close(right);

    assert_eq!(rects(&model, TALL_AREA), before);
}

#[test]
fn close_a_pane_showing_a_session_and_the_session_is_merely_not_shown() {
    let mut model = WindowModel::new();
    let right = model.layout(TALL_AREA).panes[1].pane;
    let session = SessionId(7);
    model.open_session(session, right);

    model.close(right);

    assert_eq!(model.not_shown(&[session]), vec![session]);
    assert_eq!(model.sessions_shown(), vec![]);
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

    type Command = Box<dyn Fn(&mut WindowModel)>;

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
        let commands: Vec<Command> = vec![
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

mod resize {
    use lodestar_core::geometry::{Rect, Size};
    use lodestar_core::window::{DIVIDER_THICKNESS, WindowModel};

    /// Two launch panes of 500×600 each.
    const AREA: Size = Size::new(1001.0, 600.0);

    fn rects(model: &WindowModel) -> Vec<Rect> {
        model.layout(AREA).panes.iter().map(|p| p.rect).collect()
    }

    #[test]
    fn set_a_divider_ratio_and_the_panes_on_both_sides_follow_it() {
        let mut model = WindowModel::new();
        let divider = model.layout(AREA).dividers[0].id;

        model.set_ratio(divider, 0.4, AREA).expect("both sides fit");

        assert_eq!(
            rects(&model),
            vec![
                Rect::new(0.0, 0.0, 400.0, 600.0),
                Rect::new(401.0, 0.0, 600.0, 600.0),
            ]
        );
        assert_eq!(
            model.layout(AREA).dividers[0].rect,
            Rect::new(400.0, 0.0, DIVIDER_THICKNESS, 600.0)
        );
    }
}

mod resize_limits {
    use lodestar_core::geometry::{Rect, Size};
    use lodestar_core::window::{Direction, Refused, WindowModel};

    /// Two launch panes of 500×600 each.
    const AREA: Size = Size::new(1001.0, 600.0);

    fn rects(model: &WindowModel) -> Vec<Rect> {
        model.layout(AREA).panes.iter().map(|p| p.rect).collect()
    }

    #[test]
    fn a_ratio_that_would_make_the_left_pane_too_narrow_is_refused_and_changes_nothing() {
        let mut model = WindowModel::new();
        let divider = model.layout(AREA).dividers[0].id;
        let before = rects(&model);

        // 0.3 of 1000 is 300px, under the 320px minimum.
        let result = model.set_ratio(divider, 0.3, AREA);

        assert_eq!(result, Err(Refused::BelowMinimumSize));
        assert_eq!(rects(&model), before);
    }

    #[test]
    fn a_ratio_that_would_make_the_right_pane_too_narrow_is_refused() {
        let mut model = WindowModel::new();
        let divider = model.layout(AREA).dividers[0].id;

        assert_eq!(
            model.set_ratio(divider, 0.7, AREA),
            Err(Refused::BelowMinimumSize)
        );
        assert_eq!(model.set_ratio(divider, 0.68, AREA), Ok(()));
    }

    #[test]
    fn a_ratio_is_refused_when_a_pane_nested_on_that_side_would_be_too_narrow() {
        // Wide area: 1601×600, launch panes 800×600. Split the right one side by side, so two
        // 399.5px panes sit right of the launch divider; together they need 641px.
        let area = Size::new(1601.0, 600.0);
        let mut model = WindowModel::new();
        let right = model.layout(area).panes[1].pane;
        model.split(right, Direction::Right, area).expect("fits");
        let launch_divider = model.layout(area).dividers[0].id;

        // 0.6 leaves 640px for the right side, 1px short of the two minimum panes.
        assert_eq!(
            model.set_ratio(launch_divider, 0.6, area),
            Err(Refused::BelowMinimumSize)
        );
        assert_eq!(model.set_ratio(launch_divider, 0.59, area), Ok(()));
    }

    #[test]
    fn set_the_ratio_of_a_divider_that_no_longer_exists_and_it_is_refused() {
        let mut model = WindowModel::new();
        let layout = model.layout(AREA);
        let (divider, right) = (layout.dividers[0].id, layout.panes[1].pane);
        model.close(right);

        assert_eq!(
            model.set_ratio(divider, 0.4, AREA),
            Err(Refused::NoSuchDivider)
        );
    }

    #[test]
    fn clamping_a_ratio_stops_each_side_at_the_minimum_pane_size() {
        let model = WindowModel::new();
        let divider = model.layout(AREA).dividers[0].id;

        // 1000px of space: 320px minimum each side is ratio 0.32 on the left, 0.68 on the right.
        assert_eq!(model.clamped_ratio(divider, 0.1, AREA), 0.32);
        assert_eq!(model.clamped_ratio(divider, 0.9, AREA), 0.68);
        assert_eq!(model.clamped_ratio(divider, 0.5, AREA), 0.5);
    }

    #[test]
    fn clamping_a_ratio_keeps_room_for_the_panes_nested_on_each_side() {
        let area = Size::new(1601.0, 600.0);
        let mut model = WindowModel::new();
        let right = model.layout(area).panes[1].pane;
        model.split(right, Direction::Right, area).expect("fits");
        let launch_divider = model.layout(area).dividers[0].id;

        // 1600px of space; the right side needs 641px, so the left may take at most 959px.
        let ratio = model.clamped_ratio(launch_divider, 1.0, area);

        assert_eq!(ratio, 959.0 / 1600.0);
        assert_eq!(model.set_ratio(launch_divider, ratio, area), Ok(()));
    }
}

mod resize_by_keyboard {
    use lodestar_core::geometry::{Rect, Size};
    use lodestar_core::window::{Direction, Refused, WindowModel};

    /// Two launch panes of 500×600 each.
    const AREA: Size = Size::new(1001.0, 600.0);
    const STEP: f32 = 40.0;

    fn rects(model: &WindowModel) -> Vec<Rect> {
        model.layout(AREA).panes.iter().map(|p| p.rect).collect()
    }

    #[test]
    fn grow_the_left_pane_to_the_right_and_it_gains_a_step_from_its_neighbour() {
        let mut model = WindowModel::new();

        model
            .resize_in_direction(Direction::Right, STEP, AREA)
            .expect("there is room");

        assert_eq!(
            rects(&model),
            vec![
                Rect::new(0.0, 0.0, 540.0, 600.0),
                Rect::new(541.0, 0.0, 460.0, 600.0),
            ]
        );
    }

    #[test]
    fn grow_the_left_pane_twice_and_it_gains_two_steps() {
        let mut model = WindowModel::new();

        model
            .resize_in_direction(Direction::Right, STEP, AREA)
            .expect("there is room");
        model
            .resize_in_direction(Direction::Right, STEP, AREA)
            .expect("there is room");

        let width = rects(&model)[0].width;
        assert!((width - 580.0).abs() < 0.01, "left pane is {width}px wide");
    }

    #[test]
    fn grow_the_right_pane_to_the_left_and_the_divider_moves_left() {
        let mut model = WindowModel::new();
        let right = model.layout(AREA).panes[1].pane;
        model.focus(right);

        model
            .resize_in_direction(Direction::Left, STEP, AREA)
            .expect("there is room");

        assert_eq!(
            rects(&model),
            vec![
                Rect::new(0.0, 0.0, 460.0, 600.0),
                Rect::new(461.0, 0.0, 540.0, 600.0),
            ]
        );
    }

    #[test]
    fn grow_a_pane_toward_a_side_with_no_divider_and_it_is_refused() {
        let mut model = WindowModel::new();
        let before = rects(&model);

        assert_eq!(
            model.resize_in_direction(Direction::Left, STEP, AREA),
            Err(Refused::NoDividerOnThatSide)
        );
        assert_eq!(
            model.resize_in_direction(Direction::Up, STEP, AREA),
            Err(Refused::NoDividerOnThatSide)
        );
        assert_eq!(rects(&model), before);
    }

    #[test]
    fn grow_a_pane_past_its_neighbours_minimum_and_it_is_refused() {
        let mut model = WindowModel::new();
        for _ in 0..4 {
            model
                .resize_in_direction(Direction::Right, STEP, AREA)
                .expect("there is room");
        }
        // The right pane is now 340px wide; another step would leave it 300px.
        let before = rects(&model);

        assert_eq!(
            model.resize_in_direction(Direction::Right, STEP, AREA),
            Err(Refused::BelowMinimumSize)
        );
        assert_eq!(rects(&model), before);
    }

    #[test]
    fn grow_a_nested_pane_downward_and_only_its_own_split_moves() {
        let mut model = WindowModel::new();
        let left = model.focused_pane();
        let bottom_left = model.split(left, Direction::Down, AREA).expect("fits");
        model.focus(left);

        model
            .resize_in_direction(Direction::Down, STEP, AREA)
            .expect("there is room");

        let layout = model.layout(AREA);
        let rect_of = |pane| layout.panes.iter().find(|p| p.pane == pane).unwrap().rect;
        assert_eq!(rect_of(left), Rect::new(0.0, 0.0, 500.0, 339.5));
        assert_eq!(rect_of(bottom_left), Rect::new(0.0, 340.5, 500.0, 259.5));
        assert_eq!(layout.panes[2].rect, Rect::new(501.0, 0.0, 500.0, 600.0));
    }

    #[test]
    fn grow_a_nested_pane_to_the_right_and_the_enclosing_divider_moves() {
        let mut model = WindowModel::new();
        let left = model.focused_pane();
        model.split(left, Direction::Down, AREA).expect("fits");

        model
            .resize_in_direction(Direction::Right, STEP, AREA)
            .expect("there is room");

        let layout = model.layout(AREA);
        assert_eq!(layout.panes[0].rect.width, 540.0);
        assert_eq!(layout.panes[1].rect.width, 540.0);
        assert_eq!(layout.panes[2].rect, Rect::new(541.0, 0.0, 460.0, 600.0));
    }
}
