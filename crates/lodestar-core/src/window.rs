//! The window model: the pane tree, what each pane shows, and which pane has focus.

use std::collections::HashMap;

use crate::geometry::{Rect, Size};
use crate::session::SessionId;

/// Width of the gap between two panes, in logical pixels.
pub const DIVIDER_THICKNESS: f32 = 1.0;

/// No pane may be smaller than this, in logical pixels. Commands that would make one are refused.
pub const MIN_PANE_SIZE: Size = Size::new(320.0, 200.0);

/// A side of a pane, as seen on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Where on a pane a dragged session is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DropZone {
    /// The session takes the pane over.
    Middle,
    /// The pane splits on that side and the session takes the new half.
    Edge(Direction),
}

/// Why a command did nothing. The view reacts to it, for example with a flash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refused {
    /// A pane would have ended up smaller than [`MIN_PANE_SIZE`].
    BelowMinimumSize,
    /// The pane no longer exists.
    NoSuchPane,
}

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
    /// If some pane already shows the session, that pane is focused instead, so a session
    /// is never in two panes. Otherwise the session fills `pane`, replacing whatever it showed;
    /// the replaced session is then not shown anywhere. A pane that no longer exists is ignored.
    pub fn open_session(&mut self, session: SessionId, pane: PaneId) {
        if let Some(showing) = self.pane_showing(session) {
            self.focused = showing;
            return;
        }
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

/// Splitting and closing panes.
impl WindowModel {
    /// Split `pane` in the direction its shape suggests: a pane wider than tall splits side by
    /// side, otherwise top and bottom. `area` is the pane-area size the window is laid out in.
    ///
    /// The new pane is empty and focused. Returns its id, or the refusal if either half would be
    /// smaller than [`MIN_PANE_SIZE`].
    pub fn split_auto(&mut self, pane: PaneId, area: Size) -> Result<PaneId, Refused> {
        let rect = self.rect_of(pane, area).ok_or(Refused::NoSuchPane)?;
        let direction = if rect.width > rect.height {
            Direction::Right
        } else {
            Direction::Down
        };
        self.split(pane, direction, area)
    }

    /// Split `pane`, placing the new pane on its `direction` side. See [`Self::split_auto`].
    pub fn split(
        &mut self,
        pane: PaneId,
        direction: Direction,
        area: Size,
    ) -> Result<PaneId, Refused> {
        let rect = self.rect_of(pane, area).ok_or(Refused::NoSuchPane)?;
        let axis = match direction {
            Direction::Left | Direction::Right => Axis::Horizontal,
            Direction::Up | Direction::Down => Axis::Vertical,
        };
        let (first, _, second) = split_rect(rect, axis, 0.5);
        if !fits_minimum(first) || !fits_minimum(second) {
            return Err(Refused::BelowMinimumSize);
        }

        let new_pane = self.new_pane();
        let new_first = matches!(direction, Direction::Left | Direction::Up);
        self.root.split_leaf(pane, new_pane, axis, new_first);
        self.focused = new_pane;
        Ok(new_pane)
    }

    /// Close `pane`: its sibling takes its space. The session it showed is only hidden, never
    /// ended. Closing the last pane leaves one new empty pane. A pane that no longer exists is
    /// ignored.
    pub fn close(&mut self, pane: PaneId) {
        if self.panes.remove(&pane).is_none() {
            return;
        }
        if self.panes.is_empty() {
            let fresh = self.new_pane();
            self.root = Node::Pane(fresh);
            self.focused = fresh;
            return;
        }
        self.root.remove_leaf(pane);
        if self.focused == pane {
            self.focused = self.root.first_pane();
        }
    }

    fn rect_of(&self, pane: PaneId, area: Size) -> Option<Rect> {
        self.layout(area)
            .panes
            .into_iter()
            .find(|p| p.pane == pane)
            .map(|p| p.rect)
    }
}

/// Moving focus between neighbouring panes.
impl WindowModel {
    /// The pane on the `direction` side of `pane`, judged from the layout in a pane area of
    /// `area`: probe a point just past `pane`'s edge, level with its centre, and take the pane
    /// there. When the probe lands on a divider, the nearer of the panes on either side of it
    /// wins, the upper or left one on a tie. `None` at the window edge or when `pane` does not
    /// exist.
    pub fn neighbour_in_direction(
        &self,
        pane: PaneId,
        direction: Direction,
        area: Size,
    ) -> Option<PaneId> {
        let layout = self.layout(area);
        let rect = layout.panes.iter().find(|p| p.pane == pane)?.rect;
        let probe = DIVIDER_THICKNESS + 1.0;
        let (along, level) = match direction {
            Direction::Left => (rect.x - probe, rect.y + rect.height / 2.0),
            Direction::Right => (rect.x + rect.width + probe, rect.y + rect.height / 2.0),
            Direction::Up => (rect.y - probe, rect.x + rect.width / 2.0),
            Direction::Down => (rect.y + rect.height + probe, rect.x + rect.width / 2.0),
        };
        let sideways = matches!(direction, Direction::Left | Direction::Right);
        layout
            .panes
            .iter()
            .filter(|p| p.pane != pane)
            .filter_map(|p| {
                let (span, cross) = if sideways {
                    ((p.rect.x, p.rect.width), (p.rect.y, p.rect.height))
                } else {
                    ((p.rect.y, p.rect.height), (p.rect.x, p.rect.width))
                };
                within(span, along).then(|| (distance(cross, level), p.pane))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, pane)| pane)
    }

    /// Move focus to the pane on the `direction` side of the focused pane. Returns whether focus
    /// moved; it does not at the window edge.
    pub fn focus_in_direction(&mut self, direction: Direction, area: Size) -> bool {
        match self.neighbour_in_direction(self.focused, direction, area) {
            Some(neighbour) => {
                self.focused = neighbour;
                true
            }
            None => false,
        }
    }
}

/// Swapping panes.
impl WindowModel {
    /// Exchange the places of panes `a` and `b` in the layout. Each keeps its id and the session
    /// it shows, so the sessions trade places on screen, and focus follows the focused pane.
    /// Does nothing if either pane no longer exists.
    pub fn swap(&mut self, a: PaneId, b: PaneId) {
        if a == b || !self.panes.contains_key(&a) || !self.panes.contains_key(&b) {
            return;
        }
        self.root.swap_leaves(a, b);
    }

    /// Swap the focused pane with its neighbour on the `direction` side (see
    /// [`Self::neighbour_in_direction`]); focus stays with the moved pane. Returns whether a swap
    /// happened; none does at the window edge.
    pub fn swap_in_direction(&mut self, direction: Direction, area: Size) -> bool {
        match self.neighbour_in_direction(self.focused, direction, area) {
            Some(neighbour) => {
                self.swap(self.focused, neighbour);
                true
            }
            None => false,
        }
    }
}

/// Dropping a dragged session onto a pane.
impl WindowModel {
    /// Show `session` where a drag and drop puts it: on [`DropZone::Middle`] it takes `pane`
    /// over; on an edge, `pane` splits on that side and the session takes the new half, which
    /// is focused. Whatever the session replaces is then not shown anywhere, and if the session
    /// was shown in another pane it moves, leaving that pane empty.
    ///
    /// An edge drop is refused, changing nothing, when either half would be smaller than
    /// [`MIN_PANE_SIZE`].
    pub fn drop_session(
        &mut self,
        session: SessionId,
        pane: PaneId,
        zone: DropZone,
        area: Size,
    ) -> Result<(), Refused> {
        let target = match zone {
            DropZone::Middle => {
                if !self.panes.contains_key(&pane) {
                    return Err(Refused::NoSuchPane);
                }
                pane
            }
            DropZone::Edge(direction) => self.split(pane, direction, area)?,
        };
        if let Some(old) = self.pane_showing(session) {
            self.panes.insert(old, None);
        }
        self.panes.insert(target, Some(session));
        Ok(())
    }
}

impl Node {
    /// Relabel the leaf `a` as `b` and the leaf `b` as `a`.
    fn swap_leaves(&mut self, a: PaneId, b: PaneId) {
        match self {
            Node::Pane(id) if *id == a => *id = b,
            Node::Pane(id) if *id == b => *id = a,
            Node::Pane(_) => {}
            Node::Split { first, second, .. } => {
                first.swap_leaves(a, b);
                second.swap_leaves(a, b);
            }
        }
    }
}

/// Whether `at` lies inside the interval `(start, length)`.
fn within((start, length): (f32, f32), at: f32) -> bool {
    at >= start && at < start + length
}

/// How far `at` is from the interval `(start, length)`; zero inside it.
fn distance((start, length): (f32, f32), at: f32) -> f32 {
    (start - at).max(at - (start + length)).max(0.0)
}

fn fits_minimum(rect: Rect) -> bool {
    rect.width >= MIN_PANE_SIZE.width && rect.height >= MIN_PANE_SIZE.height
}

impl Node {
    /// Replace the leaf `pane` with a split of `pane` and `new_pane`.
    fn split_leaf(&mut self, pane: PaneId, new_pane: PaneId, axis: Axis, new_first: bool) {
        match self {
            Node::Pane(id) if *id == pane => {
                let (first, second) = if new_first {
                    (new_pane, pane)
                } else {
                    (pane, new_pane)
                };
                *self = Node::Split {
                    axis,
                    ratio: 0.5,
                    first: Box::new(Node::Pane(first)),
                    second: Box::new(Node::Pane(second)),
                };
            }
            Node::Pane(_) => {}
            Node::Split { first, second, .. } => {
                first.split_leaf(pane, new_pane, axis, new_first);
                second.split_leaf(pane, new_pane, axis, new_first);
            }
        }
    }
}

impl Node {
    /// Remove the leaf `pane`, its sibling taking the parent's place. Does nothing when `pane` is
    /// the root or absent.
    fn remove_leaf(&mut self, pane: PaneId) {
        let Node::Split { first, second, .. } = self else {
            return;
        };
        if matches!(**first, Node::Pane(id) if id == pane) {
            *self = std::mem::replace(second, Node::Pane(pane));
        } else if matches!(**second, Node::Pane(id) if id == pane) {
            *self = std::mem::replace(first, Node::Pane(pane));
        } else {
            first.remove_leaf(pane);
            second.remove_leaf(pane);
        }
    }

    /// The leftmost, topmost pane under this node.
    fn first_pane(&self) -> PaneId {
        match self {
            Node::Pane(pane) => *pane,
            Node::Split { first, .. } => first.first_pane(),
        }
    }
}

impl Default for WindowModel {
    fn default() -> Self {
        Self::new()
    }
}
