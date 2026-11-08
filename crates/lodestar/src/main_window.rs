//! The window's root view: the session sidebar beside the pane area.
//!
//! It renders the window model and session store, and turns input into window-model commands.
//! Layout rules live in the window model, not here.

use std::time::Duration;

use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem};
use gpui_kit::component::{ActiveTheme, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use lodestar_core::geometry::{Rect, Size};
use lodestar_core::session::{Session, SessionStatus, SessionStore};
use lodestar_core::window::{Direction, PaneId, Refused, WindowModel};

use crate::leader::{CancelLeader, LEADER_FLAG, Leader, LeaderCommand, Mode, WINDOW_CONTEXT};

const SIDEBAR_WIDTH: Pixels = px(255.);
const PANE_HEADER_HEIGHT: Pixels = px(30.);
/// Lodestar's one accent colour: marks the focused pane.
const ACCENT: u32 = 0x3b82f6;
const UNFOCUSED_OPACITY: f32 = 0.6;
/// How long a pane flashes after a refused command.
const FLASH_DURATION: Duration = Duration::from_millis(150);
/// Marks a sidebar row whose session is shown in a pane.
const SHOWN_MARKER: &str = "\u{25A3}";

pub struct MainWindow {
    window_model: WindowModel,
    sessions: SessionStore,
    focus_handle: FocusHandle,
    mode: Mode,
    /// The pane whose last command was refused, while its flash lasts.
    flashing: Option<PaneId>,
}

impl MainWindow {
    pub fn new(sessions: SessionStore, cx: &mut Context<Self>) -> Self {
        Self {
            window_model: WindowModel::new(),
            sessions,
            focus_handle: cx.focus_handle(),
            mode: Mode::default(),
            flashing: None,
        }
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    /// The size of the pane area: the window minus the sidebar.
    fn pane_area(window: &Window) -> Size {
        let viewport = window.viewport_size();
        Size::new(
            f32::from(viewport.width - SIDEBAR_WIDTH).max(0.),
            f32::from(viewport.height),
        )
    }

    fn key_context(&self) -> KeyContext {
        let mut context = KeyContext::default();
        context.add(WINDOW_CONTEXT);
        if self.mode == Mode::Leader {
            context.add(LEADER_FLAG);
        }
        context
    }

    fn on_leader(&mut self, _: &Leader, _: &mut Window, cx: &mut Context<Self>) {
        self.mode = Mode::Leader;
        cx.notify();
    }

    fn on_cancel_leader(&mut self, _: &CancelLeader, _: &mut Window, cx: &mut Context<Self>) {
        self.mode = Mode::Normal;
        cx.notify();
    }

    /// While the leader is pending, the next key is a command or ends the mode.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::Leader {
            return;
        }
        cx.stop_propagation();
        self.mode = Mode::Normal;
        if let Some(command) = LeaderCommand::from_keystroke(&event.keystroke) {
            self.run(command, window, cx);
        }
        cx.notify();
    }

    fn run(&mut self, command: LeaderCommand, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.window_model.focused_pane();
        let area = Self::pane_area(window);
        let result = match command {
            LeaderCommand::SplitAuto => self.window_model.split_auto(pane, area).map(|_| ()),
            LeaderCommand::SplitRight => self
                .window_model
                .split(pane, Direction::Right, area)
                .map(|_| ()),
            LeaderCommand::SplitDown => self
                .window_model
                .split(pane, Direction::Down, area)
                .map(|_| ()),
            LeaderCommand::ClosePane => {
                self.window_model.close(pane);
                Ok(())
            }
            LeaderCommand::Focus(direction) => {
                self.window_model.focus_in_direction(direction, area);
                Ok(())
            }
            LeaderCommand::Swap(direction) => {
                self.window_model.swap_in_direction(direction, area);
                Ok(())
            }
        };
        if let Err(refused) = result {
            self.flash(pane, refused, window, cx);
        }
        cx.notify();
    }

    /// Show briefly that a command on `pane` was refused.
    fn flash(&mut self, pane: PaneId, _: Refused, window: &mut Window, cx: &mut Context<Self>) {
        self.flashing = Some(pane);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(FLASH_DURATION).await;
            this.update(cx, |this, cx| {
                if this.flashing == Some(pane) {
                    this.flashing = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn render_hint_popup(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let rows = LeaderCommand::ALL.iter().map(|(key, label, _)| {
            h_flex()
                .gap_2()
                .child(
                    div()
                        .min_w(px(40.))
                        .font_family("monospace")
                        .text_color(rgb(ACCENT))
                        .child(*key),
                )
                .child(div().text_color(theme.foreground).child(*label))
        });
        v_flex()
            .absolute()
            .bottom(px(16.))
            .right(px(16.))
            .p_3()
            .gap_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_md()
            .text_sm()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("ctrl+space, then"),
            )
            .children(rows)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("esc to cancel"),
            )
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
        let layout = self.window_model.layout(Self::pane_area(window));
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
            .when(self.mode == Mode::Leader, |this| {
                this.child(self.render_hint_popup(cx))
            })
    }

    fn render_pane(&self, pane: PaneId, focused: bool, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let flashing = self.flashing == Some(pane);
        let (border, header_bg, header_fg) = if flashing {
            (theme.danger, theme.danger, gpui_kit::white())
        } else if focused {
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
        let header = header.child(div().flex_1()).child(
            div()
                .id(pane_element_id("close-pane", pane))
                .px_1()
                .rounded_sm()
                .cursor_pointer()
                .hover(|this| this.bg(gpui_kit::white().opacity(0.2)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.window_model.close(pane);
                    cx.notify();
                }))
                .child("×"),
        );
        let body = v_flex()
            .flex_1()
            .overflow_hidden()
            .p_2()
            .gap_2()
            .text_sm()
            .children(session.into_iter().flat_map(|session| {
                session
                    .transcript
                    .items()
                    .iter()
                    .map(|item| div().child(item.clone()))
            }));

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
            .child(header)
            .child(body)
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .key_context(self.key_context())
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_leader))
            .on_action(cx.listener(Self::on_cancel_leader))
            .on_key_down(cx.listener(Self::on_key_down))
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

/// A stable per-pane element id, so pane widgets keep their state across renders.
fn pane_element_id(widget: &str, pane: PaneId) -> ElementId {
    ElementId::Name(format!("{widget}-{pane:?}").into())
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
