//! A Transcript's items and the small rules that shape how they are shown.

use lodestar_core::session::{ToolCall, Transcript, TranscriptItem};

fn tool_call(summary: &str) -> ToolCall {
    ToolCall {
        name: "Bash".into(),
        summary: summary.into(),
        input: "cargo test".into(),
        output: "4 passed".into(),
    }
}

#[test]
fn pushed_items_come_back_in_the_order_they_happened() {
    let mut transcript = Transcript::default();
    transcript.push(TranscriptItem::User("Run the tests.".into()));
    transcript.push(TranscriptItem::ToolCall(tool_call("cargo test")));
    transcript.push(TranscriptItem::Assistant("All **green**.".into()));

    assert_eq!(
        transcript.items(),
        &[
            TranscriptItem::User("Run the tests.".into()),
            TranscriptItem::ToolCall(tool_call("cargo test")),
            TranscriptItem::Assistant("All **green**.".into()),
        ]
    );
}

#[test]
fn a_tool_call_headline_names_the_tool_and_its_summary() {
    assert_eq!(tool_call("cargo test").headline(), "Bash  cargo test");
}

#[test]
fn a_tool_call_headline_keeps_only_the_first_line_of_the_summary() {
    assert_eq!(
        tool_call("cargo test\ncargo clippy").headline(),
        "Bash  cargo test"
    );
}

#[test]
fn a_long_tool_call_headline_is_cut_with_an_ellipsis() {
    let summary = "x".repeat(200);
    let headline = tool_call(&summary).headline();
    assert_eq!(headline.chars().count(), 80);
    assert!(headline.ends_with('…'));
    assert!(headline.starts_with("Bash  xxx"));
}

#[test]
fn a_tool_call_without_a_summary_falls_back_to_its_input() {
    assert_eq!(tool_call("").headline(), "Bash  cargo test");
}
