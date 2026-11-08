//! A pane's Transcript: a virtualised list of its items and the (disabled) input box below.
//!
//! The list is GPUI's `list`, which measures each row as it is drawn, so markdown of any
//! height works. Its `ListState` is the per-pane scroll handle. A pane whose session is
//! running follows the tail: new rows keep the end in view until the user scrolls up, which
//! pauses following and shows a "Jump to latest" pill that resumes it.

use std::collections::{HashMap, HashSet};

use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::text::TextView;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use lodestar_core::session::{SessionId, Transcript, TranscriptItem};
use lodestar_core::window::PaneId;

use crate::main_window::{MainWindow, pane_element_id};

/// Rows GPUI lays out beyond the visible part of the list, so scrolling stays smooth.
const LIST_OVERDRAW: Pixels = px(400.);
const INPUT_PLACEHOLDER: &str = "Message the agent";
const INPUT_TOOLTIP: &str = "Sending arrives in Milestone 2";
const USER_MARKER: &str = "You";
const ERROR_MARKER: &str = "Error";
const COLLAPSED_MARKER: &str = "\u{25B8}";
const EXPANDED_MARKER: &str = "\u{25BE}";
/// Shown after a pending tool call's headline, until its result arrives.
const PENDING_MARKER: &str = "\u{22EF} pending";
const PENDING_OUTPUT: &str = "(pending)";
const JUMP_TO_LATEST: &str = "\u{2193} Jump to latest";

actions!(
    lodestar,
    [
        ScrollHalfPageUp,
        ScrollHalfPageDown,
        ScrollToTop,
        ScrollToBottom
    ]
);

/// How the keyboard moves a Transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    HalfPageUp,
    HalfPageDown,
    Top,
    Bottom,
}

/// Set on the window's key context only while the focused pane shows a Transcript that
/// plain keys may scroll: normal mode, sidebar not focused, pane not empty. Without it the
/// scroll keys stay unbound, so GPUI hands them on to the window's key-down listener, where
/// an empty pane's picker and the sidebar read them.
pub const TRANSCRIPT_FLAG: &str = "transcript";

/// Keys that scroll the focused pane's Transcript.
///
/// They are plain keys, so they are bound only under [`TRANSCRIPT_FLAG`] and outside any
/// focused text input.
pub fn bind_keys(cx: &mut App) {
    const CONTEXT: Option<&str> = Some("MainWindow && transcript && !Input");
    cx.bind_keys([
        KeyBinding::new("ctrl-u", ScrollHalfPageUp, CONTEXT),
        KeyBinding::new("ctrl-d", ScrollHalfPageDown, CONTEXT),
        KeyBinding::new("g", ScrollToTop, CONTEXT),
        KeyBinding::new("shift-g", ScrollToBottom, CONTEXT),
    ]);
}

/// Route the scroll actions reaching `element` to the focused pane's Transcript.
pub fn on_scroll_actions(element: Div, cx: &mut Context<MainWindow>) -> Div {
    element
        .on_action(cx.listener(|this, _: &ScrollHalfPageUp, _, cx| {
            this.scroll_transcript(Scroll::HalfPageUp, cx)
        }))
        .on_action(cx.listener(|this, _: &ScrollHalfPageDown, _, cx| {
            this.scroll_transcript(Scroll::HalfPageDown, cx)
        }))
        .on_action(
            cx.listener(|this, _: &ScrollToTop, _, cx| this.scroll_transcript(Scroll::Top, cx)),
        )
        .on_action(
            cx.listener(|this, _: &ScrollToBottom, _, cx| {
                this.scroll_transcript(Scroll::Bottom, cx)
            }),
        )
}

/// The view state of every pane that shows a Transcript, keyed by pane.
#[derive(Default)]
pub struct TranscriptViews {
    panes: HashMap<PaneId, PaneTranscript>,
}

/// What one pane remembers about the Transcript it shows.
struct PaneTranscript {
    /// The session whose Transcript the list is sized for.
    session: SessionId,
    /// The scroll handle. Row heights are measured as rows are drawn.
    list: ListState,
    input: Entity<InputState>,
    /// Indices of the tool calls the user has expanded.
    expanded: HashSet<usize>,
    /// Whether the list was told to follow its tail, which it does for a running session.
    follows_tail: bool,
}

