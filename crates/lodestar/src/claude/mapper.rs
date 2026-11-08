//! From Claude Code's stream-json lines to `SessionUpdate`s, as a pure function.
//!
//! Every line is one JSON object with a `type`. `system`/`init` opens a turn, `assistant`
//! carries what the Agent said and the tools it called, `user` carries those tools' results
//! (a refused tool's result is an error that says so, so `system`/`permission_denied`, which
//! repeats it, is ignored), and `result` closes the turn, with an Error item when the turn
//! failed. Everything else (progress blurbs, rate-limit notes, partial messages, lines that are not
//! JSON) is ignored without complaint, because Claude Code adds event kinds freely and a
//! stray line must never take the session down.
//!
//! A tool call and its result are two lines linked by the tool_use id, so a call becomes a
//! pending `ToolCall` item and its result a `SessionUpdate::ToolResult` (ADR 0002).

use lodestar_core::session::{SessionId, SessionStatus, SessionUpdate, ToolCall, TranscriptItem};
use serde_json::{Map, Value};

/// One stream-json line, reduced to what Lodestar reacts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A turn began. Claude Code sends one at the start of every turn, with the same id.
    Init { claude_session_id: String },
    /// What one `assistant` line carried, block by block: text as markdown, and tool calls.
    Assistant(Vec<AssistantBlock>),
    /// Results for tool calls, one per `tool_result` block, each with the call's id.
    ToolResults(Vec<(String, String)>),
    /// The turn ended, with why it failed when it did. The Agent is alive either way and can
    /// take another message.
    Result { error: Option<String> },
    /// A line Lodestar does not react to.
    Ignored,
}

/// One content block of an `assistant` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
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
            let blocks = content_blocks(&value)
                .filter_map(|block| match block["type"].as_str() {
                    Some("text") => block
                        .get("text")?
                        .as_str()
                        .map(|t| AssistantBlock::Text(t.to_owned())),
                    Some("tool_use") => Some(AssistantBlock::ToolUse {
                        id: block["id"].as_str()?.to_owned(),
                        name: block["name"].as_str()?.to_owned(),
                        input: block["input"].clone(),
                    }),
                    _ => None,
                })
                .collect();
            Event::Assistant(blocks)
        }
        Some("user") if value["parent_tool_use_id"].is_null() => {
            let results = content_blocks(&value)
                .filter(|block| block["type"] == "tool_result")
                .filter_map(|block| {
                    let id = block["tool_use_id"].as_str()?.to_owned();
                    let text = result_text(&block["content"]);
                    let output = if block["is_error"].as_bool().unwrap_or(false) {
                        format!("Error: {text}")
                    } else {
                        text
                    };
                    Some((id, output))
                })
                .collect();
            Event::ToolResults(results)
        }
        Some("result") => Event::Result {
            error: turn_error(&value),
        },
        _ => Event::Ignored,
    }
}

/// Why a `result` line with `is_error` says its turn failed: its subtype, then whatever text
/// it carries (`result`, or the `errors` list). `None` for a turn that succeeded.
fn turn_error(result: &Value) -> Option<String> {
    if !result["is_error"].as_bool().unwrap_or(false) {
        return None;
    }
    let subtype = result["subtype"].as_str().unwrap_or("error");
    let mut message = format!("The turn failed ({subtype}).");
    let text = match &result["result"] {
        Value::String(text) if !text.trim().is_empty() => text.trim().to_owned(),
        _ => result["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n"),
    };
    if !text.is_empty() {
        message.push('\n');
        message.push_str(&text);
    }
    Some(message)
}

fn content_blocks(line: &Value) -> impl Iterator<Item = &Value> {
    line["message"]["content"].as_array().into_iter().flatten()
}

/// A tool result's content is a string, or an array of text blocks joined by newlines.
fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Tools whose most useful input is the path they touch.
const FILE_TOOLS: [&str; 7] = [
    "Read",
    "Write",
    "Edit",
    "MultiEdit",
    "NotebookEdit",
    "Glob",
    "Grep",
];

/// The one input that tells the most about a call: the command for Bash, the path for file
/// tools, otherwise the whole input on one line.
fn call_summary(name: &str, input: &Value) -> String {
    let field = match name {
        "Bash" => input["command"].as_str(),
        _ if FILE_TOOLS.contains(&name) => input["file_path"]
            .as_str()
            .or_else(|| input["path"].as_str())
            .or_else(|| input["pattern"].as_str()),
        _ => None,
    };
    match field {
        Some(field) => field.to_owned(),
        None => compact_json(input),
    }
}

