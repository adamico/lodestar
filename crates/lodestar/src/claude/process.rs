//! One headless Claude Code process: spawned in its Project folder, written to as JSON
//! lines, read by two threads (stdout and stderr) that hand every line down a channel.

use std::ffi::OsStr;
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use lodestar_core::session::SessionId;

/// The flags every Agent starts with. `-p` with stream-json in and out keeps the process
/// alive for as long as stdin is open; `--verbose` makes it emit every event; the permission
/// mode is ADR 0003's.
const ARGS: [&str; 8] = [
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--permission-mode",
    "acceptEdits",
];

/// How many characters of stderr an [`AgentExit`] keeps: the end, where the reason is.
const STDERR_TAIL_CHARS: usize = 2000;

/// How long an exited process's pipes get to close before its exit is reported anyway. A
/// child it left behind can hold them open for as long as it likes.
const EXIT_GRACE: Duration = Duration::from_secs(2);

/// What [`Agent::drain`] hands out: a stdout line, or the news that the process is gone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    Line(String),
    /// The process exited or was killed. Nothing more will come from it.
    Exited(AgentExit),
}

/// How a process ended. On Unix a process that was killed has a signal and no code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentExit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    /// The tail of what the process wrote to stderr, trimmed. Empty when it wrote nothing.
    pub stderr: String,
}

impl AgentExit {
    fn new(status: ExitStatus, stderr: &str) -> Self {
        #[cfg(unix)]
        let signal = std::os::unix::process::ExitStatusExt::signal(&status);
        #[cfg(not(unix))]
        let signal = None;
        Self {
            code: status.code(),
            signal,
            stderr: stderr.trim().to_owned(),
        }
    }

    /// Words for the user about how `program` ended and, when it said anything on stderr,
    /// what.
    pub fn describe(&self, program: &str) -> String {
        let mut message = match (self.code, self.signal) {
            (Some(code), _) => format!("{program} exited (code {code})."),
            (None, Some(signal)) => format!("{program} was killed (signal {signal})."),
            (None, None) => format!("{program} exited."),
        };
        if !self.stderr.is_empty() {
            message.push('\n');
            message.push_str(&self.stderr);
        }
        message
    }
}

/// What the reader threads send down the one channel.
enum PipeMessage {
    Stdout(String),
    Stderr(String),
    StdoutClosed,
    StderrClosed,
}

/// A running Claude Code process and the threads reading it. Killed when dropped.
pub struct Agent {
    child: Child,
    /// Taken (closed) to tell Claude Code there is nothing more to say.
    stdin: Option<ChildStdin>,
    pipes: Receiver<PipeMessage>,
    /// The last [`STDERR_TAIL_CHARS`] of stderr so far.
    stderr: String,
    stdout_closed: bool,
    stderr_closed: bool,
    /// The status once the process was reaped, and when that was.
    exit_status: Option<(ExitStatus, Instant)>,
    /// Whether [`AgentEvent::Exited`] was handed out already; it is handed out once.
    exit_reported: bool,
}

impl Agent {
    /// Start `program` (normally `claude`) in `project_path`.
    pub fn spawn(program: &OsStr, project_path: &Path, session: SessionId) -> io::Result<Self> {
        let mut child = Command::new(program)
            .args(ARGS)
            .current_dir(project_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // CLI-level errors (bad flags, bad stdin) land here and end up in the Transcript.
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");
        let (tx, pipes) = mpsc::channel();
        let tx_err = tx.clone();
        let readers = std::thread::Builder::new()
            .name(format!("claude-stdout-{}", session.0))
            .spawn(move || read_lines(stdout, tx, PipeMessage::Stdout, PipeMessage::StdoutClosed))
            .and_then(|_| {
                std::thread::Builder::new()
                    .name(format!("claude-stderr-{}", session.0))
                    .spawn(move || {
                        read_lines(
                            stderr,
                            tx_err,
                            PipeMessage::Stderr,
                            PipeMessage::StderrClosed,
                        )
                    })
            });
        if let Err(error) = readers {
            // Nobody would read the process, so it must not outlive this call.
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(Self {
            child,
            stdin,
            pipes,
            stderr: String::new(),
            stdout_closed: false,
            stderr_closed: false,
            exit_status: None,
            exit_reported: false,
        })
    }

    /// Send the user's `text` as one message. Only serde writes to stdin: a line that is not
    /// JSON kills Claude Code.
    pub fn send(&mut self, text: &str) -> io::Result<()> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "stdin closed"));
        };
        let message = serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": text },
        });
        let mut line = message.to_string();
        line.push('\n');
        stdin.write_all(line.as_bytes())?;
        stdin.flush()
    }

    /// Every event that arrived since the last call, in order. The process's exit is the
    /// last event, reported once, after it was reaped and its stdout and stderr closed, so
    /// every line it wrote comes first. Should a child it left behind keep them open, the
    /// exit is reported [`EXIT_GRACE`] after the reaping all the same.
    pub fn drain(&mut self) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        loop {
            match self.pipes.try_recv() {
                Ok(PipeMessage::Stdout(line)) => events.push(AgentEvent::Line(line)),
                Ok(PipeMessage::Stderr(line)) => self.keep_stderr(&line),
                Ok(PipeMessage::StdoutClosed) => self.stdout_closed = true,
                Ok(PipeMessage::StderrClosed) => self.stderr_closed = true,
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        if self.exit_reported {
            return events;
        }
        if self.exit_status.is_none()
            && let Ok(Some(status)) = self.child.try_wait()
        {
            self.exit_status = Some((status, Instant::now()));
        }
        if let Some((status, reaped_at)) = self.exit_status
            && ((self.stdout_closed && self.stderr_closed) || reaped_at.elapsed() >= EXIT_GRACE)
        {
            self.exit_reported = true;
            events.push(AgentEvent::Exited(AgentExit::new(status, &self.stderr)));
        }
        events
    }

    fn keep_stderr(&mut self, line: &str) {
        self.stderr.push_str(line);
        self.stderr.push('\n');
        let over = self
            .stderr
            .chars()
            .count()
            .saturating_sub(STDERR_TAIL_CHARS);
        if over > 0 {
            let cut = self
                .stderr
                .char_indices()
                .nth(over)
                .map(|(ix, _)| ix)
                .unwrap_or(self.stderr.len());
            self.stderr.drain(..cut);
        }
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A reader thread: every line of `pipe`, wrapped by `wrap`, then `closed`. Stops early once
/// nobody listens any more.
fn read_lines(
    pipe: impl io::Read,
    tx: Sender<PipeMessage>,
    wrap: impl Fn(String) -> PipeMessage,
    closed: PipeMessage,
) {
    for line in BufReader::new(pipe).lines() {
        let Ok(line) = line else { break };
        if tx.send(wrap(line)).is_err() {
            return;
        }
    }
    let _ = tx.send(closed);
}
