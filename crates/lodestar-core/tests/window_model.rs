use lodestar_core::geometry::{Rect, Size};
use lodestar_core::window::{DIVIDER_THICKNESS, PaneLayout, WindowModel};

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
