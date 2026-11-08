//! The Milestone 1 session source: sessions read from fixture files bundled with the app.

use std::path::PathBuf;

use lodestar_core::session::{
    InitialSessions, Project, ProjectId, Session, SessionId, SessionSource, SessionStatus,
    ToolCall, Transcript, TranscriptItem,
};
use serde::Deserialize;

const SESSIONS: &str = include_str!("../fixtures/sessions.json");

pub struct FixtureSource;

impl SessionSource for FixtureSource {
    fn initial_sessions(&mut self) -> InitialSessions {
        let file: FixtureFile =
            serde_json::from_str(SESSIONS).expect("bundled fixtures/sessions.json is valid");
        file.into_initial_sessions()
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

impl From<FixtureItem> for TranscriptItem {
    fn from(item: FixtureItem) -> Self {
        match item {
            FixtureItem::User(text) => TranscriptItem::User(text),
            FixtureItem::Assistant(markdown) => TranscriptItem::Assistant(markdown),
            FixtureItem::ToolCall(call) => TranscriptItem::ToolCall(ToolCall {
                name: call.name,
                summary: call.summary,
                input: call.input,
                output: call.output,
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

impl FixtureFile {
    fn into_initial_sessions(self) -> InitialSessions {
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
        let sessions = self
            .sessions
            .into_iter()
            .zip(0..)
            .map(|(session, id)| {
                let project = projects
                    .iter()
                    .find(|project| project.name == session.project)
                    .unwrap_or_else(|| panic!("fixture project {:?} exists", session.project));
                Session {
                    id: SessionId(id),
                    title: session.title,
                    project: project.id,
                    status: match session.status {
                        FixtureStatus::Running => SessionStatus::Running,
                        FixtureStatus::WaitingForInput => SessionStatus::WaitingForInput,
                        FixtureStatus::Done => SessionStatus::Done,
                    },
                    started_at: session.started_at,
                    transcript: Transcript::new(
                        session.transcript.into_iter().map(Into::into).collect(),
                    ),
                }
            })
            .collect();
        InitialSessions { projects, sessions }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_fixtures_load_with_every_kind_of_transcript_item() {
        let InitialSessions { sessions, .. } = FixtureSource.initial_sessions();

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
        let InitialSessions { sessions, .. } = FixtureSource.initial_sessions();

        assert!(
            sessions
                .iter()
                .any(|session| session.transcript.items().len() >= 25)
        );
    }
}
