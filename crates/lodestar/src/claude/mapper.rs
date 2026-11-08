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

/// [`parse_line`] then [`updates_for`], for callers that do not need the event itself.
pub fn map_line(session: SessionId, line: &str) -> Vec<SessionUpdate> {
    updates_for(session, &parse_line(line))
}

#[cfg(test)]
mod tests {
    use lodestar_core::session::{SessionId, SessionStatus, SessionUpdate, TranscriptItem};

    use super::*;

    const TEXT_ONLY: &str = include_str!("../../fixtures/stream-json/text-only.jsonl");

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
}
