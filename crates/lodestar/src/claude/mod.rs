//! The Milestone 2 session source: Claude Code run headless, one Agent per session.
//!
//! `process` owns the child processes and their reader threads; `mapper` turns the
//! stream-json lines they produce into `SessionUpdate`s without touching either.

use std::collections::HashMap;
use std::ffi::OsString;
use std::time::Duration;

use lodestar_core::session::{
    InitialSessions, Project, SessionId, SessionSource, SessionStatus, SessionUpdate,
    TranscriptItem,
};

use self::mapper::Event;
use self::process::{Agent, AgentEvent, AgentExit};

pub mod mapper;
pub mod process;

/// The environment variable that names the `claude` binary when it is not on PATH.
pub const CLAUDE_BIN_VAR: &str = "LODESTAR_CLAUDE_BIN";

/// Real sessions, each driven by its own Claude Code process. It starts with no sessions:
/// Milestone 2 does not persist them.
pub struct ClaudeSource {
    program: OsString,
    agents: HashMap<SessionId, Agent>,
    /// Claude Code's own id for each session, for resuming one day.
    claude_session_ids: HashMap<SessionId, String>,
    /// Updates made here (not by an Agent) since the last poll.
    pending: Vec<SessionUpdate>,
}

impl ClaudeSource {
    /// Drive Agents with `program`, the `claude` binary or a stand-in for it.
    pub fn new(program: OsString) -> Self {
        Self {
            program,
            agents: HashMap::new(),
            claude_session_ids: HashMap::new(),
            pending: Vec::new(),
        }
    }

    /// The binary named by [`CLAUDE_BIN_VAR`], or `claude` on PATH.
    pub fn from_env() -> Self {
        Self::new(std::env::var_os(CLAUDE_BIN_VAR).unwrap_or_else(|| "claude".into()))
    }

    /// The binary's name without its folder, as the user knows it.
    fn program_name(&self) -> String {
        std::path::Path::new(&self.program)
            .file_name()
            .unwrap_or(&self.program)
            .to_string_lossy()
            .into_owned()
    }

    fn updates_from(&mut self, session: SessionId, event: AgentEvent) -> Vec<SessionUpdate> {
        match event {
            AgentEvent::Line(line) => {
                let event = mapper::parse_line(&line);
                if let Event::Init { claude_session_id } = &event {
                    self.claude_session_ids
                        .entry(session)
                        .or_insert_with(|| claude_session_id.clone());
                }
                mapper::updates_for(session, &event)
            }
            AgentEvent::Exited(exit) => {
                self.agents.remove(&session);
                vec![
                    SessionUpdate::TranscriptItem {
                        session,
                        item: TranscriptItem::Error(self.exit_message(&exit)),
                    },
                    SessionUpdate::Status {
                        session,
                        status: SessionStatus::Done,
                    },
                ]
            }
        }
    }

    /// Words for the user about an Agent that is gone: how it ended and, when it said
    /// anything on stderr, what.
    fn exit_message(&self, exit: &AgentExit) -> String {
        let program = self.program_name();
        let mut message = match (exit.code, exit.signal) {
            (Some(code), _) => format!("{program} exited (code {code})."),
            (None, Some(signal)) => format!("{program} was killed (signal {signal})."),
            (None, None) => format!("{program} exited."),
        };
        if !exit.stderr.is_empty() {
            message.push('\n');
            message.push_str(&exit.stderr);
        }
        message
    }
}

impl SessionSource for ClaudeSource {
    fn initial_sessions(&mut self) -> InitialSessions {
        InitialSessions::default()
    }

    fn poll_updates(&mut self, _now: Duration) -> Vec<SessionUpdate> {
        let mut updates = std::mem::take(&mut self.pending);
        let mut sessions: Vec<SessionId> = self.agents.keys().copied().collect();
        sessions.sort();
        for session in sessions {
            let events = self
                .agents
                .get_mut(&session)
                .map(Agent::drain)
                .unwrap_or_default();
            for event in events {
                updates.extend(self.updates_from(session, event));
            }
        }
        updates
    }

