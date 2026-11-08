//! The window's root view: the session sidebar beside the pane area.
//!
//! It renders the window model and session store, and turns input into window-model commands.
//! Layout rules live in the window model, not here.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui_kit::component::{ActiveTheme, StyledExt as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use lodestar_core::geometry::{Point, Rect, Size};
use lodestar_core::picker::PickerState;
use lodestar_core::session::{SessionSource, SessionStatus, SessionStore};
use lodestar_core::window::{
    Axis, Direction, DividerId, DividerLayout, PaneId, Refused, WindowModel,
};

use crate::drag_drop::{self, DropTarget};
use crate::leader::{
    CancelLeader, Hints, LEADER_FLAG, Leader, LeaderCommand, Mode, RESIZE_FLAG, RESIZE_STEP,
    WINDOW_CONTEXT, resize_direction,
};
use crate::sidebar_view::SidebarState;

use crate::transcript_view::{self, InputFacts, Scroll, TRANSCRIPT_FLAG, TranscriptViews};

const PANE_HEADER_HEIGHT: Pixels = px(30.);
/// Lodestar's one accent colour: marks the focused pane.
pub(crate) const ACCENT: u32 = 0x3b82f6;
const UNFOCUSED_OPACITY: f32 = 0.6;
/// How long a pane flashes after a refused command.
const FLASH_DURATION: Duration = Duration::from_millis(150);
/// How often the session source is asked for updates.
const UPDATE_TICK: Duration = Duration::from_millis(250);
/// How wide the grab band around a divider is, in logical pixels.
const DIVIDER_GRAB_WIDTH: Pixels = px(7.);

pub struct MainWindow {
    pub(crate) window_model: WindowModel,
    pub(crate) sessions: SessionStore,
    /// Where updates to the sessions come from. Polled on a timer for as long as the window
    /// lives, whether or not the sessions they touch are shown.
    pub(crate) source: Box<dyn SessionSource>,
    /// When polling started; the source's clock counts from here.
    started: Instant,
    /// The timer loop that polls the source. Dropped with the window.
    _updates: Task<()>,
    transcripts: TranscriptViews,
    /// Keys go to the window unless a text input took them.
    focus_handle: FocusHandle,
    mode: Mode,
    pub(crate) sidebar: SidebarState,
    /// The pane whose last command was refused, while its flash lasts.
    flashing: Option<PaneId>,
    /// Each empty pane's picker: its filter and selection. Absent means untouched.
    pub(crate) pickers: HashMap<PaneId, PickerState>,
    /// Where the current drag would drop, while one hovers a pane.
    pub(crate) drop_target: Option<DropTarget>,
    /// The divider being dragged with the mouse, if any.
    dragging: Option<DividerId>,
}

