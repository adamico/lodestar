//! The Milestone 2 session source: Claude Code run headless, one Agent per session.
//!
//! `process` owns the child processes and their reader threads; `mapper` turns the
//! stream-json lines they produce into `SessionUpdate`s without touching either.

use std::collections::HashMap;
use std::ffi::OsString;
use std::time::Duration;

use lodestar_core::session::{InitialSessions, Project, SessionId, SessionSource, SessionUpdate};

use self::mapper::{Event, TurnTally};
use self::process::{Agent, AgentEvent};

pub mod mapper;
pub mod process;

/// The environment variable that names the `claude` binary when it is not on PATH.
pub const CLAUDE_BIN_VAR: &str = "LODESTAR_CLAUDE_BIN";

/// Real sessions, each driven by its own Claude Code process. It starts with no sessions:
/// Milestone 2 does not persist them.
pub struct ClaudeSource {
    program: OsString,
    agents: HashMap<SessionId, Agent>,
    /// Claude Code's own id for each session, from its first `system`/`init` line. The spec
    /// asks for it to be recorded: it is what ties a Lodestar session to Claude Code's.
    claude_session_ids: HashMap<SessionId, String>,
    /// How far each session's current turn has got, for the working indicator.
    tallies: HashMap<SessionId, TurnTally>,
}

impl ClaudeSource {
    /// Drive Agents with `program`, the `claude` binary or a stand-in for it.
    pub fn new(program: OsString) -> Self {
        Self {
            program,
            agents: HashMap::new(),
            claude_session_ids: HashMap::new(),
            tallies: HashMap::new(),
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
                let tally = self.tallies.entry(session).or_default();
                let (next, progress) = tally.next(session, &event);
                *tally = next;
                progress
                    .into_iter()
                    .chain(mapper::updates_for(session, &event))
                    .collect()
            }
            AgentEvent::Exited(exit) => {
                self.agents.remove(&session);
                self.tallies.remove(&session);
                vec![SessionUpdate::Ended {
                    session,
                    error: exit.describe(&self.program_name()),
                }]
            }
        }
    }
}

impl SessionSource for ClaudeSource {
    fn initial_sessions(&mut self) -> InitialSessions {
        InitialSessions::default()
    }

