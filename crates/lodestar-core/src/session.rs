//! Sessions, the Projects they work in, and the seam they arrive through.

use std::path::PathBuf;

/// Identifies a session for as long as the app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(pub u64);

/// Identifies a Project for as long as the app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectId(pub u64);

/// The folder a session works in.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub path: PathBuf,
}

/// Where a session stands right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    WaitingForInput,
    Done,
}

/// How many characters a tool call's headline may take before it is cut.
const HEADLINE_CHARS: usize = 80;

/// One tool the Agent ran: what it was asked and what came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub name: String,
    /// A short description of this call, usually the command or the file touched.
    pub summary: String,
    pub input: String,
    pub output: String,
}

impl ToolCall {
    /// The one line a collapsed tool call shows: the tool's name and the first line of its
    /// summary (or of its input, when it has no summary), cut with an ellipsis when too long.
    pub fn headline(&self) -> String {
        let summary = if self.summary.trim().is_empty() {
            &self.input
        } else {
            &self.summary
        };
        let first_line = summary.lines().next().unwrap_or_default().trim();
        let headline = format!("{}  {}", self.name, first_line);
        if headline.chars().count() <= HEADLINE_CHARS {
            return headline;
        }
        let mut cut: String = headline.chars().take(HEADLINE_CHARS - 1).collect();
        cut.push('\u{2026}');
        cut
    }
}

/// One entry in a Transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptItem {
    /// A message from the user, as plain text.
    User(String),
    /// A message from the Agent, as markdown.
    Assistant(String),
    ToolCall(ToolCall),
}

/// The ordered messages and tool calls that make up a session's history.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transcript {
    items: Vec<TranscriptItem>,
}

impl Transcript {
    pub fn new(items: Vec<TranscriptItem>) -> Self {
        Self { items }
    }

    /// The items in the order they happened.
    pub fn items(&self) -> &[TranscriptItem] {
        &self.items
    }

    /// Record an item that happened after every item so far.
    pub fn push(&mut self, item: TranscriptItem) {
        self.items.push(item);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub project: ProjectId,
    pub status: SessionStatus,
    /// Seconds since the Unix epoch. Orders sessions newest first.
    pub started_at: u64,
    pub transcript: Transcript,
}

/// What a session source supplies at startup.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InitialSessions {
    pub projects: Vec<Project>,
    pub sessions: Vec<Session>,
}

/// Where sessions come from: fixtures in Milestone 1, real agents from Milestone 2.
///
/// It will also push Transcript and status updates over time; for now it only supplies the
/// sessions that exist at startup.
pub trait SessionSource {
    fn initial_sessions(&mut self) -> InitialSessions;
}

/// Every known Project and session.
#[derive(Debug, Clone, Default)]
pub struct SessionStore {
    projects: Vec<Project>,
    sessions: Vec<Session>,
}

impl SessionStore {
    pub fn load(source: &mut impl SessionSource) -> Self {
        let InitialSessions { projects, sessions } = source.initial_sessions();
        Self { projects, sessions }
    }

    /// Projects in the order the source supplied them.
    pub fn projects(&self) -> &[Project] {
        &self.projects
    }

    /// The sessions in `project`, newest first.
    pub fn sessions_in(&self, project: ProjectId) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| session.project == project)
            .collect();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.started_at));
        sessions
    }

    pub fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|session| session.id == id)
    }
}