impl MainWindow {
    pub fn new(
        sessions: SessionStore,
        source: Box<dyn SessionSource>,
        cx: &mut Context<Self>,
    ) -> Self {
        let updates = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(UPDATE_TICK).await;
                if this.update(cx, |this, cx| this.pump_updates(cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            window_model: WindowModel::new(),
            sessions,
            source,
            started: Instant::now(),
            _updates: updates,
            transcripts: TranscriptViews::default(),
            focus_handle: cx.focus_handle(),
            mode: Mode::default(),
            sidebar: SidebarState::default(),
            flashing: None,
            pickers: HashMap::new(),
            drop_target: None,
            dragging: None,
        }
    }

    /// Apply whatever the source has for us by now. Sidebar dots, pane headers and
    /// Transcripts all render the store, so one notify refreshes them all. While a session
    /// runs, every tick re-renders, so its working indicator's seconds keep counting.
    fn pump_updates(&mut self, cx: &mut Context<Self>) {
        let updates = self.source.poll_updates(self.started.elapsed());
        let changed = !updates.is_empty();
        for update in updates {
            self.sessions.apply(update);
        }
        let running = self
            .sessions
            .sessions_newest_first()
            .iter()
            .any(|session| session.status == SessionStatus::Running);
        if changed || running || self.transcripts.any_paused() {
            cx.notify();
        }
    }

    /// Send what `pane`'s input box holds to the session it shows. The store decides whether
    /// the session takes a message now and records it; only then does the text reach the
    /// Agent. A refusal leaves the text in the box, and the box says why.
    pub fn send_from_input(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.window_model.session_in(pane) else {
            return;
        };
        let text = self.transcripts.input_text(pane, cx);
        if self
            .sessions
            .send(session, &text, self.started.elapsed())
            .is_ok()
        {
            self.source.send_message(session, &text);
            self.transcripts.clear_input(pane, window, cx);
        }
        cx.notify();
    }

    /// The input box of `pane` took the keyboard: the pane is focused and the sidebar is not.
    pub fn input_focused(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        self.window_model.focus(pane);
        self.sidebar.blur();
        cx.notify();
    }

    /// Scroll `pane`'s Transcript to the latest item and follow new ones again.
    pub fn jump_to_latest(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        self.transcripts.jump_to_latest(pane);
        cx.notify();
    }

    /// Scroll the focused pane's Transcript, if it shows one.
    pub fn scroll_transcript(&mut self, scroll: Scroll, cx: &mut Context<Self>) {
        self.transcripts
            .scroll(self.window_model.focused_pane(), scroll);
        cx.notify();
    }

    /// Expand or collapse the tool call at `ix` in `pane`.
    pub fn toggle_tool_call(&mut self, pane: PaneId, ix: usize, cx: &mut Context<Self>) {
        self.transcripts.toggle_tool_call(pane, ix);
        cx.notify();
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    /// The size of the pane area: the window minus the sidebar (all of it when hidden).
    pub(crate) fn pane_area(&self, window: &Window) -> Size {
        let viewport = window.viewport_size();
        Size::new(
            f32::from(viewport.width - self.sidebar.width()).max(0.),
            f32::from(viewport.height),
        )
    }

    fn key_context(&self) -> KeyContext {
        let mut context = KeyContext::default();
        context.add(WINDOW_CONTEXT);
        match self.mode {
            Mode::Normal => {
                if self.focused_pane_scrolls() {
                    context.add(TRANSCRIPT_FLAG);
                }
            }
            Mode::Leader => context.add(LEADER_FLAG),
            Mode::Resize => context.add(RESIZE_FLAG),
        }
        context
    }

    /// Whether plain keys scroll the focused pane's Transcript: only when the pane shows a
    /// session and the sidebar does not have the keyboard.
    fn focused_pane_scrolls(&self) -> bool {
        !self.sidebar.is_focused()
            && self
                .window_model
                .session_in(self.window_model.focused_pane())
                .is_some()
    }

    fn on_leader(&mut self, _: &Leader, _: &mut Window, cx: &mut Context<Self>) {
        self.mode = Mode::Leader;
        cx.notify();
    }

    fn on_cancel_leader(&mut self, _: &CancelLeader, _: &mut Window, cx: &mut Context<Self>) {
        self.mode = Mode::Normal;
        cx.notify();
    }

    /// While the leader is pending, the next key is a command or ends the mode. In resize mode,
    /// `h`/`j`/`k`/`l` keep resizing until `esc` (bound to [`CancelLeader`]). Otherwise keys
    /// go to a focused text input if there is one, else to the sidebar while it is focused,
    /// else to the focused pane's picker if that pane is empty.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match self.mode {
            Mode::Normal => {
                if window.has_focused_input(cx) {
                    return;
                }
                if self.sidebar.is_focused() {
                    self.on_sidebar_key(event, cx);
                } else if self.picker_key_down(event, cx) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }
            Mode::Leader => {
                cx.stop_propagation();
                self.mode = Mode::Normal;
                if let Some(command) = LeaderCommand::from_keystroke(&event.keystroke) {
                    self.run(command, window, cx);
                }
                cx.notify();
            }
            Mode::Resize => {
                cx.stop_propagation();
                if let Some(direction) = resize_direction(&event.keystroke) {
                    self.resize(direction, window, cx);
                }
            }
        }
    }

    /// One keyboard resize step toward `direction`; a refusal flashes the focused pane.
    fn resize(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        let area = self.pane_area(window);
        if let Err(refused) = self
            .window_model
            .resize_in_direction(direction, RESIZE_STEP, area)
        {
            self.flash(self.window_model.focused_pane(), refused, window, cx);
        }
        cx.notify();
    }

    fn run(&mut self, command: LeaderCommand, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.window_model.focused_pane();
        let area = self.pane_area(window);
        if command != LeaderCommand::FocusSidebar {
            self.sidebar.blur();
        }
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
            LeaderCommand::Resize => {
                self.mode = Mode::Resize;
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
            LeaderCommand::FocusSidebar => {
                self.focus_sidebar();
                Ok(())
            }
            LeaderCommand::ToggleSidebar => {
                self.toggle_sidebar();
                Ok(())
            }
        };
        if let Err(refused) = result {
            self.flash(pane, refused, window, cx);
        }
        cx.notify();
    }