/// `input` as one line of JSON with its keys sorted, so equal inputs read the same.
fn compact_json(input: &Value) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut entries: Vec<(&String, &Value)> = map.iter().collect();
                entries.sort_by_key(|(key, _)| key.as_str());
                Value::Object(
                    entries
                        .into_iter()
                        .map(|(key, value)| (key.clone(), sorted(value)))
                        .collect::<Map<String, Value>>(),
                )
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    sorted(input).to_string()
}

/// The tool call item for a `tool_use` block, pending until its result arrives.
fn tool_call(id: &str, name: &str, input: &Value) -> ToolCall {
    ToolCall {
        id: id.to_owned(),
        name: name.to_owned(),
        summary: call_summary(name, input),
        input: serde_json::to_string_pretty(input).unwrap_or_default(),
        output: None,
    }
}

/// What `event` means for `session`'s Transcript and status.
pub fn updates_for(session: SessionId, event: &Event) -> Vec<SessionUpdate> {
    match event {
        Event::Init { .. } => vec![SessionUpdate::Status {
            session,
            status: SessionStatus::Running,
        }],
        Event::Assistant(blocks) => blocks
            .iter()
            .map(|block| SessionUpdate::TranscriptItem {
                session,
                item: match block {
                    AssistantBlock::Text(text) => TranscriptItem::Assistant(text.clone()),
                    AssistantBlock::ToolUse { id, name, input } => {
                        TranscriptItem::ToolCall(tool_call(id, name, input))
                    }
                },
            })
            .collect(),
        Event::ToolResults(results) => results
            .iter()
            .map(|(call_id, output)| SessionUpdate::ToolResult {
                session,
                call_id: call_id.clone(),
                output: output.clone(),
            })
            .collect(),
        Event::Result { error } => error
            .iter()
            .map(|error| SessionUpdate::TranscriptItem {
                session,
                item: TranscriptItem::Error(error.clone()),
            })
            .chain([SessionUpdate::Status {
                session,
                status: SessionStatus::WaitingForInput,
            }])
            .collect(),
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
    use lodestar_core::session::{
        SessionId, SessionStatus, SessionStore, SessionUpdate, ToolCall, TranscriptItem,
    };

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

    fn tool_result(call_id: &str, output: &str) -> SessionUpdate {
        SessionUpdate::ToolResult {
            session: SessionId(7),
            call_id: call_id.into(),
            output: output.into(),
        }
    }

    fn pending_call(update: &SessionUpdate) -> &ToolCall {
        match update {
            SessionUpdate::TranscriptItem {
                item: TranscriptItem::ToolCall(call),
                ..
            } => {
                assert!(call.is_pending(), "{call:?}");
                call
            }
            other => panic!("not a tool call: {other:?}"),
        }
    }

    #[test]
    fn a_tool_call_arrives_pending_and_its_result_fills_it_in_by_id() {
        let updates = mapped(TOOL_CALL);

        assert_eq!(updates.len(), 7);
        assert_eq!(updates[0], status(SessionStatus::Running));
        let bash = pending_call(&updates[1]);
        assert_eq!(bash.id, "toolu_019Q4LDyUZbxP95U1obMyzDr");
        assert_eq!(bash.headline(), "Bash  echo hello");
        assert!(bash.input.contains("\"command\": \"echo hello\""));
        assert_eq!(
            updates[2],
            tool_result("toolu_019Q4LDyUZbxP95U1obMyzDr", "hello")
        );
        let read = pending_call(&updates[3]);
        assert_eq!(read.id, "toolu_01NVrtf1eKfzsL3Y49cANcY1");
        assert!(
            read.headline().starts_with("Read  /tmp/"),
            "{}",
            read.headline()
        );
        assert!(read.headline().ends_with("\u{2026}"), "{}", read.headline());
        assert_eq!(
            updates[4],
            tool_result(
                "toolu_01NVrtf1eKfzsL3Y49cANcY1",
                "1\tThe notes file says: lodestar is a desktop app.\n2\t"
            )
        );
        assert_eq!(
            updates[5],
            assistant(
                "`echo hello` printed \"hello\", and notes.txt says lodestar is a desktop app."
            )
        );
        assert_eq!(updates[6], status(SessionStatus::WaitingForInput));
    }

    #[test]
    fn a_result_that_arrives_after_more_text_still_fills_in_its_call() {
        let lines: Vec<&str> = TOOL_CALL.lines().collect();
        let (tool_use, tool_result, text) = (lines[1], lines[2], lines[7]);
        let mut store = SessionStore::default();
        let project = store.find_or_create_project("/tmp/play".into());
        let session = store.create_session(project);

        for line in [tool_use, text, tool_result] {
            for update in map_line(session, line) {
                store.apply(update);
            }
        }

        let items = store.session(session).unwrap().transcript.items();
        assert_eq!(items.len(), 2);
        assert!(matches!(
            &items[0],
            TranscriptItem::ToolCall(call) if call.output.as_deref() == Some("hello")
        ));
        assert!(matches!(&items[1], TranscriptItem::Assistant(_)));
    }

    #[test]
    fn a_denied_tool_call_gets_one_result_saying_so_and_the_turn_still_ends_waiting() {
        let updates = mapped(TOOL_DENIED);

        assert_eq!(updates.len(), 5);
        assert_eq!(updates[0], status(SessionStatus::Running));
        let write = pending_call(&updates[1]);
        assert_eq!(write.id, "toolu_01Si71DtiY9TMSP8Ppycbuif");
        assert!(
            write.headline().starts_with("Write  /tmp/"),
            "{}",
            write.headline()
        );
        assert!(matches!(
            &updates[2],
            SessionUpdate::ToolResult { call_id, output, .. }
                if call_id == "toolu_01Si71DtiY9TMSP8Ppycbuif" && output.starts_with("Error: Claude requested permissions")
        ));
        assert!(matches!(
            &updates[3],
            SessionUpdate::TranscriptItem { item: TranscriptItem::Assistant(text), .. }
                if text.contains("denied")
        ));
        assert_eq!(updates[4], status(SessionStatus::WaitingForInput));
    }

    fn error(text: &str) -> SessionUpdate {
        SessionUpdate::TranscriptItem {
            session: SessionId(7),
            item: TranscriptItem::Error(text.into()),
        }
    }

    #[test]
    fn a_failed_turn_says_why_and_waits_for_input_again() {
        let line = r#"{"type":"result","subtype":"error_max_turns","is_error":true,"num_turns":3,"session_id":"s"}"#;

        assert_eq!(
            map_line(SessionId(7), line),
            [
                error("The turn failed (error_max_turns)."),
                status(SessionStatus::WaitingForInput),
            ]
        );
    }

    #[test]
    fn a_failed_turn_shows_the_error_text_it_carries() {
        let with_result = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"API Error: 529 overloaded"}"#;
        let with_errors = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["first","second"]}"#;

        assert_eq!(
            map_line(SessionId(7), with_result)[0],
            error("The turn failed (error_during_execution).\nAPI Error: 529 overloaded")
        );
        assert_eq!(
            map_line(SessionId(7), with_errors)[0],
            error("The turn failed (error_during_execution).\nfirst\nsecond")
        );
    }

    #[test]
    fn a_tool_result_may_carry_its_text_as_blocks() {
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_x","content":[{"type":"text","text":"one"},{"type":"text","text":"two"}]}]},"parent_tool_use_id":null}"#;

        assert_eq!(
            map_line(SessionId(7), line),
            [tool_result("toolu_x", "one\ntwo")]
        );
    }

    #[test]
    fn a_tool_without_a_known_headline_field_shows_its_input_compactly() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_y","name":"WebSearch","input":{"query":"gpui list","limit":3}}]},"parent_tool_use_id":null}"#;

        let updates = map_line(SessionId(7), line);

        assert_eq!(updates.len(), 1);
        let call = pending_call(&updates[0]);
        assert_eq!(
            call.headline(),
            r#"WebSearch  {"limit":3,"query":"gpui list"}"#
        );
    }

    #[test]
    fn file_tools_are_headlined_by_the_path_they_touch() {
        for name in ["Read", "Write", "Edit", "Glob", "Grep"] {
            let line = format!(
                r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"t","name":"{name}","input":{{"pattern":"x","file_path":"src/lib.rs","content":"..."}}}}]}},"parent_tool_use_id":null}}"#
            );

            let updates = map_line(SessionId(7), &line);

            assert_eq!(
                pending_call(&updates[0]).headline(),
                format!("{name}  src/lib.rs")
            );
        }
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
