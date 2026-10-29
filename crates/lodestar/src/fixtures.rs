//! The Milestone 1 session source: sessions read from fixture files bundled with the app.

use std::path::PathBuf;

use lodestar_core::session::{
    InitialSessions, Project, ProjectId, Session, SessionId, SessionSource, SessionStatus,
    ToolCall, Transcript, TranscriptEntry,
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
    #[serde(default)]
    transcript: Vec<FixtureEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FixtureEntry {
    User(String),
    Assistant(String),
    ToolCall {
        summary: String,
        input: String,
        output: String,
    },
}

impl From<FixtureEntry> for TranscriptEntry {
    fn from(entry: FixtureEntry) -> Self {
        match entry {
            FixtureEntry::User(text) => TranscriptEntry::User(text),
            FixtureEntry::Assistant(text) => TranscriptEntry::Assistant(text),
            FixtureEntry::ToolCall {
                summary,
                input,
                output,
            } => TranscriptEntry::ToolCall(ToolCall {
                summary,
                input,
                output,
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
                    transcript: Transcript {
                        entries: session.transcript.into_iter().map(Into::into).collect(),
                    },
                }
            })
            .collect();
        InitialSessions { projects, sessions }
    }
}
