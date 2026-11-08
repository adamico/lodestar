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

mod focus_and_swap {
    use lodestar_core::geometry::Size;
    use lodestar_core::window::{Direction, WindowModel};

    const AREA: Size = Size::new(1601.0, 600.0);

    #[test]
    fn focus_right_from_the_left_launch_pane_lands_on_the_right_pane() {
        let mut model = WindowModel::new();
        let right = model.layout(AREA).panes[1].pane;

        let moved = model.focus_in_direction(Direction::Right, AREA);

        assert!(moved);
        assert_eq!(model.focused_pane(), right);
    }
}

mod focus_edges {
    use lodestar_core::geometry::Size;
    use lodestar_core::window::{Direction, WindowModel};

    const AREA: Size = Size::new(1601.0, 600.0);

    #[test]
    fn focus_left_at_the_window_edge_does_nothing() {
        let mut model = WindowModel::new();
        let left = model.focused_pane();

        let moved = model.focus_in_direction(Direction::Left, AREA);

        assert!(!moved);
        assert_eq!(model.focused_pane(), left);
    }

    #[test]
    fn focus_up_and_down_with_nothing_above_or_below_does_nothing() {
        let mut model = WindowModel::new();
        let left = model.focused_pane();

        assert!(!model.focus_in_direction(Direction::Up, AREA));
        assert!(!model.focus_in_direction(Direction::Down, AREA));
        assert_eq!(model.focused_pane(), left);
    }
}

/// Left pane full height; right column split top/bottom, the bottom one split again.
/// Panes are named by position: `a` left, `b1` top right, `b2a` and `b2b` below it.
mod focus_across_uneven_layouts {
    use lodestar_core::geometry::Size;
    use lodestar_core::window::{Direction, PaneId, WindowModel};

    const AREA: Size = Size::new(1601.0, 1000.0);

    struct Panes {
        a: PaneId,
        b1: PaneId,
        b2a: PaneId,
        b2b: PaneId,
    }

    fn uneven(model: &mut WindowModel) -> Panes {
        let layout = model.layout(AREA);
        let (a, b1) = (layout.panes[0].pane, layout.panes[1].pane);
        let b2a = model.split(b1, Direction::Down, AREA).expect("fits");
        let b2b = model.split(b2a, Direction::Down, AREA).expect("fits");
        Panes { a, b1, b2a, b2b }
    }

    #[test]
    fn focus_left_from_a_small_lower_right_pane_lands_on_the_tall_left_pane() {
        let mut model = WindowModel::new();
        let panes = uneven(&mut model);
        model.focus(panes.b2b);

        assert!(model.focus_in_direction(Direction::Left, AREA));
        assert_eq!(model.focused_pane(), panes.a);
    }

    #[test]
    fn focus_up_from_the_lowest_pane_lands_on_the_pane_directly_above_not_the_top_one() {
        let mut model = WindowModel::new();
        let panes = uneven(&mut model);
        model.focus(panes.b2b);

        assert!(model.focus_in_direction(Direction::Up, AREA));
        assert_eq!(model.focused_pane(), panes.b2a);
        assert!(model.focus_in_direction(Direction::Up, AREA));
        assert_eq!(model.focused_pane(), panes.b1);
        assert!(!model.focus_in_direction(Direction::Up, AREA));
    }

    #[test]
    fn focus_right_from_the_tall_left_pane_lands_on_the_right_pane_level_with_its_centre() {
        // The left pane's centre is level with the divider between the top-right pane and the
        // lower two; the upper of the two equally close panes wins.
        let mut model = WindowModel::new();
        let panes = uneven(&mut model);
        model.focus(panes.a);

        assert!(model.focus_in_direction(Direction::Right, AREA));
        assert_eq!(model.focused_pane(), panes.b1);
    }

    #[test]
    fn focus_right_from_a_lower_left_pane_lands_on_the_lower_right_pane_level_with_it() {
        let mut model = WindowModel::new();
        let panes = uneven(&mut model);
        // Split the left pane too, so its lower half is level with the two small right panes.
        let a2 = model.split(panes.a, Direction::Down, AREA).expect("fits");
        model.focus(a2);

        assert!(model.focus_in_direction(Direction::Right, AREA));
        assert_eq!(model.focused_pane(), panes.b2a);
        model.focus(panes.a);
        assert!(model.focus_in_direction(Direction::Right, AREA));
        assert_eq!(model.focused_pane(), panes.b1);
        assert!(model.focus_in_direction(Direction::Left, AREA));
        assert_eq!(model.focused_pane(), panes.a);
        model.focus(panes.b2b);
        assert!(model.focus_in_direction(Direction::Left, AREA));
        assert_eq!(model.focused_pane(), a2);
    }
}

/// Left pane full height; right column split top/bottom. Sessions 1, 2, 3 sit in them in that
/// order, so rects can be checked by session after a swap.
mod swap {
    use lodestar_core::geometry::{Rect, Size};
    use lodestar_core::session::SessionId;
    use lodestar_core::window::{Direction, PaneId, WindowModel};

    const AREA: Size = Size::new(1601.0, 600.0);
    const LEFT: Rect = Rect::new(0.0, 0.0, 800.0, 600.0);
    const TOP_RIGHT: Rect = Rect::new(801.0, 0.0, 800.0, 299.5);
    const BOTTOM_RIGHT: Rect = Rect::new(801.0, 300.5, 800.0, 299.5);