    /// Show briefly that a command on `pane` was refused.
    pub(crate) fn flash(
        &mut self,
        pane: PaneId,
        _: Refused,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    fn render_hint_popup(&self, hints: Hints, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let rows = hints.rows.into_iter().map(|(key, label)| {
            h_flex()
                .gap_2()
                .child(
                    div()
                        .min_w(px(40.))
                        .font_family("monospace")
                        .text_color(rgb(ACCENT))
                        .child(key),
                )
                .child(div().text_color(theme.foreground).child(label))
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
                    .child(hints.title),
            )
            .children(rows)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(hints.footer),
            )
    }

    fn render_panes(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = self.window_model.layout(self.pane_area(window));
        let focused = self.window_model.focused_pane();
        self.transcripts
            .retain(|pane| self.window_model.session_in(pane).is_some());
        let panes: Vec<Div> = layout
            .panes
            .iter()
            .map(|pane| {
                place(
                    self.render_pane(pane.pane, pane.pane == focused, window, cx),
                    pane.rect,
                )
            })
            .collect();

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
            .children(panes)
            .children(
                layout
                    .dividers
                    .iter()
                    .map(|divider| self.render_divider_grab(divider, cx)),
            )
            .when_some(self.mode.hints(), |this, hints| {
                this.child(self.render_hint_popup(hints, cx))
            })
    }

    /// The invisible band over a divider that the mouse grabs to drag it. It sits above the
    /// panes so it can be a few pixels wider than the divider itself.
    fn render_divider_grab(&self, divider: &DividerLayout, cx: &mut Context<Self>) -> Div {
        let id = divider.id;
        let grab = f32::from(DIVIDER_GRAB_WIDTH);
        let rect = divider.rect;
        let (band, element) = match divider.axis {
            Axis::Horizontal => (
                Rect::new(rect.x + (rect.width - grab) / 2., rect.y, grab, rect.height),
                div().cursor_col_resize(),
            ),
            Axis::Vertical => (
                Rect::new(rect.x, rect.y + (rect.height - grab) / 2., rect.width, grab),
                div().cursor_row_resize(),
            ),
        };
        place(element, band).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.dragging = Some(id);
                cx.notify();
            }),
        )
    }

    /// A mouse move while a divider is being dragged: the divider follows the pointer.
    fn on_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(divider) = self.dragging else {
            return;
        };
        if !event.dragging() {
            self.dragging = None;
            cx.notify();
            return;
        }
        let position = Point::new(
            f32::from(event.position.x - self.sidebar.width()),
            f32::from(event.position.y),
        );
        let area = self.pane_area(window);
        if self
            .window_model
            .drag_divider(divider, position, area)
            .is_err()
        {
            self.dragging = None;
        }
        cx.notify();
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.dragging.take().is_some() {
            cx.notify();
        }
    }

    fn render_pane(
        &mut self,
        pane: PaneId,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.theme();
        let background = theme.background;
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
        let title: SharedString =
            session.map_or("Empty".into(), |session| session.title.clone().into());
        let header = h_flex()
            .id(pane_element_id("pane-header", pane))
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
        let header = drag_drop::drag_pane(header, pane, title);
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
        let body = match session {
            Some(session) => self
                .transcripts
                .render(
                    pane,
                    session,
                    InputFacts {
                        can_send: self.sessions.can_send(session.id),
                        working: self
                            .sessions
                            .working_line(session.id, self.started.elapsed()),
                        suggestion: self.sessions.suggestion(session.id),
                    },
                    window,
                    cx,
                )
                .into_any_element(),
            None => self.render_picker(pane, cx).into_any_element(),
        };

        let pane_element = v_flex()
            .overflow_hidden()
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
            .children(self.render_drop_overlay(pane));
        self.drop_target(pane_element, pane, cx)
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.clear_stale_drop_target(cx);
        let root = h_flex()
            .key_context(self.key_context())
            .on_action(cx.listener(Self::on_leader))
            .on_action(cx.listener(Self::on_cancel_leader))
            .on_key_down(cx.listener(Self::on_key_down))
            // The root is always hovered, so it sees the whole drag, including outside the band.
            .when_some(self.dragging, |this, _| {
                this.on_mouse_move(cx.listener(Self::on_drag_move))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_drag(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_drag(cx)),
                    )
            })
            .when(self.sidebar.is_dragging(), |this| {
                this.on_mouse_move(cx.listener(Self::on_sidebar_drag_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_sidebar_drag_end))
            })
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground);
        transcript_view::on_scroll_actions(root, cx)
            .children(self.render_sidebar(cx))
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

/// A stable per-pane element id, so pane widgets keep their state across renders.
pub(crate) fn pane_element_id(widget: &str, pane: PaneId) -> ElementId {
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
