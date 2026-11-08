//! One headless Claude Code process: spawned in its Project folder, written to as JSON
//! lines, read by a thread that hands every stdout line down a channel.

use std::ffi::OsStr;
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

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

/// What the reader thread sends: a stdout line, or the news that there will be no more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    Line(String),
    /// Stdout closed: the process exited or was killed. Left for the error-handling slice.
    Exited,
}

/// A running Claude Code process and the thread reading it. Killed when dropped.
pub struct Agent {
    child: Child,
    /// Taken (closed) to tell Claude Code there is nothing more to say.
    stdin: Option<ChildStdin>,
    events: Receiver<AgentEvent>,
}

impl Agent {
    /// Start `program` (normally `claude`) in `project_path`.
    pub fn spawn(program: &OsStr, project_path: &Path, session: SessionId) -> io::Result<Self> {
        let mut child = Command::new(program)
            .args(ARGS)
            .current_dir(project_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // CLI-level errors (bad flags, bad stdin) land here; leave them visible until the
            // error-handling slice reads them into the Transcript.
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout was piped");
        let (tx, events) = mpsc::channel();
        std::thread::Builder::new()
            .name(format!("claude-stdout-{}", session.0))
            .spawn(move || read_lines(stdout, tx))?;
        Ok(Self {
            child,
            stdin,
            events,
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

    /// Every event that arrived since the last call, in order.
    pub fn drain(&mut self) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => return events,
            }
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

/// The reader thread: one `Line` per stdout line, then `Exited`. Stops early once nobody
/// listens any more.
fn read_lines(stdout: impl io::Read, tx: Sender<AgentEvent>) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        if tx.send(AgentEvent::Line(line)).is_err() {
            return;
        }
    }
    let _ = tx.send(AgentEvent::Exited);
}
