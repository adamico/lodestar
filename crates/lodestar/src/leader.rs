//! The `ctrl+space` leader: a mode flag on the window plus a key context, not a GPUI key
//! sequence, so it never times out and later sticky modes (resize) can build on it.
//!
//! Adding a leader key means one match arm in [`LeaderCommand::from_keystroke`], one entry in
//! [`LeaderCommand::ALL`] for the hint popup, and one arm where the window runs the command.

use gpui_kit::*;

gpui_kit::actions!(lodestar, [Leader, CancelLeader]);

/// The key context of the window's root element.
pub const WINDOW_CONTEXT: &str = "MainWindow";
/// Set on the root element's key context while the leader is pending.
pub const LEADER_FLAG: &str = "leader";
/// The key context of bindings that only apply while the leader is pending.
const LEADER_CONTEXT: &str = "MainWindow && leader";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-space", Leader, Some(WINDOW_CONTEXT)),
        KeyBinding::new("escape", CancelLeader, Some(LEADER_CONTEXT)),
    ]);
}

/// What the window is waiting for from the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Keys go to whatever has focus; `ctrl+space` arms the leader.
    #[default]
    Normal,
    /// The leader was pressed; the next key is a [`LeaderCommand`] or `esc`.
    Leader,
}

/// A command reached through the leader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderCommand {
    SplitAuto,
    SplitRight,
    SplitDown,
    ClosePane,
    FocusSidebar,
    ToggleSidebar,
}

impl LeaderCommand {
    /// Every command, in hint-popup order, with the key that triggers it.
    pub const ALL: &[(&str, &str, LeaderCommand)] = &[
        ("enter", "split", LeaderCommand::SplitAuto),
        ("s", "split right", LeaderCommand::SplitRight),
        ("v", "split down", LeaderCommand::SplitDown),
        ("x", "close pane", LeaderCommand::ClosePane),
        ("b", "focus sidebar", LeaderCommand::FocusSidebar),
        ("B", "toggle sidebar", LeaderCommand::ToggleSidebar),
    ];

    /// The command for a keystroke pressed after the leader, if it is one.
    pub fn from_keystroke(keystroke: &Keystroke) -> Option<Self> {
        let m = keystroke.modifiers;
        if m.control || m.alt || m.platform || m.function {
            return None;
        }
        match (keystroke.key.as_str(), m.shift) {
            ("enter", false) => Some(Self::SplitAuto),
            ("s", false) => Some(Self::SplitRight),
            ("v", false) => Some(Self::SplitDown),
            ("x", false) => Some(Self::ClosePane),
            ("b", false) => Some(Self::FocusSidebar),
            ("b", true) => Some(Self::ToggleSidebar),
            _ => None,
        }
    }
}
