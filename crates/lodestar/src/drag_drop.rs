//! Drag and drop: a session from the sidebar onto a pane, or a pane by its header onto another.
//!
//! GPUI owns the active drag and paints the preview at the cursor; the payloads here carry
//! only ids. Which zone of a pane the pointer is in comes from [`DropZone::at`], so this
//! module only measures the pointer against the pane's bounds and paints the result.

use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::*;
use lodestar_core::geometry::{self, Size};
use lodestar_core::session::SessionId;
use lodestar_core::window::{Direction, DropZone, PaneId};

use crate::main_window::{ACCENT, MainWindow};

/// How see-through the drop-zone overlay is.
const OVERLAY_OPACITY: f32 = 0.3;

/// A session being dragged out of the sidebar.
#[derive(Debug, Clone)]
pub struct DraggedSession {
    pub session: SessionId,
    pub title: SharedString,
}

/// A pane being dragged by its header.
#[derive(Debug, Clone)]
pub struct DraggedPane {
    pub pane: PaneId,
    pub title: SharedString,
}

/// The pane a drag hovers and the zone of it the pointer is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropTarget {
    pub pane: PaneId,
    pub zone: DropZone,
}

/// The chip GPUI paints at the cursor during a drag.
pub struct DragPreview {
    title: SharedString,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(ACCENT))
            .bg(cx.theme().popover)
            .text_color(cx.theme().foreground)
            .text_sm()
            .shadow_md()
            .child(self.title.clone())
    }
}

fn preview(title: &SharedString, cx: &mut App) -> Entity<DragPreview> {
    let title = title.clone();
    cx.new(|_| DragPreview { title })
}

/// Make `element` start a drag of `session` when pulled.
pub fn drag_session<E: StatefulInteractiveElement + Styled>(
    element: E,
    session: SessionId,
    title: SharedString,
) -> E {
    element
        .cursor_grab()
        .on_drag(DraggedSession { session, title }, |dragged, _, _, cx| {
            preview(&dragged.title, cx)
        })
}

/// Make `element` (a pane's header) start a drag of `pane` when pulled.
pub fn drag_pane<E: StatefulInteractiveElement + Styled>(
    element: E,
    pane: PaneId,
    title: SharedString,
) -> E {
    element
        .cursor_grab()
        .on_drag(DraggedPane { pane, title }, |dragged, _, _, cx| {
            preview(&dragged.title, cx)
        })
}

impl MainWindow {
    /// Make a pane's element a drop target. While a drag crosses it, the zone under the pointer
    /// is remembered for [`Self::render_drop_overlay`]; a dropped session goes to the window
    /// model with that zone, a dropped pane swaps with this one.
    pub(crate) fn drop_target(&self, element: Div, pane: PaneId, cx: &mut Context<Self>) -> Div {
        element
            .on_drag_move::<DraggedSession>(cx.listener(
                move |this, event: &DragMoveEvent<DraggedSession>, _, cx| {
                    let zone = DropZone::at(
                        offset_in(event.bounds, event.event.position),
                        size_of(event.bounds),
                    );
                    this.track_drop(pane, event.bounds, event.event.position, zone, cx);
                },
            ))
            .on_drag_move::<DraggedPane>(cx.listener(
                move |this, event: &DragMoveEvent<DraggedPane>, _, cx| {
                    let bounds = if event.drag(cx).pane == pane {
                        Bounds::default()
                    } else {
                        event.bounds
                    };
                    this.track_drop(pane, bounds, event.event.position, DropZone::Middle, cx);
                },
            ))
            .on_drop(
                cx.listener(move |this, dragged: &DraggedSession, window, cx| {
                    let zone = this
                        .drop_target
                        .filter(|target| target.pane == pane)
                        .map_or(DropZone::Middle, |target| target.zone);
                    this.drop_target = None;
                    let area = this.pane_area(window);
                    if let Err(refused) =
                        this.window_model
                            .drop_session(dragged.session, pane, zone, area)
                    {
                        this.flash(pane, refused, window, cx);
                    }
                    cx.notify();
                }),
            )
            .on_drop(cx.listener(move |this, dragged: &DraggedPane, _, cx| {
                this.drop_target = None;
                this.window_model.swap(dragged.pane, pane);
                cx.notify();
            }))
    }

    /// Remember that a drag at `position` hovers `zone` of `pane`, whose element fills
    /// `bounds`; forget `pane` as the target when the pointer has left it.
    fn track_drop(
        &mut self,
        pane: PaneId,
        bounds: Bounds<Pixels>,
        position: Point<Pixels>,
        zone: DropZone,
        cx: &mut Context<Self>,
    ) {
        let next = bounds
            .contains(&position)
            .then_some(DropTarget { pane, zone });
        let current = self.drop_target.filter(|target| target.pane == pane);
        if next != current {
            self.drop_target = next;
            cx.notify();
        }
    }

    /// Forget the drop target once no drag is active, so no overlay lingers after a drag ends
    /// outside every pane.
    pub(crate) fn clear_stale_drop_target(&mut self, cx: &App) {
        if !cx.has_active_drag() {
            self.drop_target = None;
        }
    }

    /// A translucent accent overlay on the part of `pane` a drop would fill: the whole pane
    /// for the middle, the half a dragged session would take for an edge. `None` when no drag
    /// hovers `pane`.
    pub(crate) fn render_drop_overlay(&self, pane: PaneId) -> Option<Div> {
        let target = self.drop_target.filter(|target| target.pane == pane)?;
        let overlay = div()
            .absolute()
            .border_2()
            .border_color(rgb(ACCENT))
            .bg(rgb(ACCENT).opacity(OVERLAY_OPACITY));
        let half = relative(0.5);
        Some(match target.zone {
            DropZone::Middle => overlay.top_0().bottom_0().left_0().right_0(),
            DropZone::Edge(Direction::Left) => overlay.top_0().bottom_0().left_0().w(half),
            DropZone::Edge(Direction::Right) => overlay.top_0().bottom_0().right_0().w(half),
            DropZone::Edge(Direction::Up) => overlay.left_0().right_0().top_0().h(half),
            DropZone::Edge(Direction::Down) => overlay.left_0().right_0().bottom_0().h(half),
        })
    }
}

/// `position` relative to the top-left of `bounds`, in logical pixels.
fn offset_in(bounds: Bounds<Pixels>, position: Point<Pixels>) -> geometry::Point {
    geometry::Point::new(
        f32::from(position.x - bounds.origin.x),
        f32::from(position.y - bounds.origin.y),
    )
}

fn size_of(bounds: Bounds<Pixels>) -> Size {
    Size::new(f32::from(bounds.size.width), f32::from(bounds.size.height))
}
