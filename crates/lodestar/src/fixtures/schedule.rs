//! Session updates that arrive over time.
//!
//! A source that plays updates back on a clock (recorded sessions, replays) needs
//! the same small rule: release each update once, when its time has come, in time order.
//! That rule lives here, free of timers, so it is testable with plain `Duration`s.

use std::time::Duration;

use lodestar_core::session::SessionUpdate;

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
    #[cfg(test)]
    pub fn is_finished(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lodestar_core::session::{SessionId, SessionStatus, SessionUpdate, TranscriptItem};

    use super::UpdateSchedule;

    fn item(text: &str) -> SessionUpdate {
        SessionUpdate::TranscriptItem {
            session: SessionId(0),
            item: TranscriptItem::Assistant(text.into()),
        }
    }

    fn done() -> SessionUpdate {
        SessionUpdate::Status {
            session: SessionId(0),
            status: SessionStatus::Done,
        }
    }

    fn schedule() -> UpdateSchedule {
        UpdateSchedule::new(vec![
            (Duration::from_millis(500), item("one")),
            (Duration::from_millis(1500), item("two")),
            (Duration::from_millis(3000), done()),
        ])
    }

    #[test]
    fn nothing_is_due_before_its_time() {
        let mut schedule = schedule();

        assert!(schedule.due(Duration::from_millis(499)).is_empty());
        assert!(!schedule.is_finished());
    }

    #[test]
    fn every_update_whose_time_has_come_is_due_in_time_order_and_only_once() {
        let mut schedule = schedule();

        assert_eq!(
            schedule.due(Duration::from_millis(1600)),
            vec![item("one"), item("two")]
        );
        assert!(schedule.due(Duration::from_millis(1600)).is_empty());
        assert!(schedule.due(Duration::from_millis(2999)).is_empty());
    }

    #[test]
    fn the_final_status_comes_after_the_last_item_and_finishes_the_schedule() {
        let mut schedule = schedule();

        assert_eq!(
            schedule.due(Duration::from_secs(10)),
            vec![item("one"), item("two"), done()]
        );
        assert!(schedule.is_finished());
        assert!(schedule.due(Duration::from_secs(20)).is_empty());
    }

    #[test]
    fn updates_given_out_of_order_are_still_released_in_time_order() {
        let mut schedule = UpdateSchedule::new(vec![
            (Duration::from_millis(2000), item("late")),
            (Duration::from_millis(1000), item("early")),
        ]);

        assert_eq!(
            schedule.due(Duration::from_secs(5)),
            vec![item("early"), item("late")]
        );
    }
}