    fn uneven(model: &mut WindowModel) -> (PaneId, PaneId, PaneId) {
        let layout = model.layout(AREA);
        let (left, top_right) = (layout.panes[0].pane, layout.panes[1].pane);
        let bottom_right = model.split(top_right, Direction::Down, AREA).expect("fits");
        model.open_session(SessionId(1), left);
        model.open_session(SessionId(2), top_right);
        model.open_session(SessionId(3), bottom_right);
        (left, top_right, bottom_right)
    }

    fn rect_of_session(model: &WindowModel, session: SessionId) -> Rect {
        let pane = model.pane_showing(session).expect("shown");
        model
            .layout(AREA)
            .panes
            .iter()
            .find(|p| p.pane == pane)
            .expect("laid out")
            .rect
    }

    #[test]
    fn swap_two_panes_and_their_sessions_trade_places_on_screen() {
        let mut model = WindowModel::new();
        let (left, _, bottom_right) = uneven(&mut model);

        model.swap(left, bottom_right);

        assert_eq!(rect_of_session(&model, SessionId(1)), BOTTOM_RIGHT);
        assert_eq!(rect_of_session(&model, SessionId(3)), LEFT);
        assert_eq!(rect_of_session(&model, SessionId(2)), TOP_RIGHT);
        assert_eq!(model.session_in(left), Some(SessionId(1)));
    }

    #[test]
    fn swap_follows_the_focused_pane_to_its_new_place() {
        let mut model = WindowModel::new();
        let (left, top_right, _) = uneven(&mut model);
        model.focus(top_right);

        model.swap(left, top_right);

        assert_eq!(model.focused_pane(), top_right);
        assert_eq!(rect_of_session(&model, SessionId(2)), LEFT);
    }

    #[test]
    fn swap_right_from_the_left_pane_moves_it_and_focus_to_the_level_right_pane() {
        let mut model = WindowModel::new();
        let (left, top_right, _) = uneven(&mut model);
        model.focus(left);

        let swapped = model.swap_in_direction(Direction::Right, AREA);

        assert!(swapped);
        assert_eq!(model.focused_pane(), left);
        assert_eq!(rect_of_session(&model, SessionId(1)), TOP_RIGHT);
        assert_eq!(rect_of_session(&model, SessionId(2)), LEFT);
        assert_eq!(rect_of_session(&model, SessionId(3)), BOTTOM_RIGHT);
        assert_eq!(model.session_in(top_right), Some(SessionId(2)));
    }

    #[test]
    fn swap_down_from_the_top_right_pane_trades_places_with_the_one_below() {
        let mut model = WindowModel::new();
        let (_, top_right, _) = uneven(&mut model);
        model.focus(top_right);

        assert!(model.swap_in_direction(Direction::Down, AREA));

        assert_eq!(rect_of_session(&model, SessionId(2)), BOTTOM_RIGHT);
        assert_eq!(rect_of_session(&model, SessionId(3)), TOP_RIGHT);
        assert_eq!(rect_of_session(&model, SessionId(1)), LEFT);
    }

    #[test]
    fn swap_towards_the_window_edge_does_nothing() {
        let mut model = WindowModel::new();
        let (left, _, _) = uneven(&mut model);
        model.focus(left);

        assert!(!model.swap_in_direction(Direction::Left, AREA));
        assert!(!model.swap_in_direction(Direction::Up, AREA));

        assert_eq!(model.focused_pane(), left);
        assert_eq!(rect_of_session(&model, SessionId(1)), LEFT);
        assert_eq!(rect_of_session(&model, SessionId(2)), TOP_RIGHT);
    }

    #[test]
    fn swap_an_empty_pane_with_a_session_pane_and_the_empty_one_moves_too() {
        let mut model = WindowModel::new();
        let layout = model.layout(AREA);
        let (left, right) = (layout.panes[0].pane, layout.panes[1].pane);
        model.open_session(SessionId(1), left);
        model.focus(right);

        assert!(model.swap_in_direction(Direction::Left, AREA));

        assert_eq!(model.focused_pane(), right);
        assert_eq!(
            rect_of_session(&model, SessionId(1)),
            Rect::new(801.0, 0.0, 800.0, 600.0)
        );
        let layout = model.layout(AREA);
        assert_eq!(layout.panes[0].pane, right);
        assert_eq!(model.session_in(right), None);
        assert_eq!(model.sessions_shown(), vec![SessionId(1)]);
    }
}

mod drop_session {
    use super::*;
    use lodestar_core::window::DropZone;

    /// The launch panes are 800×600 each.
    const AREA: Size = Size::new(1601.0, 600.0);

    #[test]
    fn drop_on_the_middle_of_an_empty_pane_shows_the_session_there() {
        let mut model = WindowModel::new();
        let right = model.layout(AREA).panes[1].pane;

        model
            .drop_session(SessionId(1), right, DropZone::Middle, AREA)
            .expect("a middle drop never changes sizes");

        assert_eq!(model.session_in(right), Some(SessionId(1)));
        assert_eq!(rects(&model, AREA).len(), 2);
    }
}
