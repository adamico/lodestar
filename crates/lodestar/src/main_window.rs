//! The window's root view: the session sidebar beside the pane area.
//!
//! It renders the window model and session store, and turns input into window-model commands.
//! Layout rules live in the window model, not here.

use std::collections::HashMap;

use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem};
use gpui_kit::component::{ActiveTheme, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use lodestar_core::geometry::{Rect, Size};
use lodestar_core::picker::PickerState;
use lodestar_core::session::{Session, SessionStatus, SessionStore};
use lodestar_core::window::{PaneId, WindowModel};

const SIDEBAR_WIDTH: Pixels = px(255.);
const PANE_HEADER_HEIGHT: Pixels = px(30.);
/// Lodestar's one accent colour: marks the focused pane.
const ACCENT: u32 = 0x3b82f6;
const UNFOCUSED_OPACITY: f32 = 0.6;
/// Marks a sidebar row whose session is shown in a pane.
const SHOWN_MARKER: &str = "\u{25A3}";

pub struct MainWindow {
    pub(crate) window_model: WindowModel,
    pub(crate) sessions: SessionStore,
    /// Each empty pane's picker: its filter and selection. Absent means untouched.
    pub(crate) pickers: HashMap<PaneId, PickerState>,
    /// Keys go to the window as a whole; the view routes them to the focused pane.
    pub(crate) focus_handle: FocusHandle,
}

impl MainWindow {
    pub fn new(sessions: SessionStore, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        Self {
            window_model: WindowModel::new(),
            sessions,
            pickers: HashMap::new(),
            focus_handle,
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.picker_key_down(event, cx) {
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let groups: Vec<SidebarGroup<SidebarMenu>> = self
            .sessions
            .projects()
            .iter()
            .map(|project| {
                let items: Vec<SidebarMenuItem> = self
                    .sessions
                    .sessions_in(project.id)
                    .into_iter()
                    .map(|session| self.render_sidebar_row(session, cx))
                    .collect();
                SidebarGroup::new(project.name.clone()).child(SidebarMenu::new().children(items))
            })
            .collect();

        Sidebar::new("sessions")
            .collapsible(false)
            .w(SIDEBAR_WIDTH)
            .header(div().px_2().text_sm().font_semibold().child("Sessions"))
            .children(groups)
    }

    /// One session in the sidebar: title, a marker if a pane shows it, and its status dot.
    /// Clicking it opens the session in the focused pane.
    fn render_sidebar_row(&self, session: &Session, cx: &mut Context<Self>) -> SidebarMenuItem {
        let id = session.id;
        let shown = self.window_model.pane_showing(id).is_some();
        let color = status_color(session.status, cx);
        SidebarMenuItem::new(session.title.clone())
            .active(shown)
            .suffix(move |_, _| {
                h_flex()
                    .gap_2()
                    .items_center()
                    .when(shown, |this| this.child(SHOWN_MARKER))
                    .child(status_dot(color))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                let pane = this.window_model.focused_pane();
                this.window_model.open_session(id, pane);
                cx.notify();
            }))
    }

    fn render_panes(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let area = Size::new(
            f32::from(viewport.width - SIDEBAR_WIDTH).max(0.),
            f32::from(viewport.height),
        );
        let layout = self.window_model.layout(area);
        let focused = self.window_model.focused_pane();

        div()
            .relative()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .children(
                layout
                    .dividers
                    .iter()
                    .map(|divider| place(div().bg(cx.theme().border), divider.rect)),
            )
            .children(layout.panes.iter().map(|pane| {
                place(
                    self.render_pane(pane.pane, pane.pane == focused, cx),
                    pane.rect,
                )
            }))
    }

    fn render_pane(&self, pane: PaneId, focused: bool, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let background = theme.background;
        let (border, header_bg, header_fg) = if focused {
            (rgb(ACCENT).into(), rgb(ACCENT).into(), gpui_kit::white())
        } else {
            (theme.border, theme.secondary, theme.muted_foreground)
        };
        let session = self
            .window_model
            .session_in(pane)
            .and_then(|id| self.sessions.session(id));
        let header = h_flex()
            .h(PANE_HEADER_HEIGHT)
            .flex_shrink_0()
            .px_2()
            .gap_2()
            .items_center()
            .bg(header_bg)
            .text_color(header_fg)
            .text_sm();
        let header = match session {
            Some(session) => {
                let project = self
                    .sessions
                    .project(session.project)
                    .map(|project| project.name.clone())
                    .unwrap_or_default();
                header
                    .child(status_dot(status_color(session.status, cx)))
                    .child(div().font_semibold().child(session.title.clone()))
                    .child(div().text_xs().opacity(0.8).child(project))
            }
            None => header.child("Empty"),
        };
        let body = match session {
            Some(session) => v_flex()
                .flex_1()
                .overflow_hidden()
                .p_2()
                .gap_2()
                .text_sm()
                .children(
                    session
                        .transcript
                        .items()
                        .iter()
                        .map(|item| div().child(item.clone())),
                ),
            None => self.render_picker(pane, cx),
        };

        v_flex()
            .border_1()
            .border_color(border)
            .bg(background)
            .when(!focused, |this| this.opacity(UNFOCUSED_OPACITY))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.window_model.focus(pane);
                    cx.notify();
                }),
            )
            .child(header)
            .child(body)
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_sidebar(cx))
            .child(self.render_panes(window, cx))
    }
}

pub(crate) fn status_color(status: SessionStatus, cx: &App) -> Hsla {
    match status {
        SessionStatus::Running => cx.theme().success,
        SessionStatus::WaitingForInput => cx.theme().warning,
        SessionStatus::Done => cx.theme().muted_foreground,
    }
}

pub(crate) fn status_dot(color: Hsla) -> impl IntoElement {
    div().size_2().rounded_full().bg(color)
}

/// Position `element` at `rect`, relative to the pane area.
fn place(element: Div, rect: Rect) -> Div {
    element
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.width))
        .h(px(rect.height))
}
