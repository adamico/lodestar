//! From Claude Code's stream-json lines to `SessionUpdate`s, as a pure function.
//!
//! Every line is one JSON object with a `type`. `system`/`init` opens a turn, `assistant`
//! carries what the Agent said and the tools it called, `user` carries those tools' results
//! (a refused tool's result is an error that says so, so `system`/`permission_denied`, which
//! repeats it, is ignored), and `result` closes the turn, with an Error item when the turn
//! failed. A `prompt_suggestion` may follow, with a guess at the user's next message.
//! Partial messages (`stream_event`, and `system`/`status` while requesting) only move the
//! working indicator, through a [`TurnTally`]; whole items still come from `assistant`.
//! Everything else (progress blurbs, rate-limit notes, lines that are not JSON) is ignored
//! without complaint, because Claude Code adds event kinds freely and a stray line must never
//! take the session down.
//!
//! A tool call and its result are two lines linked by the tool_use id, so a call becomes a
//! pending `ToolCall` item and its result a `SessionUpdate::ToolResult` (ADR 0002).

use lodestar_core::session::{SessionId, SessionStatus, SessionUpdate, ToolCall, TranscriptItem};
use lodestar_core::turn::Phase;
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
    /// A partial message: how far the API message being written has got.
    Partial(Partial),
    /// The Agent's guess at the user's next message, sent after a turn's `result` when it
    /// runs with `--prompt-suggestions`.
    Suggestion(String),
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

/// One partial-message line, as far as the working indicator cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Partial {
    /// Claude Code is sending a request to the API.
    Requesting,
    /// An API message began, with what it has written so far.
    MessageStart { output_tokens: u64 },
    /// A content block began: thinking, text or a tool call.
    BlockStart(Phase),
    /// The API message's running output token count.
    MessageDelta { output_tokens: u64 },
    /// The API message is complete.
    MessageStop,
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
        Some("system") if value["subtype"] == "status" && value["status"] == "requesting" => {
            Event::Partial(Partial::Requesting)
        }
        Some("prompt_suggestion") => match value["suggestion"].as_str().map(str::trim) {
            Some(text) if !text.is_empty() => Event::Suggestion(text.to_owned()),
            _ => Event::Ignored,
        },
        Some("stream_event") if value["parent_tool_use_id"].is_null() => {
            partial(&value["event"]).map_or(Event::Ignored, Event::Partial)
        }
        _ => Event::Ignored,
    }
}

/// What a `stream_event`'s `event` says about progress, if anything.
fn partial(event: &Value) -> Option<Partial> {
    match event["type"].as_str()? {
        "message_start" => Some(Partial::MessageStart {
            output_tokens: event["message"]["usage"]["output_tokens"].as_u64()?,
        }),
        "content_block_start" => {
            let block = &event["content_block"];
            let phase = match block["type"].as_str()? {
                "thinking" | "redacted_thinking" => Phase::Thinking,
                "text" => Phase::Writing,
                "tool_use" | "server_tool_use" => Phase::Tool(block["name"].as_str()?.to_owned()),
                _ => return None,
            };
            Some(Partial::BlockStart(phase))
        }
        "message_delta" => Some(Partial::MessageDelta {
            output_tokens: event["usage"]["output_tokens"].as_u64()?,
        }),
        "message_stop" => Some(Partial::MessageStop),
        _ => None,
    }
}

/// What a turn has done so far, folded over its partial messages: its phase, and its output
/// tokens. Each API message reports a running count of its own, so the turn's total is the
/// finished messages' tokens plus the current one's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnTally {
    phase: Phase,
    finished_tokens: u64,
    message_tokens: u64,
}

impl Default for TurnTally {
    fn default() -> Self {
        Self {
            phase: Phase::Working,
            finished_tokens: 0,
            message_tokens: 0,
        }
    }
}

impl TurnTally {
    /// The tally after `event`, and the Progress update for `session` when the phase or the
    /// token count changed. A turn's start and end begin a fresh tally.
    pub fn next(&self, session: SessionId, event: &Event) -> (TurnTally, Option<SessionUpdate>) {
        let mut next = self.clone();
        match event {
            Event::Init { .. } | Event::Result { .. } => return (TurnTally::default(), None),
            Event::Partial(Partial::Requesting) => next.phase = Phase::Working,
            Event::Partial(Partial::MessageStart { output_tokens })
            | Event::Partial(Partial::MessageDelta { output_tokens }) => {
                next.message_tokens = *output_tokens
            }
            Event::Partial(Partial::BlockStart(phase)) => next.phase = phase.clone(),
            Event::Partial(Partial::MessageStop) => {
                next.finished_tokens += next.message_tokens;
                next.message_tokens = 0;
            }
            _ => {}
        }
        let changed = next.phase != self.phase || next.output_tokens() != self.output_tokens();
        let update = changed.then(|| SessionUpdate::Progress {
            session,
            phase: next.phase.clone(),
            output_tokens: next.output_tokens(),
        });
        (next, update)
    }

