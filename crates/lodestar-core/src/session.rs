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

/// The ordered messages and tool calls that make up a session's history.
///
/// For now each item is plain text; richer items (markdown, tool calls) come later.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transcript {
    items: Vec<String>,
}

impl Transcript {
    pub fn new(items: Vec<String>) -> Self {
        Self { items }
    }

    /// The items in the order they happened.
    pub fn items(&self) -> &[String] {
        &self.items
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

    /// Every session, newest first.
    pub fn sessions_newest_first(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.iter().collect();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.started_at));
        sessions
    }

    pub fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|session| session.id == id)
    }

    /// Start a new session in `project`: it waits for input with an empty Transcript.
    ///
    /// It is titled "<project name> #n", where n counts the sessions in the Project.
    pub fn create_session(&mut self, project: ProjectId) -> SessionId {
        let name = self.project(project).map(|p| p.name.as_str()).unwrap_or("");
        let title = format!("{name} #{}", self.sessions_in(project).len() + 1);
        let id = SessionId(
            self.sessions
                .iter()
                .map(|session| session.id.0 + 1)
                .max()
                .unwrap_or(0),
        );
        let started_at = self
            .sessions
            .iter()
            .map(|session| session.started_at + 1)
            .max()
            .unwrap_or(0);
        self.sessions.push(Session {
            id,
            title,
            project,
            status: SessionStatus::WaitingForInput,
            started_at,
            transcript: Transcript::default(),
        });
        id
    }
}
