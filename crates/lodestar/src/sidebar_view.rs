//! The session sidebar: its rows, keyboard focus, visibility and drag-resizable width.
//!
//! All of this is view state. The window model lays out the pane area it is given, so hiding
//! or resizing the sidebar only changes the size the window hands it. The row order and the
//! selection rules live in [`lodestar_core::sidebar`]; this module renders them and turns keys
//! and mouse events into commands.

use std::rc::Rc;

use gpui_kit::component::Collapsible;
use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarItem};
use gpui_kit::component::{ActiveTheme, StyledExt as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use lodestar_core::session::{Session, SessionId};
use lodestar_core::sidebar::{SidebarList, clamp_width};

use crate::drag_drop;
use crate::main_window::{MainWindow, status_color, status_dot};

/// The sidebar's width at launch.
const DEFAULT_WIDTH: Pixels = px(255.);
/// How wide the drag handle on the sidebar's edge is.
const HANDLE_WIDTH: Pixels = px(6.);
/// Marks a sidebar row whose session is shown in a pane.
const SHOWN_MARKER: &str = "\u{25A3}";

/// Everything the window remembers about the sidebar.
pub struct SidebarState {
    visible: bool,
    width: Pixels,
    /// The selected row, while the sidebar has keyboard focus.
    selected: Option<usize>,
    /// While the edge is being dragged: the mouse x and the width when the drag started.
    drag: Option<(Pixels, Pixels)>,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            visible: true,
            width: DEFAULT_WIDTH,
            selected: None,
            drag: None,
        }
    }
}

impl SidebarState {
    /// The width the sidebar takes from the window: none while hidden.
    pub fn width(&self) -> Pixels {
        if self.visible { self.width } else { px(0.) }
    }

    /// Whether the sidebar has keyboard focus.
    pub fn is_focused(&self) -> bool {
        self.selected.is_some()
    }

    /// Give the keyboard back to the panes.
    pub fn blur(&mut self) {
        self.selected = None;
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    fn drag_to(&mut self, mouse_x: Pixels) {
        if let Some((start_x, start_width)) = self.drag {
            let width = f32::from(start_width + (mouse_x - start_x));
            self.width = px(clamp_width(width));
        }
    }
}

/// A key pressed while the sidebar has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarKey {
    Down,
    Up,
    Open,
    Leave,
}

impl SidebarKey {
    fn from_keystroke(keystroke: &Keystroke) -> Option<Self> {
        if keystroke.modifiers.modified() {
            return None;
        }
        match keystroke.key.as_str() {
            "j" => Some(Self::Down),
            "k" => Some(Self::Up),
            "enter" => Some(Self::Open),
            "escape" => Some(Self::Leave),
            _ => None,
        }
    }
}

impl MainWindow {
    fn sidebar_list(&self) -> SidebarList {
        SidebarList::new(&self.sessions)
    }

    /// Leader `b`: show the sidebar if needed and give it the keyboard, selecting the focused
    /// pane's session.
    pub(crate) fn focus_sidebar(&mut self) {
        self.sidebar.visible = true;
        let focused_session = self
            .window_model
            .session_in(self.window_model.focused_pane());
        let selection = self.sidebar_list().initial_selection(focused_session);
        self.sidebar.selected = Some(selection);
    }

    /// Leader `B`: hide or show the sidebar. Hiding it also gives the keyboard back to the panes.
    pub(crate) fn toggle_sidebar(&mut self) {
        self.sidebar.visible = !self.sidebar.visible;
        if !self.sidebar.visible {
            self.sidebar.blur();
        }
    }