    fn output_tokens(&self) -> u64 {
        self.finished_tokens + self.message_tokens
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
        Event::Suggestion(text) => vec![SessionUpdate::Suggestion {
            session,
            text: text.clone(),
        }],
        Event::Partial(_) | Event::Ignored => Vec::new(),
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
    use lodestar_core::turn::Phase;

    use super::*;

    const TEXT_ONLY: &str = include_str!("../../fixtures/stream-json/text-only.jsonl");
    const MULTI_TURN: &str = include_str!("../../fixtures/stream-json/multi-turn.jsonl");
    const TOOL_CALL: &str = include_str!("../../fixtures/stream-json/tool-call.jsonl");
    const TOOL_DENIED: &str = include_str!("../../fixtures/stream-json/tool-denied.jsonl");
    const PARTIAL_MESSAGES: &str =
        include_str!("../../fixtures/stream-json/partial-messages.jsonl");
    /// Sonnet with `--effort high`, `--settings '{"alwaysThinkingEnabled":true}'`,
    /// `--allowedTools Bash` and `--include-partial-messages`, asked to think about whether
    /// 391 is prime, run `echo hi` and answer: one API message that thinks then calls Bash
    /// (113 output tokens), then one that writes the answer (24).
    const THINKING_TOOL: &str = include_str!("../../fixtures/stream-json/thinking-tool.jsonl");
    /// Haiku with `--prompt-suggestions true`, over two turns: asked what is wrong with an
    /// `add` that subtracts (no suggestion follows), then to show the fixed line, after
    /// which Claude Code suggests "run cargo test".
    const PROMPT_SUGGESTION: &str =
        include_str!("../../fixtures/stream-json/prompt-suggestion.jsonl");

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
    fn a_prompt_suggestion_after_a_turn_becomes_the_sessions_suggestion() {
        let updates = mapped(PROMPT_SUGGESTION);

        assert_eq!(
            updates
                .iter()
                .filter(|update| !matches!(update, SessionUpdate::TranscriptItem { .. }))
                .collect::<Vec<_>>(),
            [
                &status(SessionStatus::Running),
                &status(SessionStatus::WaitingForInput),
                &status(SessionStatus::Running),
                &status(SessionStatus::WaitingForInput),
                &SessionUpdate::Suggestion {
                    session: SessionId(7),
                    text: "run cargo test".into()
                },
            ]
        );
    }

    #[test]
    fn a_blank_or_missing_suggestion_is_ignored() {
        assert_eq!(
            map_line(
                SessionId(7),
                r#"{"type":"prompt_suggestion","suggestion":"  "}"#
            ),
            []
        );
        assert_eq!(
            map_line(SessionId(7), r#"{"type":"prompt_suggestion"}"#),
            []
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

    /// The Progress updates a turn's lines make, folding one tally over them as the source does.
    fn progress_through(recording: &str) -> Vec<SessionUpdate> {
        let mut tally = TurnTally::default();
        let mut updates = Vec::new();
        for line in recording.lines() {
            let (next, update) = tally.next(SessionId(7), &parse_line(line));
            tally = next;
            updates.extend(update);
        }
        updates
    }

    fn progress(phase: Phase, output_tokens: u64) -> SessionUpdate {
        SessionUpdate::Progress {
            session: SessionId(7),
            phase,
            output_tokens,
        }
    }

    #[test]
    fn a_turn_reports_each_phase_and_sums_output_tokens_across_its_api_messages() {
        assert_eq!(
            progress_through(THINKING_TOOL),
            [
                progress(Phase::Working, 7),
                progress(Phase::Thinking, 7),
                progress(Phase::Tool("Bash".into()), 7),
                progress(Phase::Tool("Bash".into()), 113),
                progress(Phase::Working, 113),
                progress(Phase::Working, 137),
                progress(Phase::Writing, 137),
            ]
        );
    }

    #[test]
    fn partial_messages_drive_progress_but_add_no_transcript_items() {
        assert_eq!(
            mapped(THINKING_TOOL)
                .iter()
                .filter(|update| matches!(update, SessionUpdate::TranscriptItem { .. }))
                .count(),
            2
        );
        assert_eq!(
            progress_through(PARTIAL_MESSAGES),
            [progress(Phase::Working, 7), progress(Phase::Writing, 7)]
        );
    }
}
