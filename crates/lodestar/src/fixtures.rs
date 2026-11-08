//! The Milestone 1 session source: sessions read from fixture files bundled with the app.

use std::path::PathBuf;
use std::time::Duration;

use lodestar_core::session::{
    InitialSessions, Project, ProjectId, Session, SessionId, SessionSource, SessionStatus,
    SessionUpdate, ToolCall, Transcript, TranscriptItem,
};
use serde::Deserialize;

use self::schedule::UpdateSchedule;

mod schedule;

const SESSIONS: &str = include_str!("../fixtures/sessions.json");

/// Sessions from `fixtures/sessions.json`. The ones with a `stream` play it back on the clock
/// the window supplies, so they look live.
pub struct FixtureSource {
    initial: InitialSessions,
    stream: UpdateSchedule,
}

impl FixtureSource {
    pub fn new() -> Self {
        let file: FixtureFile =
            serde_json::from_str(SESSIONS).expect("bundled fixtures/sessions.json is valid");
        let (initial, stream) = file.into_initial_sessions_and_stream();
        Self { initial, stream }
    }
}

impl SessionSource for FixtureSource {
    fn initial_sessions(&mut self) -> InitialSessions {
        self.initial.clone()
    }

    fn poll_updates(&mut self, now: Duration) -> Vec<SessionUpdate> {
        self.stream.due(now)
    }
}

#[derive(Deserialize)]
struct FixtureFile {
    projects: Vec<FixtureProject>,
    sessions: Vec<FixtureSession>,
}

#[derive(Deserialize)]
struct FixtureProject {
    name: String,
    path: PathBuf,
}

#[derive(Deserialize)]
struct FixtureSession {
    title: String,
    /// The name of a project in the same file.
    project: String,
    status: FixtureStatus,
    started_at: u64,
    /// Transcript items, oldest first.
    #[serde(default)]
    transcript: Vec<FixtureItem>,
    /// What arrives after startup, each step at its `after_ms`. The last step should be a
    /// status, so the session stops running.
    #[serde(default)]
    stream: Vec<FixtureStreamStep>,
}

/// One step of a session's stream: `{"after_ms": 1500, "assistant": ".."}` or
/// `{"after_ms": 9000, "status": "done"}`.
#[derive(Deserialize)]
struct FixtureStreamStep {
    after_ms: u64,
    #[serde(flatten)]
    event: FixtureStreamEvent,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FixtureStreamEvent {
    User(String),
    Assistant(String),
    ToolCall(FixtureToolCall),
    Status(FixtureStatus),
}

impl FixtureStreamEvent {
    fn into_update(self, session: SessionId, ids: &mut CallIds) -> SessionUpdate {
        let item = match self {
            FixtureStreamEvent::User(text) => FixtureItem::User(text),
            FixtureStreamEvent::Assistant(markdown) => FixtureItem::Assistant(markdown),
            FixtureStreamEvent::ToolCall(call) => FixtureItem::ToolCall(call),
            FixtureStreamEvent::Status(status) => {
                return SessionUpdate::Status {
                    session,
                    status: status.into(),
                };
            }
        };
        SessionUpdate::TranscriptItem {
            session,
            item: item.into_transcript_item(ids),
        }
    }
}

/// Hands every fixture tool call an id of its own, as a real Agent would, so a
/// `SessionUpdate::ToolResult` could address it.
#[derive(Default)]
struct CallIds(u64);

impl CallIds {
    fn next(&mut self) -> String {
        self.0 += 1;
        format!("fixture-call-{}", self.0)
    }
}

/// One Transcript item as written in the fixture file:
/// `{"user": ".."}`, `{"assistant": ".."}` or `{"tool_call": {..}}`.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FixtureItem {
    User(String),
    Assistant(String),
    ToolCall(FixtureToolCall),
}

#[derive(Deserialize)]
struct FixtureToolCall {
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    input: String,
    #[serde(default)]
    output: String,
}