    fn poll_updates(&mut self, _now: Duration) -> Vec<SessionUpdate> {
        let mut updates = Vec::new();
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
                "The Project {} no longer exists on disk.",
                project.path.display()
            ));
        }
        let agent = Agent::spawn(&self.program, &project.path, session).map_err(|error| {
            let program = self.program_name();
            match error.kind() {
                std::io::ErrorKind::NotFound => format!(
                    "Couldn't start {program}: not found on PATH. \
                     Install Claude Code or set {CLAUDE_BIN_VAR}."
                ),
                _ => format!("Couldn't start {program}: {error}."),
            }
        })?;
        self.agents.insert(session, agent);
        Ok(())
    }

    fn send_message(&mut self, session: SessionId, text: &str) {
        let Some(agent) = self.agents.get_mut(&session) else {
            return;
        };
        // A write fails only when the Agent is gone or going. Its exit ends the session on a
        // poll soon, saying why; an error here as well would tell the user twice.
        let _ = agent.send(text);
    }

    /// Kill every Agent (dropping one kills it) and forget what they had to say.
    fn shutdown(&mut self) {
        self.agents.clear();
        self.tallies.clear();
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

    /// The committed stand-in for the `claude` binary. It runs `fake-claude-body` from the
    /// Project it starts in, so no test ever execs a file written at runtime (ETXTBSY).
    const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/fake-claude/claude");

    fn fake_source() -> ClaudeSource {
        ClaudeSource::new(FAKE_CLAUDE.into())
    }

    /// A fake Agent's Project: it waits for the first stdin line, records it, replays a
    /// recorded turn on stdout, then stays alive until stdin closes, as claude does. Also
    /// returns where the first line is recorded.
    #[cfg(unix)]
    fn fake_claude(name: &str) -> (Project, PathBuf) {
        let record = fake_dir(name).join("stdin-first-line");
        let project = fake_claude_doing(
            name,
            &format!(
                "read -r line\nprintf '%s\\n' \"$line\" > '{}'\ncat '{RECORDING}'\ncat > /dev/null\n",
                record.display()
            ),
        );
        (project, record)
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

    /// A Project in which [`FAKE_CLAUDE`] runs `body` as a shell script.
    #[cfg(unix)]
    fn fake_claude_doing(name: &str, body: &str) -> Project {
        let dir = fake_dir(name);
        fs::write(dir.join("fake-claude-body"), body).unwrap();
        project_in(dir)
    }

    fn project_in(path: PathBuf) -> Project {
        Project {
            id: ProjectId(0),
            name: "tmp".into(),
            path,
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
        poll_until(source, |all| {
            all.iter()
                .any(|update| matches!(update, SessionUpdate::Ended { .. }))
        })
    }

    #[cfg(unix)]
    #[test]
    fn an_agent_that_exits_with_an_error_ends_the_session_with_its_code_and_stderr() {
        let project = fake_claude_doing("exit", "echo 'error: unknown option' >&2\nexit 1\n");
        let mut source = fake_source();

        source.start_agent(SessionId(3), &project).unwrap();
        let updates = poll_until_done(&mut source);

        assert_eq!(
            updates,
            [SessionUpdate::Ended {
                session: SessionId(3),
                error: "claude exited (code 1).\nerror: unknown option".into()
            }]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_started_agent_gets_the_message_as_one_json_line_and_its_turn_streams_back() {
        let (project, record) = fake_claude("turn");
        let mut source = fake_source();

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
        let project = fake_claude_doing("killed", "kill -KILL $$\n");
        let mut source = fake_source();

        source.start_agent(SessionId(3), &project).unwrap();
        let updates = poll_until_done(&mut source);

        assert_eq!(
            updates,
            [SessionUpdate::Ended {
                session: SessionId(3),
                error: "claude was killed (signal 9).".into()
            }]
        );
    }

    #[cfg(unix)]
    #[test]
    fn lines_that_are_not_json_are_skipped_and_the_turn_still_arrives() {
        let project = fake_claude_doing(
            "garbage",
            &format!(
                "echo 'not json at all'\necho '{{\"type\": 42}}'\ncat '{RECORDING}'\ncat > /dev/null\n"
            ),
        );
        let mut source = fake_source();

        source.start_agent(SessionId(3), &project).unwrap();
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

    /// Whether a process with `pid` is still around.
    #[cfg(unix)]
    fn alive(pid: &str) -> bool {
        std::path::Path::new("/proc").join(pid).exists()
    }

    #[cfg(unix)]
    #[test]
    fn shutting_down_stops_every_agent_the_source_started() {
        let pids = fake_dir("shutdown").join("pids");
        let _ = fs::remove_file(&pids);
        let project = fake_claude_doing(
            "shutdown",
            &format!("echo $$ >> '{}'\ncat > /dev/null\n", pids.display()),
        );
        let mut source = fake_source();
        source.start_agent(SessionId(1), &project).unwrap();
        source.start_agent(SessionId(2), &project).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while fs::read_to_string(&pids)
            .map(|s| s.lines().count())
            .unwrap_or(0)
            < 2
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        let pids = fs::read_to_string(&pids).unwrap();
        let pids: Vec<&str> = pids.lines().collect();
        assert_eq!(pids.len(), 2);
        assert!(pids.iter().all(|pid| alive(pid)));

        source.shutdown();

        assert!(
            pids.iter().all(|pid| !alive(pid)),
            "an Agent survived shutdown"
        );
        assert!(source.poll_updates(Duration::ZERO).is_empty());
    }

    #[test]
    fn an_agent_cannot_start_in_a_project_that_no_longer_exists_on_disk() {
        let project = Project {
            id: ProjectId(0),
            name: "gone".into(),
            path: "/nonexistent/lodestar/project".into(),
        };
        let mut source = ClaudeSource::new("claude".into());

        let started = source.start_agent(SessionId(3), &project);

        assert_eq!(
            started.unwrap_err(),
            "The Project /nonexistent/lodestar/project no longer exists on disk."
        );
        assert!(source.poll_updates(Duration::ZERO).is_empty());
    }

    #[test]
    fn a_missing_binary_is_reported_when_the_agent_starts() {
        let mut source = ClaudeSource::new("lodestar-no-such-claude-binary".into());

        let started = source.start_agent(SessionId(3), &project_in(std::env::temp_dir()));

        assert_eq!(
            started.unwrap_err(),
            "Couldn't start lodestar-no-such-claude-binary: not found on PATH. \
             Install Claude Code or set LODESTAR_CLAUDE_BIN."
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_that_cannot_run_is_reported_without_the_install_hint() {
        let mut source = ClaudeSource::new(RECORDING.into());

        let started = source.start_agent(SessionId(3), &project_in(std::env::temp_dir()));

        assert_eq!(
            started.unwrap_err(),
            "Couldn't start text-only.jsonl: Permission denied (os error 13)."
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_message_to_an_agent_that_already_exited_ends_the_session_with_one_error() {
        let gone = fake_dir("gone-before-send").join("gone");
        let _ = fs::remove_file(&gone);
        let project = fake_claude_doing(
            "gone-before-send",
            &format!("touch '{}'\nexit 1\n", gone.display()),
        );
        let mut source = fake_source();
        source.start_agent(SessionId(3), &project).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !gone.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(200));

        source.send_message(SessionId(3), "Anyone there?");
        let updates = poll_until_done(&mut source);

        assert_eq!(
            updates,
            [SessionUpdate::Ended {
                session: SessionId(3),
                error: "claude exited (code 1).".into()
            }]
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_agent_whose_child_keeps_its_output_open_still_ends_the_session_when_it_exits() {
        let holder = fake_dir("holder").join("pid");
        let project = fake_claude_doing(
            "holder",
            &format!("sleep 30 &\necho $! > '{}'\nexit 2\n", holder.display()),
        );
        let mut source = fake_source();

        source.start_agent(SessionId(3), &project).unwrap();
        let updates = poll_until_done(&mut source);

        if let Ok(pid) = fs::read_to_string(&holder) {
            let _ = std::process::Command::new("kill").arg(pid.trim()).status();
        }
        assert_eq!(
            updates,
            [SessionUpdate::Ended {
                session: SessionId(3),
                error: "claude exited (code 2).".into()
            }]
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_agent_is_asked_for_partial_messages_and_its_turn_reports_progress_between_items() {
        const THINKING_TOOL: &str = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/stream-json/thinking-tool.jsonl"
        );
        let args = fake_dir("progress").join("args");
        let project = fake_claude_doing(
            "progress",
            &format!(
                "printf '%s\\n' \"$@\" > '{}'\ncat '{THINKING_TOOL}'\ncat > /dev/null\n",
                args.display()
            ),
        );
        let mut source = fake_source();

        source.start_agent(SessionId(3), &project).unwrap();
        let updates = poll_until_waiting(&mut source);

        let args = fs::read_to_string(&args).unwrap();
        assert!(
            args.lines().any(|arg| arg == "--include-partial-messages"),
            "{args}"
        );
        let kinds: Vec<String> = updates
            .iter()
            .map(|update| match update {
                SessionUpdate::Status { status, .. } => format!("{status:?}"),
                SessionUpdate::TranscriptItem { item, .. } => match item {
                    TranscriptItem::ToolCall(call) => format!("call {}", call.name),
                    TranscriptItem::Assistant(_) => "text".into(),
                    other => format!("{other:?}"),
                },
                SessionUpdate::ToolResult { .. } => "result".into(),
                SessionUpdate::Progress {
                    phase,
                    output_tokens,
                    ..
                } => format!("{} {output_tokens}", phase.label()),
                SessionUpdate::Ended { .. } => "ended".into(),
                SessionUpdate::Suggestion { text, .. } => format!("suggest {text}"),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "Running",
                "Working 7",
                "Thinking 7",
                "Running Bash 7",
                "call Bash",
                "Running Bash 113",
                "result",
                "Working 113",
                "Working 137",
                "Writing 137",
                "text",
                "WaitingForInput",
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_agent_is_asked_for_prompt_suggestions_and_its_suggestion_follows_the_turn() {
        const PROMPT_SUGGESTION: &str = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/stream-json/prompt-suggestion.jsonl"
        );
        let args = fake_dir("suggest").join("args");
        let project = fake_claude_doing(
            "suggest",
            &format!(
                "printf '%s\\n' \"$@\" > '{}'\ncat '{PROMPT_SUGGESTION}'\ncat > /dev/null\n",
                args.display()
            ),
        );
        let mut source = fake_source();

        source.start_agent(SessionId(3), &project).unwrap();
        let updates = poll_until(&mut source, |all| {
            all.iter()
                .any(|update| matches!(update, SessionUpdate::Suggestion { .. }))
        });

        let args = fs::read_to_string(&args).unwrap();
        let args: Vec<&str> = args.lines().collect();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--prompt-suggestions", "true"]),
            "{args:?}"
        );
        assert_eq!(
            updates.last(),
            Some(&SessionUpdate::Suggestion {
                session: SessionId(3),
                text: "run cargo test".into()
            })
        );
    }
}
