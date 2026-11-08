//! A schedule of session updates that become due as time passes.

use std::time::Duration;

use lodestar_core::session::{SessionId, SessionStatus, SessionUpdate, TranscriptItem};
use lodestar_core::stream::UpdateSchedule;

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