    fn start_agent(&mut self, session: SessionId, project: &Project) -> Result<(), String> {
        if !project.path.is_dir() {
            return Err(format!(
                "The Project folder {} does not exist.",
                project.path.display()
            ));
        }
        let agent = Agent::spawn(&self.program, &project.path, session).map_err(|error| {
            format!(
                "Could not start {}: {error}. Install Claude Code or set {CLAUDE_BIN_VAR}.",
                self.program.to_string_lossy()
            )
        })?;
        self.agents.insert(session, agent);
        Ok(())
    }

    fn send_message(&mut self, session: SessionId, text: &str) {
        let Some(agent) = self.agents.get_mut(&session) else {
            return;
        };
        match agent.send(text) {
            Ok(()) => self.pending.push(SessionUpdate::Status {
                session,
                status: SessionStatus::Running,
            }),
            // The Agent is gone; the error-handling slice turns this into a Transcript item.
            Err(error) => eprintln!("lodestar: session {}: send failed: {error}", session.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use lodestar_core::session::{
        Project, ProjectId, SessionId, SessionSource, SessionStatus, SessionUpdate, TranscriptItem,
    };

    use super::*;

    /// A stand-in for the `claude` binary: it waits for the first stdin line, records it,
    /// replays a recorded turn on stdout, then stays alive until stdin closes, as claude does.
    #[cfg(unix)]
    fn fake_claude(name: &str) -> (PathBuf, PathBuf) {
        let record = fake_dir(name).join("stdin-first-line");
        let script = fake_claude_doing(
            name,
            &format!(
                "read -r line\nprintf '%s\\n' \"$line\" > '{}'\ncat '{RECORDING}'\ncat > /dev/null\n",
                record.display()
            ),
        );
        (script, record)
    }

    const RECORDING: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/stream-json/text-only.jsonl"
    );

    fn fake_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lodestar-fake-claude-{name}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A stand-in for the `claude` binary that runs `body` as a shell script.
    #[cfg(unix)]
    fn fake_claude_doing(name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;

        let script = fake_dir(name).join("claude");
        fs::write(&script, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    fn tmp_project() -> Project {
        Project {
            id: ProjectId(0),
            name: "tmp".into(),
            path: std::env::temp_dir(),
        }
    }

    fn poll_until(
        source: &mut ClaudeSource,
        done: impl Fn(&[SessionUpdate]) -> bool,
    ) -> Vec<SessionUpdate> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut all = Vec::new();
        while Instant::now() < deadline {
            all.extend(source.poll_updates(Duration::ZERO));
            if done(&all) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        all
    }

    fn reached(updates: &[SessionUpdate], wanted: SessionStatus) -> bool {
        updates.iter().any(
            |update| matches!(update, SessionUpdate::Status { status, .. } if *status == wanted),
        )
    }

    fn poll_until_waiting(source: &mut ClaudeSource) -> Vec<SessionUpdate> {
        poll_until(source, |all| reached(all, SessionStatus::WaitingForInput))
    }

    fn poll_until_done(source: &mut ClaudeSource) -> Vec<SessionUpdate> {
        poll_until(source, |all| reached(all, SessionStatus::Done))
    }

    #[cfg(unix)]
    #[test]
    fn an_agent_that_exits_with_an_error_ends_the_session_with_its_code_and_stderr() {
        let program = fake_claude_doing("exit", "echo 'error: unknown option' >&2\nexit 1\n");
        let mut source = ClaudeSource::new(program.into());

        source.start_agent(SessionId(3), &tmp_project()).unwrap();
        let updates = poll_until_done(&mut source);

        assert_eq!(
            updates,
            [
                SessionUpdate::TranscriptItem {
                    session: SessionId(3),
                    item: TranscriptItem::Error(
                        "claude exited (code 1).\nerror: unknown option".into()
                    ),
                },
                SessionUpdate::Status {
                    session: SessionId(3),
                    status: SessionStatus::Done
                },
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_started_agent_gets_the_message_as_one_json_line_and_its_turn_streams_back() {
        let (program, record) = fake_claude("turn");
        let project = Project {
            id: ProjectId(0),
            name: "play".into(),
            path: std::env::temp_dir(),
        };
        let mut source = ClaudeSource::new(program.into());

        source.start_agent(SessionId(3), &project).unwrap();
        source.send_message(SessionId(3), "Say hello in one sentence.");
        let updates = poll_until_waiting(&mut source);

        let sent: serde_json::Value =
            serde_json::from_str(fs::read_to_string(&record).unwrap().trim()).unwrap();
        assert_eq!(sent["type"], "user");
        assert_eq!(sent["message"]["content"], "Say hello in one sentence.");
        assert_eq!(
            updates,
            [
                SessionUpdate::Status {
                    session: SessionId(3),
                    status: SessionStatus::Running
                },
                SessionUpdate::Status {
                    session: SessionId(3),
                    status: SessionStatus::Running
                },
                SessionUpdate::TranscriptItem {
                    session: SessionId(3),
                    item: TranscriptItem::Assistant("pong".into())
                },
                SessionUpdate::Status {
                    session: SessionId(3),
                    status: SessionStatus::WaitingForInput
                },
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_agent_killed_from_outside_ends_the_session_naming_the_signal() {
        let program = fake_claude_doing("killed", "kill -KILL $$\n");
        let mut source = ClaudeSource::new(program.into());

        source.start_agent(SessionId(3), &tmp_project()).unwrap();
        let updates = poll_until_done(&mut source);

        assert_eq!(
            updates,
            [
                SessionUpdate::TranscriptItem {
                    session: SessionId(3),
                    item: TranscriptItem::Error("claude was killed (signal 9).".into()),
                },
                SessionUpdate::Status {
                    session: SessionId(3),
                    status: SessionStatus::Done
                },
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn lines_that_are_not_json_are_skipped_and_the_turn_still_arrives() {
        let program = fake_claude_doing(
            "garbage",
            &format!(
                "echo 'not json at all'\necho '{{\"type\": 42}}'\ncat '{RECORDING}'\ncat > /dev/null\n"
            ),
        );
        let mut source = ClaudeSource::new(program.into());

        source.start_agent(SessionId(3), &tmp_project()).unwrap();
        let updates = poll_until_waiting(&mut source);

        assert_eq!(
            updates,
            [
                SessionUpdate::Status {
                    session: SessionId(3),
                    status: SessionStatus::Running
                },
                SessionUpdate::TranscriptItem {
                    session: SessionId(3),
                    item: TranscriptItem::Assistant("pong".into())
                },
                SessionUpdate::Status {
                    session: SessionId(3),
                    status: SessionStatus::WaitingForInput
                },
            ]
        );
        assert!(source.poll_updates(Duration::ZERO).is_empty());
    }

    #[test]
    fn an_agent_cannot_start_in_a_folder_that_does_not_exist() {
        let project = Project {
            id: ProjectId(0),
            name: "gone".into(),
            path: "/nonexistent/lodestar/project".into(),
        };
        let mut source = ClaudeSource::new("claude".into());

        let started = source.start_agent(SessionId(3), &project);

        assert!(started.is_err());
        assert!(source.poll_updates(Duration::ZERO).is_empty());
    }

    #[test]
    fn a_missing_binary_is_reported_when_the_agent_starts() {
        let mut source = ClaudeSource::new("lodestar-no-such-claude-binary".into());

        let started = source.start_agent(SessionId(3), &tmp_project());

        assert!(
            started
                .unwrap_err()
                .contains("lodestar-no-such-claude-binary")
        );
    }
}
