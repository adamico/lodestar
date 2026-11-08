//! Sessions, the Projects they work in, and the seam they arrive through.

use std::path::{Path, PathBuf};
use std::time::Duration;

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

impl Project {
    /// The name a project at `path` goes by: its folder's name, or the whole path when it has
    /// no final component (such as `/`).
    pub fn name_for(path: &Path) -> String {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned())
    }
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

/// Something that happened to a session after startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionUpdate {
    /// An item that happened after every item so far in the session's Transcript.
    TranscriptItem {
        session: SessionId,
        item: TranscriptItem,
    },
    /// The session's status changed.
    Status {
        session: SessionId,
        status: SessionStatus,
    },
}

/// Where sessions come from: fixtures in Milestone 1, real agents from Milestone 2.
///
/// It supplies the sessions that exist at startup, then is polled for the updates that
/// arrived since. Polling keeps this crate free of threads: whoever owns the source (the
/// window) drives it from its own timer. Commands go the other way through the same seam:
/// the window creates a session in the store, then asks the source to start its Agent and
/// to carry messages to it. Views never reach an Agent directly.
pub trait SessionSource {
    fn initial_sessions(&mut self) -> InitialSessions;

    /// The updates that arrived up to `now`, measured from startup, in the order they happened.
    /// Each update is returned once. Sources that never change return nothing.
    fn poll_updates(&mut self, now: Duration) -> Vec<SessionUpdate> {
        let _ = now;
        Vec::new()
    }

    /// Start the Agent that drives `session`, working in `project`. Whatever the Agent does
    /// from here arrives through `poll_updates`. The error is a message for the user, for
    /// when no Agent could be started at all. Sources without Agents start nothing.
    fn start_agent(&mut self, session: SessionId, project: &Project) -> Result<(), String> {
        let _ = (session, project);
        Ok(())
    }

    /// Carry the user's `text` to `session`'s Agent, which starts a turn. The Transcript item
    /// for the message is the caller's to push. Sources without Agents drop it.
    fn send_message(&mut self, session: SessionId, text: &str) {
        let _ = (session, text);
    }
}

/// Every known Project and session.
#[derive(Debug, Clone, Default)]
pub struct SessionStore {
    projects: Vec<Project>,
    sessions: Vec<Session>,
    /// Most recent first. Starts as the Projects' folders; folders browsed this run join it.
    recent_folders: Vec<PathBuf>,
}

impl SessionStore {
    pub fn load(source: &mut impl SessionSource) -> Self {
        let InitialSessions { projects, sessions } = source.initial_sessions();
        let recent_folders = projects
            .iter()
            .map(|project| project.path.clone())
            .collect();
        Self {
            projects,
            sessions,
            recent_folders,
        }
    }

    /// Folders to offer for a new session, most recently used first.
    pub fn recent_folders(&self) -> &[PathBuf] {
        &self.recent_folders
    }

    /// Move `path` to the front of the recent folders, so it is offered first.
    pub fn add_recent_folder(&mut self, path: PathBuf) {
        self.recent_folders.retain(|folder| *folder != path);
        self.recent_folders.insert(0, path);
    }

    /// The Project at `path`, created if none works there. A new Project is named after the
    /// folder's last component.
    pub fn find_or_create_project(&mut self, path: PathBuf) -> ProjectId {
        if let Some(project) = self.projects.iter().find(|project| project.path == path) {
            return project.id;
        }
        let id = ProjectId(
            self.projects
                .iter()
                .map(|project| project.id.0 + 1)
                .max()
                .unwrap_or(0),
        );
        let name = Project::name_for(&path);
        self.projects.push(Project { id, name, path });
        id
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

    /// Record `update` on its session: append the item to its Transcript, or move it to the
    /// new status. Whether the session is shown anywhere is not the store's concern; updates
    /// land all the same. Updates for unknown sessions are dropped.
    pub fn apply(&mut self, update: SessionUpdate) {
        match update {
            SessionUpdate::TranscriptItem { session, item } => {
                if let Some(session) = self.session_mut(session) {
                    session.transcript.push(item);
                }
            }
            SessionUpdate::Status { session, status } => {
                if let Some(session) = self.session_mut(session) {
                    session.status = status;
                }
            }
        }
    }

    fn session_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|session| session.id == id)
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
