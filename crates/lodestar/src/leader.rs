//! The `ctrl+space` leader: a mode flag on the window plus a key context, not a GPUI key
//! sequence, so it never times out and later sticky modes (resize) can build on it.
//!
//! Adding a leader key means one match arm in [`LeaderCommand::from_keystroke`], one entry in
//! [`LeaderCommand::ALL`] for the hint popup, and one arm where the window runs the command.

use gpui_kit::*;
use lodestar_core::window::Direction;

gpui_kit::actions!(lodestar, [Leader, CancelLeader]);

/// The key context of the window's root element.
pub const WINDOW_CONTEXT: &str = "MainWindow";
/// Set on the root element's key context while the leader is pending.
pub const LEADER_FLAG: &str = "leader";
/// Set on the root element's key context while resize mode is on.
pub const RESIZE_FLAG: &str = "resize";
/// The key context of bindings that only apply while the leader is pending.
const LEADER_CONTEXT: &str = "MainWindow && leader";
/// The key context of bindings that only apply in resize mode.
const RESIZE_CONTEXT: &str = "MainWindow && resize";
/// How far one `h`/`j`/`k`/`l` in resize mode moves a divider, in logical pixels.
pub const RESIZE_STEP: f32 = 40.0;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-space", Leader, Some(WINDOW_CONTEXT)),
        KeyBinding::new("escape", CancelLeader, Some(LEADER_CONTEXT)),
        KeyBinding::new("escape", CancelLeader, Some(RESIZE_CONTEXT)),
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
    /// Sticky resize mode: `h`/`j`/`k`/`l` resize the focused pane until `esc`.
    Resize,
}

/// What the hint popup shows for a mode.
pub struct Hints {
    pub title: &'static str,
    pub rows: Vec<(&'static str, &'static str)>,
    pub footer: &'static str,
}

const RESIZE_HINTS: &[(&str, &str)] = &[
    ("h", "grow left"),
    ("j", "grow down"),
    ("k", "grow up"),
    ("l", "grow right"),
];

impl Mode {
    /// The hint popup for this mode, if it shows one.
    pub fn hints(self) -> Option<Hints> {
        match self {
            Mode::Normal => None,
            Mode::Leader => Some(Hints {
                title: "ctrl+space, then",
                rows: LeaderCommand::ALL
                    .iter()
                    .map(|(key, label, _)| (*key, *label))
                    .collect(),
                footer: "esc to cancel",
            }),
            Mode::Resize => Some(Hints {
                title: "resize",
                rows: RESIZE_HINTS.to_vec(),
                footer: "esc to finish",
            }),
        }
    }
}

/// The direction a key resizes toward in [`Mode::Resize`], if it is one of `h`/`j`/`k`/`l`.
pub fn resize_direction(keystroke: &Keystroke) -> Option<Direction> {
    if keystroke.modifiers.modified() {
        return None;
    }
    match keystroke.key.as_str() {
        "h" => Some(Direction::Left),
        "j" => Some(Direction::Down),
        "k" => Some(Direction::Up),
        "l" => Some(Direction::Right),
        _ => None,
    }
}

/// A command reached through the leader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderCommand {
    SplitAuto,
    SplitRight,
    SplitDown,
    ClosePane,
    /// Leader + `h`/`j`/`k`/`l`.
    Focus(Direction),
    /// Leader + `H`/`J`/`K`/`L`.
    Swap(Direction),
    FocusSidebar,
    ToggleSidebar,
    Resize,
}

impl LeaderCommand {
    /// Every command, in hint-popup order, with the key that triggers it.
    pub const ALL: &[(&str, &str, LeaderCommand)] = &[
        ("enter", "split", LeaderCommand::SplitAuto),
        ("s", "split right", LeaderCommand::SplitRight),
        ("v", "split down", LeaderCommand::SplitDown),
        ("x", "close pane", LeaderCommand::ClosePane),
        ("h", "focus left", LeaderCommand::Focus(Direction::Left)),
        ("j", "focus down", LeaderCommand::Focus(Direction::Down)),
        ("k", "focus up", LeaderCommand::Focus(Direction::Up)),
        ("l", "focus right", LeaderCommand::Focus(Direction::Right)),
        ("H", "swap left", LeaderCommand::Swap(Direction::Left)),
        ("J", "swap down", LeaderCommand::Swap(Direction::Down)),
        ("K", "swap up", LeaderCommand::Swap(Direction::Up)),
        ("L", "swap right", LeaderCommand::Swap(Direction::Right)),
        ("b", "focus sidebar", LeaderCommand::FocusSidebar),
        ("B", "toggle sidebar", LeaderCommand::ToggleSidebar),
        ("r", "resize mode", LeaderCommand::Resize),
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
            ("h", false) => Some(Self::Focus(Direction::Left)),
            ("j", false) => Some(Self::Focus(Direction::Down)),
            ("k", false) => Some(Self::Focus(Direction::Up)),
            ("l", false) => Some(Self::Focus(Direction::Right)),
            ("h", true) => Some(Self::Swap(Direction::Left)),
            ("j", true) => Some(Self::Swap(Direction::Down)),
            ("k", true) => Some(Self::Swap(Direction::Up)),
            ("l", true) => Some(Self::Swap(Direction::Right)),
            ("b", false) => Some(Self::FocusSidebar),
            ("b", true) => Some(Self::ToggleSidebar),
            ("r", false) => Some(Self::Resize),
            _ => None,
        }
    }
}
