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

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub project: ProjectId,
    pub status: SessionStatus,
    /// Seconds since the Unix epoch. Orders sessions newest first.
    pub started_at: u64,
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

    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|session| session.id == id)
    }
}
