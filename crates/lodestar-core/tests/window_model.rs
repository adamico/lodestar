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
