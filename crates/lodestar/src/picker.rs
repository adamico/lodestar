//! The empty-pane picker view.
//!
//! It renders a pane's [`PickerList`] and turns keys and clicks into commands: open a session
//! in the pane, or start a new one in a folder. Which rows exist and how the filter and selection
//! behave are rules of `lodestar_core::picker`; nothing here decides that.

use std::path::PathBuf;

use gpui_kit::component::{ActiveTheme, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use lodestar_core::picker::{PickerEntry, PickerList, PickerState};
use lodestar_core::session::{Project, ProjectId, SessionId, SessionStatus, SessionUpdate};
use lodestar_core::window::PaneId;

use crate::main_window::{MainWindow, pane_element_id, status_color, status_dot};

const BROWSE_LABEL: &str = "Browse\u{2026}";
const FILTER_PLACEHOLDER: &str = "Type to filter";

impl MainWindow {
    fn picker_list(&self, pane: PaneId) -> PickerList {
        let filter = self
            .pickers
            .get(&pane)
            .map(PickerState::filter)
            .unwrap_or_default();
        PickerList::new(&self.sessions, &self.window_model, filter)
    }

    /// The body of an empty pane: a filter line, the unshown sessions, then "New session".
    pub(crate) fn render_picker(&self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let state = self.pickers.get(&pane).cloned().unwrap_or_default();
        let list = self.picker_list(pane);
        let entries = list.entries();
        let session_count = list.sessions().len();

        let filter_line = h_flex()
            .px_2()
            .py_1()
            .gap_1()
            .border_b_1()
            .border_color(theme.border)
            .child(div().text_color(theme.muted_foreground).child(">"))
            .when(state.filter().is_empty(), |this| {
                this.child(
                    div()
                        .text_color(theme.muted_foreground)
                        .child(FILTER_PLACEHOLDER),
                )
            })
            .when(!state.filter().is_empty(), |this| {
                this.child(state.filter().to_string())
            });

        let mut rows = v_flex().flex_1().overflow_hidden().py_1();
        for (ix, entry) in entries.into_iter().enumerate() {
            if ix == 0 && session_count > 0 {
                rows = rows.child(section_label("Sessions", cx));
            }
            if ix == session_count {
                rows = rows.child(section_label("New session", cx));
            }
            let selected = ix == state.selected();
            rows = rows.child(self.render_picker_row(pane, ix, entry, selected, cx));
        }

        v_flex()
            .flex_1()
            .overflow_hidden()
            .text_sm()
            .child(filter_line)
            .child(rows)
    }

    fn render_picker_row(
        &self,
        pane: PaneId,
        ix: usize,
        entry: PickerEntry,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = cx.theme();
        let row = h_flex()
            .id(pane_element_id(&format!("picker-row-{ix}"), pane))
            .px_2()
            .py_1()
            .gap_2()
            .items_center()
            .cursor_pointer()
            .when(selected, |this| this.bg(theme.list_active))
            .hover(|this| this.bg(theme.list_hover));
        let row = match &entry {
            PickerEntry::Session(id) => match self.sessions.session(*id) {
                Some(session) => {
                    let project = self
                        .sessions
                        .project(session.project)
                        .map(|project| project.name.clone())
                        .unwrap_or_default();
                    row.child(status_dot(status_color(session.status, cx)))
                        .child(session.title.clone())
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(project),
                        )
                }
                None => row,
            },
            PickerEntry::Folder(path) => row.child(Project::name_for(path)).child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(path.to_string_lossy().into_owned()),
            ),
            PickerEntry::Browse => row.child(BROWSE_LABEL),
        };
        row.on_click(cx.listener(move |this, _, _, cx| {
            this.choose(pane, entry.clone(), cx);
            cx.notify();
        }))
    }

    /// Keys typed while the focused pane is empty drive that pane's picker.
    ///
    /// Arrows move; `j`/`k` move too while nothing is typed, and are letters once a filter has
    /// begun. Returns whether the key was used.
    pub(crate) fn picker_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let pane = self.window_model.focused_pane();
        if self.window_model.session_in(pane).is_some() {
            return false;
        }
        let keystroke = &event.keystroke;
        if keystroke.modifiers.control || keystroke.modifiers.alt || keystroke.modifiers.platform {
            return false;
        }
        let entries = self.picker_list(pane).entries();
        let state = self.pickers.entry(pane).or_default();
        let filter_empty = state.filter().is_empty();
        match keystroke.key.as_str() {
            "down" => state.move_down(entries.len()),
            "up" => state.move_up(entries.len()),
            "j" if filter_empty => state.move_down(entries.len()),
            "k" if filter_empty => state.move_up(entries.len()),
            "backspace" => state.backspace(),
            "enter" => {
                if let Some(entry) = entries.get(state.selected()) {
                    self.choose(pane, entry.clone(), cx);
                }
            }
            _ => {
                let Some(c) = typed_char(keystroke) else {
                    return false;
                };
                state.type_char(c);
            }
        }
        true
    }

    fn choose(&mut self, pane: PaneId, entry: PickerEntry, cx: &mut Context<Self>) {
        match entry {
            PickerEntry::Session(id) => {
                self.window_model.open_session(id, pane);
                self.pickers.remove(&pane);
            }
            PickerEntry::Folder(path) => self.start_session_in(path, pane),
            PickerEntry::Browse => self.browse(pane, cx),
        }
    }

    /// Start a new session in the Project at `path`, show it in `pane`, and start its Agent.
    /// The Agent gets nothing until the user sends the first message from the input box; the
    /// session waits for input until then. A session whose Agent could not start is done at
    /// once.
    fn start_session_in(&mut self, path: PathBuf, pane: PaneId) {
        let project = self.sessions.find_or_create_project(path.clone());
        self.sessions.add_recent_folder(path);
        let session = self.sessions.create_session(project);
        self.window_model.open_session(session, pane);
        self.pickers.remove(&pane);
        self.start_agent(session, project);
    }

    fn start_agent(&mut self, session: SessionId, project: ProjectId) {
        let Some(project) = self.sessions.project(project).cloned() else {
            return;
        };
        if let Err(message) = self.source.start_agent(session, &project) {
            // The error-handling slice puts `message` in the Transcript.
            eprintln!("lodestar: {message}");
            self.sessions.apply(SessionUpdate::Status {
                session,
                status: SessionStatus::Done,
            });
        }
    }

    /// Ask the system for a folder, then start a session there.
    fn browse(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Start a session in".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = chosen.await
                && let Some(path) = paths.into_iter().next()
            {
                this.update(cx, |this, cx| {
                    this.start_session_in(path, pane);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}

/// The character a keystroke types, if it types one.
fn typed_char(keystroke: &Keystroke) -> Option<char> {
    let text = keystroke.key_char.as_deref().unwrap_or(&keystroke.key);
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_control() => Some(c),
        _ => None,
    }
}

fn section_label(label: &'static str, cx: &App) -> Div {
    div()
        .px_2()
        .pt_2()
        .pb_1()
        .text_xs()
        .font_semibold()
        .text_color(cx.theme().muted_foreground)
        .child(label)
}
