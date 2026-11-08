//! What a running turn is doing right now, and how far along it is: the working indicator.

use std::time::Duration;

/// The dot between the parts of the indicator line.
const SEPARATOR: &str = " \u{b7} ";

/// What the Agent is busy with in its current turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Working,
    Thinking,
    Writing,
    /// Calling the named tool, or waiting for its result.
    Tool(String),
}

impl Phase {
    /// The word the indicator shows for this phase.
    pub fn label(&self) -> String {
        match self {
            Self::Working => "Working".into(),
            Self::Thinking => "Thinking".into(),
            Self::Writing => "Writing".into(),
            Self::Tool(name) => match name.as_str() {
                "Bash" => "Running Bash".into(),
                "Read" => "Reading".into(),
                "Edit" | "MultiEdit" | "NotebookEdit" => "Editing".into(),
                "Write" => "Writing".into(),
                "Grep" | "Glob" => "Searching".into(),
                _ => format!("Using {name}"),
            },
        }
    }
}

/// How far the current turn has got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnProgress {
    /// When the turn began, on the clock the window polls its source with.
    pub started: Duration,
    pub phase: Phase,
    /// What the Agent has written so far this turn, across all its API messages.
    pub output_tokens: u64,
}

impl TurnProgress {
    /// The indicator's text at `now`: `55s · 1.5k tokens · Thinking`.
    /// The tokens are left out until there are some.
    pub fn line(&self, now: Duration) -> String {
        let mut parts = vec![format_elapsed(now.saturating_sub(self.started))];
        if self.output_tokens > 0 {
            parts.push(format_tokens(self.output_tokens));
        }
        parts.push(self.phase.label());
        parts.join(SEPARATOR)
    }
}

/// `55s` under a minute, `1m 05s` past one.
pub fn format_elapsed(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

/// `850 tokens` under a thousand, `1.5k tokens` from there, `2k tokens` when the tenth is 0.
pub fn format_tokens(tokens: u64) -> String {
    if tokens < 1000 {
        return format!("{tokens} tokens");
    }
    let thousands = format!("{:.1}", tokens as f64 / 1000.);
    let thousands = thousands.strip_suffix(".0").unwrap_or(&thousands);
    format!("{thousands}k tokens")
}
