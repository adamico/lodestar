//! The empty-pane picker: open a session no pane shows, or start a new one in a folder.

use std::path::PathBuf;

use crate::session::{SessionId, SessionStore};
use crate::window::WindowModel;

/// One row of the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerEntry {
    /// Open this session in the pane.
    Session(SessionId),
    /// Start a new session in the Project at this folder.
    Folder(PathBuf),
    /// Pick a folder with the system dialog, then start a new session there.
    Browse,
}

/// What an empty pane offers, in display order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PickerList {
    sessions: Vec<SessionId>,
    folders: Vec<PathBuf>,
}

impl PickerList {
    /// The sessions no pane shows, newest first, and the recent folders.
    pub fn new(store: &SessionStore, model: &WindowModel, _filter: &str) -> Self {
        let all: Vec<SessionId> = store
            .sessions_newest_first()
            .iter()
            .map(|session| session.id)
            .collect();
        Self {
            sessions: model.not_shown(&all),
            folders: store.recent_folders().to_vec(),
        }
    }

    /// The "Sessions" section.
    pub fn sessions(&self) -> &[SessionId] {
        &self.sessions
    }

    /// The folders of the "New session" section, before "Browse…".
    pub fn folders(&self) -> &[PathBuf] {
        &self.folders
    }

    /// Every row in order: sessions, then folders, then Browse….
    pub fn entries(&self) -> Vec<PickerEntry> {
        self.sessions
            .iter()
            .map(|id| PickerEntry::Session(*id))
            .chain(self.folders.iter().cloned().map(PickerEntry::Folder))
            .chain(std::iter::once(PickerEntry::Browse))
            .collect()
    }
}
