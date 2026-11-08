use std::time::Duration;

use lodestar_core::turn::{Phase, TurnProgress};

fn progress(phase: Phase, output_tokens: u64) -> TurnProgress {
    TurnProgress {
        started: Duration::from_secs(10),
        phase,
        output_tokens,
    }
}

#[test]
fn the_line_reads_elapsed_seconds_then_tokens_then_the_phase() {
    let line = progress(Phase::Thinking, 1500).line(Duration::from_secs(65));

    assert_eq!(line, "55s \u{b7} 1.5k tokens \u{b7} Thinking");
}

#[test]
fn past_a_minute_the_time_reads_in_minutes_and_two_digit_seconds() {
    let line = progress(Phase::Writing, 850).line(Duration::from_secs(10 + 65));

    assert_eq!(line, "1m 05s \u{b7} 850 tokens \u{b7} Writing");
}

#[test]
fn before_any_output_the_line_leaves_tokens_out() {
    let line = progress(Phase::Working, 0).line(Duration::from_secs(13));

    assert_eq!(line, "3s \u{b7} Working");
}

#[test]
fn round_thousands_drop_the_decimal_and_large_counts_keep_one() {
    assert_eq!(
        progress(Phase::Writing, 2000).line(Duration::from_secs(10)),
        "0s \u{b7} 2k tokens \u{b7} Writing"
    );
    assert_eq!(
        progress(Phase::Writing, 12_345).line(Duration::from_secs(10)),
        "0s \u{b7} 12.3k tokens \u{b7} Writing"
    );
}

#[test]
fn a_tool_phase_names_what_the_tool_does_or_the_tool_itself() {
    let label = |name: &str| Phase::Tool(name.into()).label();

    assert_eq!(label("Bash"), "Running Bash");
    assert_eq!(label("Read"), "Reading");
    assert_eq!(label("Edit"), "Editing");
    assert_eq!(label("Write"), "Writing");
    assert_eq!(label("Grep"), "Searching");
    assert_eq!(label("WebSearch"), "Using WebSearch");
}
