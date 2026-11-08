//! The window model: the pane tree, what each pane shows, and which pane has focus.

use std::collections::HashMap;

use crate::geometry::{Rect, Size};
use crate::session::SessionId;

/// Width of the gap between two panes, in logical pixels.
pub const DIVIDER_THICKNESS: f32 = 1.0;

/// Identifies a pane for as long as it exists. Ids are never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(u64);

/// How a split lays out its two children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Children side by side, first on the left.
    Horizontal,
    /// Children stacked, first on top.
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneLayout {
    pub pane: PaneId,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DividerLayout {
    /// The axis of the split this divider belongs to. A horizontal split has a vertical divider.
    pub axis: Axis,
    pub rect: Rect,
}

/// Where every pane and divider sits, for a given pane-area size.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    /// In tree order: left before right, top before bottom.
    pub panes: Vec<PaneLayout>,
    pub dividers: Vec<DividerLayout>,
}

#[derive(Debug, Clone)]
enum Node {
    Pane(PaneId),
    Split {
        axis: Axis,
        /// The share of space, minus the divider, that goes to `first`.
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

impl Node {
    fn lay_out(&self, rect: Rect, layout: &mut Layout) {
        match self {
            Node::Pane(pane) => layout.panes.push(PaneLayout { pane: *pane, rect }),
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (first_rect, divider, second_rect) = split_rect(rect, *axis, *ratio);
                first.lay_out(first_rect, layout);
                layout.dividers.push(DividerLayout {
                    axis: *axis,
                    rect: divider,
                });
                second.lay_out(second_rect, layout);
            }
        }
    }
}

fn split_rect(rect: Rect, axis: Axis, ratio: f32) -> (Rect, Rect, Rect) {
    match axis {
        Axis::Horizontal => {
            let first = (rect.width - DIVIDER_THICKNESS) * ratio;
            let divider_x = rect.x + first;
            let second_x = divider_x + DIVIDER_THICKNESS;
            (
                Rect::new(rect.x, rect.y, first, rect.height),
                Rect::new(divider_x, rect.y, DIVIDER_THICKNESS, rect.height),
                Rect::new(
                    second_x,
                    rect.y,
                    rect.x + rect.width - second_x,
                    rect.height,
                ),
            )
        }
        Axis::Vertical => {
            let first = (rect.height - DIVIDER_THICKNESS) * ratio;
            let divider_y = rect.y + first;
            let second_y = divider_y + DIVIDER_THICKNESS;
            (
                Rect::new(rect.x, rect.y, rect.width, first),
                Rect::new(rect.x, divider_y, rect.width, DIVIDER_THICKNESS),
                Rect::new(
                    rect.x,
                    second_y,
                    rect.width,
                    rect.y + rect.height - second_y,
                ),
            )
        }
    }
}

#[derive(Debug, Clone)]
pub struct WindowModel {
    root: Node,
    /// Every pane that exists, and the session it shows, if any.
    panes: HashMap<PaneId, Option<SessionId>>,
    focused: PaneId,
    next_pane_id: u64,
}

impl WindowModel {
    /// The launch state: two empty panes side by side, the left one focused.
    pub fn new() -> Self {
        let mut model = Self {
            root: Node::Pane(PaneId(0)),
            panes: HashMap::new(),
            focused: PaneId(0),
            next_pane_id: 0,
        };
        let left = model.new_pane();
        let right = model.new_pane();
        model.root = Node::Split {
            axis: Axis::Horizontal,
            ratio: 0.5,
            first: Box::new(Node::Pane(left)),
            second: Box::new(Node::Pane(right)),
        };
        model.focused = left;
        model
    }

    /// Where every pane and divider sits in a pane area of `size`, with its top-left at the origin.
    pub fn layout(&self, size: Size) -> Layout {
        let mut layout = Layout::default();
        self.root
            .lay_out(Rect::new(0.0, 0.0, size.width, size.height), &mut layout);
        layout
    }

    /// The pane that keyboard commands act on.
    pub fn focused_pane(&self) -> PaneId {
        self.focused
    }

    /// Focus `pane`, as a click on it does. A pane that no longer exists is ignored.
    pub fn focus(&mut self, pane: PaneId) {
        if self.panes.contains_key(&pane) {
            self.focused = pane;
        }
    }

    /// The session `pane` shows, or `None` if it is empty or does not exist.
    pub fn session_in(&self, pane: PaneId) -> Option<SessionId> {
        self.panes.get(&pane).copied().flatten()
    }

    /// The pane that shows `session`, if any pane does.
    pub fn pane_showing(&self, session: SessionId) -> Option<PaneId> {
        self.panes
            .iter()
            .find(|(_, shown)| **shown == Some(session))
            .map(|(pane, _)| *pane)
    }

    /// Every session that some pane shows, in no particular order.
    pub fn sessions_shown(&self) -> Vec<SessionId> {
        self.panes.values().flatten().copied().collect()
    }

    /// The sessions among `all_sessions` that no pane shows, in the order given.
    pub fn not_shown(&self, all_sessions: &[SessionId]) -> Vec<SessionId> {
        all_sessions
            .iter()
            .copied()
            .filter(|session| self.pane_showing(*session).is_none())
            .collect()
    }

    /// Show `session` in `pane`, as a sidebar click does.
    ///
    /// A pane that no longer exists is ignored.
    pub fn open_session(&mut self, session: SessionId, pane: PaneId) {
        if let Some(slot) = self.panes.get_mut(&pane) {
            *slot = Some(session);
        }
    }

    fn new_pane(&mut self) -> PaneId {
        let pane = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        self.panes.insert(pane, None);
        pane
    }
}

impl Default for WindowModel {
    fn default() -> Self {
        Self::new()
    }
}
