//! The sidebar's rows and keyboard selection, in plain Rust.
//!
//! The sidebar lists every session grouped by Project. Keyboard focus moves over the rows as
//! one flat list in that order; this module owns the order and the selection rules, so the
//! view only renders them.

use crate::session::{SessionId, SessionStore};

/// The narrowest the sidebar can be dragged, in logical pixels.
pub const SIDEBAR_MIN_WIDTH: f32 = 160.0;
/// The widest the sidebar can be dragged, in logical pixels.
pub const SIDEBAR_MAX_WIDTH: f32 = 480.0;

/// Keep a dragged sidebar width within its limits.
pub fn clamp_width(width: f32) -> f32 {
    width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
}

/// The sidebar's session rows as one flat list, in the order the sidebar shows them:
/// Projects in store order, and within each Project the newest session first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarList {
    sessions: Vec<SessionId>,
}

impl SidebarList {
    pub fn new(store: &SessionStore) -> Self {
        let sessions = store
            .projects()
            .iter()
            .flat_map(|project| store.sessions_in(project.id))
            .map(|session| session.id)
            .collect();
        Self { sessions }
    }

    /// Every row's session, top to bottom.
    pub fn sessions(&self) -> &[SessionId] {
        &self.sessions
    }

    /// The row to select when the sidebar takes focus: the row of the focused pane's session,
    /// or the first row when that pane is empty.
    pub fn initial_selection(&self, focused_session: Option<SessionId>) -> usize {
        focused_session
            .and_then(|session| self.sessions.iter().position(|s| *s == session))
            .unwrap_or(0)
    }

    /// Move the selection by `delta` rows, stopping at the first and last row.
    pub fn step(&self, selected: usize, delta: isize) -> usize {
        let last = self.sessions.len().saturating_sub(1);
        selected.saturating_add_signed(delta).min(last)
    }

    /// The session on row `index`, if there is one.
    pub fn session_at(&self, index: usize) -> Option<SessionId> {
        self.sessions.get(index).copied()
    }
}
