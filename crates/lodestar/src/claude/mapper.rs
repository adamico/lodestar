//! From Claude Code's stream-json lines to `SessionUpdate`s, as a pure function.
//!
//! Every line is one JSON object with a `type`. Only three kinds matter to Lodestar so far:
//! `system`/`init` opens a turn, `assistant` carries what the Agent said, and `result` closes
//! the turn. Everything else (tool calls and their results, progress blurbs, rate-limit
//! notes, partial messages, lines that are not JSON) is ignored without complaint, because
//! Claude Code adds event kinds freely and a stray line must never take the session down.

use lodestar_core::session::{SessionId, SessionStatus, SessionUpdate, TranscriptItem};
use serde_json::Value;

/// One stream-json line, reduced to what Lodestar reacts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A turn began. Claude Code sends one at the start of every turn, with the same id.
    Init { claude_session_id: String },
    /// Text the Agent said, one entry per text block, as markdown.
    AssistantText(Vec<String>),
    /// The turn ended. `is_error` is for a later slice; the Agent is alive either way.
    Result { is_error: bool },
    /// A line Lodestar does not react to.
    Ignored,
}

/// Read one line of stream-json. Never fails: what cannot be read is [`Event::Ignored`].
pub fn parse_line(line: &str) -> Event {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Event::Ignored;
    };
    match value["type"].as_str() {
        Some("system") if value["subtype"] == "init" => match value["session_id"].as_str() {
            Some(id) => Event::Init {
                claude_session_id: id.to_owned(),
            },
            None => Event::Ignored,
        },
        // Subagent output comes with a parent tool use; it is not the Agent talking to us.
        Some("assistant") if value["parent_tool_use_id"].is_null() => {
            let texts: Vec<String> = value["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|block| block["type"] == "text")
                .filter_map(|block| block["text"].as_str())
                .map(str::to_owned)
                .collect();
            Event::AssistantText(texts)
        }
        Some("result") => Event::Result {
            is_error: value["is_error"].as_bool().unwrap_or(false),
        },
        _ => Event::Ignored,
    }
}

/// What `event` means for `session`'s Transcript and status.
pub fn updates_for(session: SessionId, event: &Event) -> Vec<SessionUpdate> {
    match event {
        Event::Init { .. } => vec![SessionUpdate::Status {
            session,
            status: SessionStatus::Running,
        }],
        Event::AssistantText(texts) => texts
            .iter()
            .map(|text| SessionUpdate::TranscriptItem {
                session,
                item: TranscriptItem::Assistant(text.clone()),
            })
            .collect(),
        Event::Result { .. } => vec![SessionUpdate::Status {
            session,
            status: SessionStatus::WaitingForInput,
        }],
        Event::Ignored => Vec::new(),
    }
}

/// [`parse_line`] then [`updates_for`], for tests that do not need the event itself.
#[cfg(test)]
pub fn map_line(session: SessionId, line: &str) -> Vec<SessionUpdate> {
    updates_for(session, &parse_line(line))
}

#[cfg(test)]
mod tests {
    use lodestar_core::session::{SessionId, SessionStatus, SessionUpdate, TranscriptItem};

    use super::*;

    const TEXT_ONLY: &str = include_str!("../../fixtures/stream-json/text-only.jsonl");
    const MULTI_TURN: &str = include_str!("../../fixtures/stream-json/multi-turn.jsonl");
    const TOOL_CALL: &str = include_str!("../../fixtures/stream-json/tool-call.jsonl");
    const TOOL_DENIED: &str = include_str!("../../fixtures/stream-json/tool-denied.jsonl");
    const PARTIAL_MESSAGES: &str =
        include_str!("../../fixtures/stream-json/partial-messages.jsonl");

    fn mapped(recording: &str) -> Vec<SessionUpdate> {
        recording
            .lines()
            .flat_map(|line| map_line(SessionId(7), line))
            .collect()
    }

    fn assistant(text: &str) -> SessionUpdate {
        SessionUpdate::TranscriptItem {
            session: SessionId(7),
            item: TranscriptItem::Assistant(text.into()),
        }
    }

    fn status(status: SessionStatus) -> SessionUpdate {
        SessionUpdate::Status {
            session: SessionId(7),
            status,
        }
    }

    #[test]
    fn a_text_only_turn_runs_then_shows_the_reply_then_waits_for_input() {
        assert_eq!(
            mapped(TEXT_ONLY),
            [
                status(SessionStatus::Running),
                assistant("pong"),
                status(SessionStatus::WaitingForInput),
            ]
        );
    }

    #[test]
    fn every_turn_runs_then_waits_even_though_init_is_sent_again() {
        assert_eq!(
            mapped(MULTI_TURN),
            [
                status(SessionStatus::Running),
                assistant("one"),
                status(SessionStatus::WaitingForInput),
                status(SessionStatus::Running),
                assistant("one"),
                status(SessionStatus::WaitingForInput),
            ]
        );
    }

    #[test]
    fn tool_calls_and_their_results_are_ignored_for_now_and_the_reply_still_arrives() {
        assert_eq!(
            mapped(TOOL_CALL),
            [
                status(SessionStatus::Running),
                assistant(
                    "`echo hello` printed \"hello\", and notes.txt says lodestar is a desktop app."
                ),
                status(SessionStatus::WaitingForInput),
            ]
        );
    }

    #[test]
    fn a_denied_tool_call_is_ignored_too_and_the_turn_still_ends_waiting() {
        let updates = mapped(TOOL_DENIED);

        assert_eq!(updates.len(), 3);
        assert_eq!(updates[0], status(SessionStatus::Running));
        assert!(matches!(
            &updates[1],
            SessionUpdate::TranscriptItem { item: TranscriptItem::Assistant(text), .. }
                if text.contains("denied")
        ));
        assert_eq!(updates[2], status(SessionStatus::WaitingForInput));
    }

    #[test]
    fn partial_message_chunks_add_nothing_to_the_complete_reply() {
        assert_eq!(
            mapped(PARTIAL_MESSAGES),
            [
                status(SessionStatus::Running),
                assistant("pong pong."),
                status(SessionStatus::WaitingForInput),
            ]
        );
    }

    #[test]
    fn lines_that_are_not_json_or_not_known_are_ignored() {
        assert_eq!(map_line(SessionId(7), ""), []);
        assert_eq!(map_line(SessionId(7), "not json at all"), []);
        assert_eq!(map_line(SessionId(7), r#"{"type":"bogus"}"#), []);
        assert_eq!(
            map_line(SessionId(7), r#"{"type":"system","subtype":"init"}"#),
            []
        );
        assert_eq!(map_line(SessionId(7), r#"{"type":"assistant"}"#), []);
        assert_eq!(map_line(SessionId(7), "[1, 2]"), []);
    }

    #[test]
    fn init_carries_claudes_own_session_id() {
        let first_line = TEXT_ONLY.lines().next().unwrap();

        assert_eq!(
            parse_line(first_line),
            Event::Init {
                claude_session_id: "e25be59c-9c60-4c20-a69b-1a7ecc1ebaf4".into()
            }
        );
    }
}
