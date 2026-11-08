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
    /// The sessions no pane shows, newest first, and the recent folders, keeping only those
    /// whose title or path contains `filter` (ignoring case).
    pub fn new(store: &SessionStore, model: &WindowModel, filter: &str) -> Self {
        let filter = filter.to_lowercase();
        let matches = |text: &str| text.to_lowercase().contains(&filter);
        let all: Vec<SessionId> = store
            .sessions_newest_first()
            .iter()
            .filter(|session| matches(&session.title))
            .map(|session| session.id)
            .collect();
        Self {
            sessions: model.not_shown(&all),
            folders: store
                .recent_folders()
                .iter()
                .filter(|folder| matches(&folder.to_string_lossy()))
                .cloned()
                .collect(),
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

/// What one empty pane's picker remembers: the typed filter and the selected row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PickerState {
    filter: String,
    selected: usize,
}

impl PickerState {
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// The index of the selected row in [`PickerList::entries`].
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Append `c` to the filter; the selection returns to the first row.
    pub fn type_char(&mut self, c: char) {
        self.filter.push(c);
        self.selected = 0;
    }

    /// Delete the last filter character; the selection returns to the first row.
    pub fn backspace(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }

    /// Select the next of `len` rows, stopping at the last.
    pub fn move_down(&mut self, len: usize) {
        self.selected = (self.selected + 1).min(len.saturating_sub(1));
    }

    /// Select the previous of `len` rows, stopping at the first.
    pub fn move_up(&mut self, len: usize) {
        self.selected = self.selected.saturating_sub(1).min(len.saturating_sub(1));
    }
}