    /// Handle a key while the sidebar has focus. Returns whether the key was the sidebar's.
    pub(crate) fn on_sidebar_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(selected) = self.sidebar.selected else {
            return false;
        };
        let Some(key) = SidebarKey::from_keystroke(&event.keystroke) else {
            return false;
        };
        cx.stop_propagation();
        let list = self.sidebar_list();
        match key {
            SidebarKey::Down => self.sidebar.selected = Some(list.step(selected, 1)),
            SidebarKey::Up => self.sidebar.selected = Some(list.step(selected, -1)),
            SidebarKey::Open => {
                if let Some(session) = list.session_at(selected) {
                    let pane = self.window_model.focused_pane();
                    self.window_model.open_session(session, pane);
                }
                self.sidebar.blur();
            }
            SidebarKey::Leave => self.sidebar.blur(),
        }
        cx.notify();
        true
    }

    /// Follow an edge drag with the mouse. Wired on the root while a drag is active, since a
    /// handle's own listeners only fire while the mouse is over it.
    pub(crate) fn on_sidebar_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            self.sidebar.drag = None;
        } else {
            self.sidebar.drag_to(event.position.x);
        }
        cx.notify();
    }

    pub(crate) fn on_sidebar_drag_end(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar.drag_to(event.position.x);
        self.sidebar.drag = None;
        cx.notify();
    }

    /// The sidebar with its drag handle, or nothing while hidden.
    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if !self.sidebar.visible {
            return None;
        }
        let width = self.sidebar.width;
        let mut row = 0;
        let groups: Vec<SidebarGroup<SessionRow>> = self
            .sessions
            .projects()
            .iter()
            .map(|project| {
                let rows: Vec<SessionRow> = self
                    .sessions
                    .sessions_in(project.id)
                    .into_iter()
                    .map(|session| {
                        let item = self.render_sidebar_row(session, row, cx);
                        row += 1;
                        item
                    })
                    .collect();
                SidebarGroup::new(project.name.clone()).children(rows)
            })
            .collect();

        let sidebar = Sidebar::new("sessions")
            .collapsible(false)
            .w(width)
            .header(div().px_2().text_sm().font_semibold().child("Sessions"))
            .children(groups);

        let handle = div()
            .id("sidebar-edge")
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(HANDLE_WIDTH)
            .cursor_col_resize()
            .hover(|this| this.bg(cx.theme().border))
            .when(self.sidebar.is_dragging(), |this| this.bg(cx.theme().ring))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.sidebar.drag = Some((event.position.x, width));
                    cx.notify();
                }),
            );

        Some(
            div()
                .relative()
                .flex_shrink_0()
                .h_full()
                .w(width)
                .child(sidebar)
                .child(handle),
        )
    }

    /// One session in the sidebar: title, a marker if a pane shows it, and its status dot.
    /// Clicking it opens the session in the focused pane; dragging it onto a pane drops it
    /// there. While the sidebar has focus, the selected row is highlighted instead of the
    /// shown ones.
    fn render_sidebar_row(
        &self,
        session: &Session,
        row: usize,
        cx: &mut Context<Self>,
    ) -> SessionRow {
        let id = session.id;
        let shown = self.window_model.pane_showing(id).is_some();
        let active = match self.sidebar.selected {
            Some(selected) => selected == row,
            None => shown,
        };
        SessionRow {
            session: id,
            title: session.title.clone().into(),
            shown,
            active,
            status: status_color(session.status, cx),
            on_click: Rc::new(cx.listener(move |this, _, _, cx| {
                let pane = this.window_model.focused_pane();
                this.window_model.open_session(id, pane);
                this.sidebar.blur();
                cx.notify();
            })),
            collapsed: false,
        }
    }
}

/// What a sidebar row does when clicked.
type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A sidebar row for one session. gpui-kit's menu item cannot start a drag, so the row is
/// Lodestar's own, drawn in the same style.
#[derive(Clone)]
struct SessionRow {
    session: SessionId,
    title: SharedString,
    shown: bool,
    active: bool,
    status: Hsla,
    on_click: ClickHandler,
    collapsed: bool,
}

impl Collapsible for SessionRow {
    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    fn is_collapsed(&self) -> bool {
        self.collapsed
    }
}

impl SidebarItem for SessionRow {
    fn render(self, id: impl Into<ElementId>, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let on_click = self.on_click;
        let row = h_flex()
            .id(id)
            .w_full()
            .h_7()
            .p_2()
            .gap_x_2()
            .items_center()
            .rounded(theme.radius)
            .text_sm()
            .overflow_x_hidden()
            .when(!self.active, |this| {
                this.hover(|this| {
                    this.bg(theme.sidebar_accent.opacity(0.8))
                        .text_color(theme.sidebar_accent_foreground)
                })
            })
            .when(self.active, |this| {
                this.font_medium()
                    .bg(theme.sidebar_accent)
                    .text_color(theme.sidebar_accent_foreground)
            })
            .child(div().flex_1().truncate().child(self.title.clone()))
            .when(self.shown, |this| this.child(SHOWN_MARKER))
            .child(status_dot(self.status))
            .on_click(move |event, window, cx| on_click(event, window, cx));
        drag_drop::drag_session(row, self.session, self.title)
    }
}
