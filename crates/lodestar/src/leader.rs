//! The `ctrl+space` leader: a mode flag on the window plus a key context, not a GPUI key
//! sequence, so it never times out and later sticky modes (resize) can build on it.
//!
//! Adding a leader key means one row in [`LeaderCommand::ALL`], which drives both the hint
//! popup and [`LeaderCommand::from_keystroke`], and one arm where the window runs the command.

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

/// The vi directions: `h`/`j`/`k`/`l` as left, down, up and right.
const HJKL: &[(&str, Direction)] = &[
    ("h", Direction::Left),
    ("j", Direction::Down),
    ("k", Direction::Up),
    ("l", Direction::Right),
];

/// The direction `key` stands for, if it is one of `h`/`j`/`k`/`l`.
fn direction_of(key: &str) -> Option<Direction> {
    HJKL.iter()
        .find(|(k, _)| *k == key)
        .map(|(_, direction)| *direction)
}

/// The direction a key resizes toward in [`Mode::Resize`], if it is one of `h`/`j`/`k`/`l`.
pub fn resize_direction(keystroke: &Keystroke) -> Option<Direction> {
    if keystroke.modifiers.modified() {
        return None;
    }
    direction_of(&keystroke.key)
}

/// The key a hint-popup row names, as GPUI reports it: a single capital letter stands for
/// the lower-case key with shift held, anything else is the key itself, unshifted.
fn key_spec(hint_key: &str) -> (String, bool) {
    let mut chars = hint_key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_uppercase() => (c.to_ascii_lowercase().to_string(), true),
        _ => (hint_key.to_string(), false),
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
    /// Every command, in hint-popup order, with the key that triggers it. The key is spelled
    /// as the hint popup shows it (see [`key_spec`]); this table is the one place a leader key
    /// is defined.
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
        Self::ALL
            .iter()
            .find(|(hint_key, _, _)| key_spec(hint_key) == (keystroke.key.clone(), m.shift))
            .map(|(_, _, command)| *command)
    }
}

#[cfg(test)]
mod tests {
    // Named imports: `gpui_kit::*` would also bring in GPUI's own `#[test]` macro.
    use gpui_kit::Keystroke;
    use lodestar_core::window::Direction;

    use super::{LeaderCommand, direction_of, key_spec, resize_direction};

    fn keystroke(spec: &'static str) -> Keystroke {
        Keystroke::parse(spec).expect("a valid keystroke")
    }

    #[test]
    fn every_hint_row_maps_its_key_to_its_command() {
        let expected: Vec<(&str, LeaderCommand)> = vec![
            ("enter", LeaderCommand::SplitAuto),
            ("s", LeaderCommand::SplitRight),
            ("v", LeaderCommand::SplitDown),
            ("x", LeaderCommand::ClosePane),
            ("h", LeaderCommand::Focus(Direction::Left)),
            ("j", LeaderCommand::Focus(Direction::Down)),
            ("k", LeaderCommand::Focus(Direction::Up)),
            ("l", LeaderCommand::Focus(Direction::Right)),
            ("shift-h", LeaderCommand::Swap(Direction::Left)),
            ("shift-j", LeaderCommand::Swap(Direction::Down)),
            ("shift-k", LeaderCommand::Swap(Direction::Up)),
            ("shift-l", LeaderCommand::Swap(Direction::Right)),
            ("b", LeaderCommand::FocusSidebar),
            ("shift-b", LeaderCommand::ToggleSidebar),
            ("r", LeaderCommand::Resize),
        ];
        assert_eq!(expected.len(), LeaderCommand::ALL.len());

        for (spec, command) in expected {
            assert_eq!(
                LeaderCommand::from_keystroke(&keystroke(spec)),
                Some(command),
                "{spec}"
            );
        }
    }

    #[test]
    fn keys_with_other_modifiers_or_unbound_keys_are_not_commands() {
        for spec in ["ctrl-s", "alt-h", "cmd-x", "shift-s", "q", "escape"] {
            assert_eq!(
                LeaderCommand::from_keystroke(&keystroke(spec)),
                None,
                "{spec}"
            );
        }
    }

    #[test]
    fn resize_mode_reads_hjkl_without_modifiers_only() {
        assert_eq!(resize_direction(&keystroke("h")), Some(Direction::Left));
        assert_eq!(resize_direction(&keystroke("j")), Some(Direction::Down));
        assert_eq!(resize_direction(&keystroke("k")), Some(Direction::Up));
        assert_eq!(resize_direction(&keystroke("l")), Some(Direction::Right));
        assert_eq!(resize_direction(&keystroke("shift-h")), None);
        assert_eq!(resize_direction(&keystroke("x")), None);
    }

    #[test]
    fn the_focus_and_swap_rows_agree_with_the_hjkl_map() {
        for (hint_key, _, command) in LeaderCommand::ALL {
            let (key, _) = key_spec(hint_key);
            match command {
                LeaderCommand::Focus(direction) | LeaderCommand::Swap(direction) => {
                    assert_eq!(direction_of(&key), Some(*direction), "{hint_key}");
                }
                _ => assert_eq!(direction_of(&key), None, "{hint_key}"),
            }
        }
    }
}