impl TranscriptViews {
    /// Render `transcript` in `pane`: the list above, the disabled input box below.
    ///
    /// While `streaming`, the list follows new items; once the user scrolls up, a pill over
    /// the bottom of the list offers to jump back to the latest.
    pub fn render(
        &mut self,
        pane: PaneId,
        session: SessionId,
        transcript: &Transcript,
        streaming: bool,
        window: &mut Window,
        cx: &mut Context<MainWindow>,
    ) -> impl IntoElement + use<> {
        let state = self.sync(
            pane,
            session,
            transcript.items().len(),
            streaming,
            window,
            cx,
        );
        let paused = streaming && !state.list.is_following_tail();
        let items = transcript.items().to_vec();
        let expanded = state.expanded.clone();
        let this = cx.weak_entity();
        let list_state = state.list.clone();
        let list_for_scrollbar = list_state.clone();

        let list = list(list_state, move |ix, _, cx| {
            let item = &items[ix];
            div()
                .w_full()
                .min_w_0()
                .px_3()
                .pb_3()
                .when(ix == 0, |this| this.pt_3())
                .child(match item {
                    TranscriptItem::User(text) => render_user(text, cx).into_any_element(),
                    TranscriptItem::Assistant(markdown) => {
                        render_assistant(pane, ix, markdown).into_any_element()
                    }
                    TranscriptItem::Error(message) => render_error(message, cx).into_any_element(),
                    TranscriptItem::ToolCall(call) => {
                        let this = this.clone();
                        let toggle = move |cx: &mut App| {
                            this.update(cx, |main_window, cx| {
                                main_window.toggle_tool_call(pane, ix, cx);
                            })
                            .ok();
                        };
                        render_tool_call(pane, ix, call, expanded.contains(&ix), toggle, cx)
                            .into_any_element()
                    }
                })
                .into_any_element()
        })
        .size_full()
        .min_h_0();

        v_flex()
            .flex_1()
            .min_h_0()
            .text_sm()
            .child(
                div()
                    .id(pane_element_id("transcript", pane))
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(list)
                    .vertical_scrollbar(&list_for_scrollbar)
                    .when(paused, |this| this.child(render_jump_to_latest(pane, cx))),
            )
            .child(
                div()
                    .id(pane_element_id("input", pane))
                    .flex_shrink_0()
                    .p_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .tooltip(|window, cx| Tooltip::new(INPUT_TOOLTIP).build(window, cx))
                    .child(Input::new(&state.input).disabled(true)),
            )
    }

    /// Show or hide the input and output of the tool call at `ix` in `pane`.
    pub fn toggle_tool_call(&mut self, pane: PaneId, ix: usize) {
        if let Some(state) = self.panes.get_mut(&pane) {
            if !state.expanded.remove(&ix) {
                state.expanded.insert(ix);
            }
            if ix < state.list.item_count() {
                state.list.remeasure_items(ix..ix + 1);
            }
        }
    }

    /// Scroll `pane`'s Transcript to its end and follow new items again.
    pub fn jump_to_latest(&mut self, pane: PaneId) {
        if let Some(state) = self.panes.get_mut(&pane) {
            state.follows_tail = true;
            state.list.set_follow_mode(FollowMode::Tail);
        }
    }

    /// Whether any pane has stopped following a tail it was told to follow.
    ///
    /// The list resumes following on its own when scrolled back to the end, during layout,
    /// so the window re-renders while this holds to keep the pill honest.
    pub fn any_paused(&self) -> bool {
        self.panes
            .values()
            .any(|state| state.follows_tail && !state.list.is_following_tail())
    }

    /// Move `pane`'s Transcript. Panes without a Transcript ignore it.
    pub fn scroll(&mut self, pane: PaneId, scroll: Scroll) {
        let Some(state) = self.panes.get(&pane) else {
            return;
        };
        let list = &state.list;
        match scroll {
            Scroll::HalfPageUp => list.scroll_by(-list.viewport_bounds().size.height / 2.),
            Scroll::HalfPageDown => list.scroll_by(list.viewport_bounds().size.height / 2.),
            Scroll::Top => list.scroll_to(ListOffset::default()),
            Scroll::Bottom => list.scroll_to_end(),
        }
    }

    /// Forget the view state of panes that no longer show a Transcript.
    pub fn retain(&mut self, shows_transcript: impl Fn(PaneId) -> bool) {
        self.panes.retain(|pane, _| shows_transcript(*pane));
    }

