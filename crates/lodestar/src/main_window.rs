//! The window's root view: the session sidebar beside the pane area.
//!
//! It renders the window model and session store, and turns input into window-model commands.
//! Layout rules live in the window model, not here.

use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem};
use gpui_kit::component::{ActiveTheme, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use lodestar_core::geometry::{Rect, Size};
use lodestar_core::session::{SessionStatus, SessionStore};
use lodestar_core::window::{PaneId, WindowModel};

const SIDEBAR_WIDTH: Pixels = px(255.);
const PANE_HEADER_HEIGHT: Pixels = px(30.);
/// Lodestar's one accent colour: marks the focused pane.
const ACCENT: u32 = 0x3b82f6;
const UNFOCUSED_OPACITY: f32 = 0.6;

pub struct MainWindow {
    window_model: WindowModel,
    sessions: SessionStore,
}

impl MainWindow {
    pub fn new(sessions: SessionStore) -> Self {
        Self {
            window_model: WindowModel::new(),
            sessions,
        }
    }

    fn render_sidebar(&self, cx: &App) -> impl IntoElement {
        let groups = self.sessions.projects().iter().map(|project| {
            let items = self
                .sessions
                .sessions_in(project.id)
                .into_iter()
                .map(|session| {
                    let color = status_color(session.status, cx);
                    SidebarMenuItem::new(session.title.clone())
                        .suffix(move |_, _| status_dot(color))
                });
            SidebarGroup::new(project.name.clone()).child(SidebarMenu::new().children(items))
        });

        Sidebar::new("sessions")
            .collapsible(false)
            .w(SIDEBAR_WIDTH)
            .header(div().px_2().text_sm().font_semibold().child("Sessions"))
            .children(groups)
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
        let (border, header_bg, header_fg) = if focused {
            (rgb(ACCENT).into(), rgb(ACCENT).into(), gpui_kit::white())
        } else {
            (theme.border, theme.secondary, theme.muted_foreground)
        };
        let title = match self
            .window_model
            .session_in(pane)
            .and_then(|id| self.sessions.session(id))
        {
            Some(session) => session.title.clone(),
            None => "Empty".to_owned(),
        };

        v_flex()
            .border_1()
            .border_color(border)
            .bg(theme.background)
            .when(!focused, |this| this.opacity(UNFOCUSED_OPACITY))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.window_model.focus(pane);
                    cx.notify();
                }),
            )
            .child(
                h_flex()
                    .h(PANE_HEADER_HEIGHT)
                    .flex_shrink_0()
                    .px_2()
                    .bg(header_bg)
                    .text_color(header_fg)
                    .text_sm()
                    .child(title),
            )
            .child(div().flex_1())
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_sidebar(cx))
            .child(self.render_panes(window, cx))
    }
}

fn status_color(status: SessionStatus, cx: &App) -> Hsla {
    match status {
        SessionStatus::Running => cx.theme().success,
        SessionStatus::WaitingForInput => cx.theme().warning,
        SessionStatus::Done => cx.theme().muted_foreground,
    }
}

fn status_dot(color: Hsla) -> impl IntoElement {
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