impl FixtureItem {
    fn into_transcript_item(self, ids: &mut CallIds) -> TranscriptItem {
        match self {
            FixtureItem::User(text) => TranscriptItem::User(text),
            FixtureItem::Assistant(markdown) => TranscriptItem::Assistant(markdown),
            FixtureItem::ToolCall(call) => TranscriptItem::ToolCall(ToolCall {
                id: ids.next(),
                name: call.name,
                summary: call.summary,
                input: call.input,
                output: Some(call.output),
            }),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FixtureStatus {
    Running,
    WaitingForInput,
    Done,
}

impl From<FixtureStatus> for SessionStatus {
    fn from(status: FixtureStatus) -> Self {
        match status {
            FixtureStatus::Running => SessionStatus::Running,
            FixtureStatus::WaitingForInput => SessionStatus::WaitingForInput,
            FixtureStatus::Done => SessionStatus::Done,
        }
    }
}

impl FixtureFile {
    fn into_initial_sessions_and_stream(self) -> (InitialSessions, UpdateSchedule) {
        let projects: Vec<Project> = self
            .projects
            .into_iter()
            .zip(0..)
            .map(|(project, id)| Project {
                id: ProjectId(id),
                name: project.name,
                path: project.path,
            })
            .collect();
        let mut stream = Vec::new();
        let mut ids = CallIds::default();
        let sessions = self
            .sessions
            .into_iter()
            .zip(0..)
            .map(|(session, id)| {
                let id = SessionId(id);
                let project = projects
                    .iter()
                    .find(|project| project.name == session.project)
                    .unwrap_or_else(|| panic!("fixture project {:?} exists", session.project));
                stream.extend(session.stream.into_iter().map(|step| {
                    (
                        Duration::from_millis(step.after_ms),
                        step.event.into_update(id, &mut ids),
                    )
                }));
                Session {
                    id,
                    title: session.title,
                    project: project.id,
                    status: session.status.into(),
                    started_at: session.started_at,
                    transcript: Transcript::new(
                        session
                            .transcript
                            .into_iter()
                            .map(|item| item.into_transcript_item(&mut ids))
                            .collect(),
                    ),
                }
            })
            .collect();
        (
            InitialSessions { projects, sessions },
            UpdateSchedule::new(stream),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lodestar_core::session::SessionUpdate;

    use super::*;

    #[test]
    fn the_bundled_fixtures_load_with_every_kind_of_transcript_item() {
        let InitialSessions { sessions, .. } = FixtureSource::new().initial_sessions();

        let items: Vec<&TranscriptItem> = sessions
            .iter()
            .flat_map(|session| session.transcript.items())
            .collect();
        assert!(
            items
                .iter()
                .any(|item| matches!(item, TranscriptItem::User(_)))
        );
        assert!(
            items
                .iter()
                .any(|item| matches!(item, TranscriptItem::Assistant(_)))
        );
        assert!(
            items
                .iter()
                .any(|item| matches!(item, TranscriptItem::ToolCall(_)))
        );
    }

    #[test]
    fn at_least_one_fixture_transcript_is_long_enough_to_scroll() {
        let InitialSessions { sessions, .. } = FixtureSource::new().initial_sessions();

        assert!(
            sessions
                .iter()
                .any(|session| session.transcript.items().len() >= 25)
        );
    }

    /// A streaming fixture: its initial state and every update it will ever send.
    fn streamed(source: &mut FixtureSource) -> Vec<(Session, Vec<SessionUpdate>)> {
        let InitialSessions { sessions, .. } = source.initial_sessions();
        let updates = source.poll_updates(Duration::from_secs(24 * 60 * 60));
        sessions
            .into_iter()
            .filter_map(|session| {
                let own: Vec<SessionUpdate> = updates
                    .iter()
                    .filter(|update| update.session() == session.id)
                    .cloned()
                    .collect();
                (!own.is_empty()).then_some((session, own))
            })
            .collect()
    }

    #[test]
    fn at_least_two_fixture_sessions_stream_from_a_running_mid_transcript_start() {
        let streamed = streamed(&mut FixtureSource::new());

        assert!(streamed.len() >= 2, "{} streaming sessions", streamed.len());
        for (session, updates) in &streamed {
            assert_eq!(session.status, SessionStatus::Running, "{}", session.title);
            assert!(!session.transcript.items().is_empty(), "{}", session.title);
            assert!(
                updates
                    .iter()
                    .any(|update| matches!(update, SessionUpdate::TranscriptItem { .. })),
                "{} streams no items",
                session.title
            );
        }
    }

    #[test]
    fn one_stream_ends_waiting_for_input_and_one_ends_done() {
        let streamed = streamed(&mut FixtureSource::new());

        let final_statuses: Vec<SessionStatus> = streamed
            .iter()
            .map(|(session, updates)| match updates.last() {
                Some(SessionUpdate::Status { status, .. }) => *status,
                other => panic!("{} does not end with a status: {other:?}", session.title),
            })
            .collect();
        assert!(final_statuses.contains(&SessionStatus::WaitingForInput));
        assert!(final_statuses.contains(&SessionStatus::Done));
    }

    #[test]
    fn nothing_has_streamed_at_startup_and_each_update_arrives_once() {
        let mut source = FixtureSource::new();
        source.initial_sessions();

        assert!(source.poll_updates(Duration::ZERO).is_empty());
        let all = source.poll_updates(Duration::from_secs(24 * 60 * 60));
        assert!(!all.is_empty());
        assert!(
            source
                .poll_updates(Duration::from_secs(48 * 60 * 60))
                .is_empty()
        );
    }

    #[test]
    fn a_fixture_agent_starts_and_takes_messages_without_streaming_anything() {
        let mut source = FixtureSource::new();
        let InitialSessions { projects, .. } = source.initial_sessions();
        let project = projects.first().unwrap().clone();

        let started = source.start_agent(SessionId(99), &project);
        source.send_message(SessionId(99), "Say hello in one sentence.");

        assert_eq!(started, Ok(()));
        assert!(source.poll_updates(Duration::ZERO).is_empty());
    }
}