    /// The state for `pane`, sized for `item_count` items of `session`.
    ///
    /// A pane that switches session starts with every tool call collapsed, from the top, or
    /// from the end and following the tail when the session is `streaming`; a Transcript
    /// that grew keeps its scroll position and learns about the new rows.
    fn sync(
        &mut self,
        pane: PaneId,
        session: SessionId,
        item_count: usize,
        streaming: bool,
        window: &mut Window,
        cx: &mut Context<MainWindow>,
    ) -> &PaneTranscript {
        let mut fresh = false;
        let state = self.panes.entry(pane).or_insert_with(|| {
            fresh = true;
            PaneTranscript {
                session,
                list: ListState::new(item_count, ListAlignment::Top, LIST_OVERDRAW),
                input: cx.new(|cx| InputState::new(window, cx).placeholder(INPUT_PLACEHOLDER)),
                expanded: HashSet::new(),
                follows_tail: false,
            }
        });
        if state.session != session {
            fresh = true;
            state.session = session;
            state.expanded.clear();
            state.list.reset(item_count);
        } else {
            let known = state.list.item_count();
            if item_count != known {
                let kept = known.min(item_count);
                state.list.splice(kept..known, item_count - kept);
            }
        }
        if fresh {
            state.follows_tail = streaming;
            state.list.set_follow_mode(if streaming {
                FollowMode::Tail
            } else {
                FollowMode::Normal
            });
        }
        state
    }
}

fn item_element_id(pane: PaneId, ix: usize, what: &str) -> ElementId {
    ElementId::NamedInteger(format!("{what}-{pane:?}").into(), ix as u64)
}

/// The pill over the bottom of a paused streaming Transcript: click to follow the tail again.
fn render_jump_to_latest(pane: PaneId, cx: &mut Context<MainWindow>) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .absolute()
        .bottom_3()
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            div()
                .id(pane_element_id("jump-to-latest", pane))
                .px_3()
                .py_1()
                .rounded_full()
                .bg(theme.primary)
                .text_color(theme.primary_foreground)
                .text_xs()
                .font_semibold()
                .shadow_md()
                .cursor_pointer()
                .hover(|this| this.bg(theme.primary_hover))
                .on_click(cx.listener(move |this, _, _, cx| this.jump_to_latest(pane, cx)))
                .child(JUMP_TO_LATEST),
        )
}

/// A user message: a marker and the text on an accent-tinted card.
fn render_user(text: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .gap_1()
        .p_2()
        .rounded_md()
        .bg(theme.secondary)
        .border_l_2()
        .border_color(theme.primary)
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(theme.muted_foreground)
                .child(USER_MARKER),
        )
        .child(div().child(text.to_string()))
}

/// An error: why the session stopped, on a danger-tinted card.
fn render_error(message: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .gap_1()
        .p_2()
        .rounded_md()
        .bg(theme.secondary)
        .border_l_2()
        .border_color(theme.danger)
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(theme.danger)
                .child(ERROR_MARKER),
        )
        .child(div().child(message.to_string()))
}

/// An assistant message: its markdown, rendered without syntax highlighting.
fn render_assistant(pane: PaneId, ix: usize, markdown: &str) -> impl IntoElement {
    TextView::markdown(item_element_id(pane, ix, "assistant"), markdown.to_string())
        .selectable(true)
}

/// A tool call: its headline (with a pending marker until the result arrives), and its input
/// and output in monospace when expanded.
fn render_tool_call(
    pane: PaneId,
    ix: usize,
    call: &lodestar_core::session::ToolCall,
    expanded: bool,
    toggle: impl Fn(&mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let marker = if expanded {
        EXPANDED_MARKER
    } else {
        COLLAPSED_MARKER
    };
    let headline = h_flex()
        .id(item_element_id(pane, ix, "tool-call"))
        .gap_2()
        .px_2()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .text_color(theme.muted_foreground)
        .hover(|this| this.bg(theme.list_hover))
        .on_click(move |_, _, cx| toggle(cx))
        .child(div().w_3().flex_shrink_0().child(marker))
        .child(
            div()
                .font_family(theme.mono_font_family.clone())
                .child(call.headline()),
        )
        .when(call.is_pending(), |this| {
            this.child(
                div()
                    .text_xs()
                    .flex_shrink_0()
                    .text_color(theme.warning)
                    .child(PENDING_MARKER),
            )
        });
    let output = match &call.output {
        Some(output) => monospace_block("Output", output, cx).into_any_element(),
        None => div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(PENDING_OUTPUT)
            .into_any_element(),
    };

    Collapsible::new()
        .open(expanded)
        .motion_id(item_element_id(pane, ix, "tool-call-reveal"))
        .border_1()
        .border_color(theme.border)
        .rounded_md()
        .child(headline)
        .content(
            v_flex()
                .gap_2()
                .p_2()
                .border_t_1()
                .border_color(theme.border)
                .child(monospace_block("Input", &call.input, cx))
                .child(output),
        )
}

/// A labelled block of plain monospace text.
fn monospace_block(label: &str, text: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let text = if text.is_empty() { "(empty)" } else { text };
    v_flex()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(theme.muted_foreground)
                .child(label.to_string()),
        )
        .child(
            div()
                .p_2()
                .rounded_sm()
                .bg(theme.muted)
                .font_family(theme.mono_font_family.clone())
                .text_xs()
                .whitespace_normal()
                .child(text.to_string()),
        )
}
