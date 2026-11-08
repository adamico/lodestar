//! Session updates that arrive over time.
//!
//! A source that plays updates back on a clock (recorded sessions, replays) needs
//! the same small rule: release each update once, when its time has come, in time order.
//! That rule lives here, free of timers, so it is testable with plain `Duration`s.

use std::time::Duration;

use crate::session::SessionUpdate;

/// Updates waiting for their moment, each due at a point in time measured from startup.
#[derive(Debug, Clone, Default)]
pub struct UpdateSchedule {
    /// Sorted by due time, the soonest last so releasing pops from the end.
    pending: Vec<(Duration, SessionUpdate)>,
}

impl UpdateSchedule {
    /// A schedule of `updates`, each paired with when it is due. Any order is fine.
    pub fn new(mut updates: Vec<(Duration, SessionUpdate)>) -> Self {
        updates.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        Self { pending: updates }
    }

    /// Release every update due at or before `now`, soonest first. Each is released once.
    pub fn due(&mut self, now: Duration) -> Vec<SessionUpdate> {
        let mut due = Vec::new();
        while self.pending.last().is_some_and(|(at, _)| *at <= now) {
            if let Some((_, update)) = self.pending.pop() {
                due.push(update);
            }
        }
        due
    }

    /// Whether every update has been released.
    pub fn is_finished(&self) -> bool {
        self.pending.is_empty()
    }
}
