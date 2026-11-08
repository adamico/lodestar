//! A pane's Transcript: a virtualised list of its items and the (disabled) input box below.
//!
//! The list is GPUI's `list`, which measures each row as it is drawn, so markdown of any
//! height works. Its `ListState` is the per-pane scroll handle: ticket #12 can follow the tail
//! with `set_follow_mode(FollowMode::Tail)` and detect "scrolled up" with `is_following_tail()`
//! and `is_scrolled_to_end()`.

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

use crate::main_window::MainWindow;

/// Rows GPUI lays out beyond the visible part of the list, so scrolling stays smooth.
const LIST_OVERDRAW: Pixels = px(400.);
const INPUT_PLACEHOLDER: &str = "Message the agent";
const INPUT_TOOLTIP: &str = "Sending arrives in Milestone 2";
const USER_MARKER: &str = "You";
const COLLAPSED_MARKER: &str = "\u{25B8}";
const EXPANDED_MARKER: &str = "\u{25BE}";

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

/// Keys that scroll the focused pane's Transcript.
///
/// They are plain keys, so they stay out of the way of any focused text input and of a
/// pending leader.
pub fn bind_keys(cx: &mut App) {
    const OUTSIDE_INPUTS: Option<&str> = Some("!Input && !leader");
    cx.bind_keys([
        KeyBinding::new("ctrl-u", ScrollHalfPageUp, OUTSIDE_INPUTS),
        KeyBinding::new("ctrl-d", ScrollHalfPageDown, OUTSIDE_INPUTS),
        KeyBinding::new("g", ScrollToTop, OUTSIDE_INPUTS),
        KeyBinding::new("shift-g", ScrollToBottom, OUTSIDE_INPUTS),
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
}

impl TranscriptViews {
    /// Render `transcript` in `pane`: the list above, the disabled input box below.
    pub fn render(
        &mut self,
        pane: PaneId,
        session: SessionId,
        transcript: &Transcript,
        window: &mut Window,
        cx: &mut Context<MainWindow>,
    ) -> impl IntoElement + use<> {
        let state = self.sync(pane, session, transcript.items().len(), window, cx);
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
                    .id(pane_element_id(pane, "transcript"))
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(list)
                    .vertical_scrollbar(&list_for_scrollbar),
            )
            .child(
                div()
                    .id(pane_element_id(pane, "input"))
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
    /// A pane that switches session starts from the top with every tool call collapsed; a
    /// Transcript that grew keeps its scroll position and learns about the new rows.
    fn sync(
        &mut self,
        pane: PaneId,
        session: SessionId,
        item_count: usize,
        window: &mut Window,
        cx: &mut Context<MainWindow>,
    ) -> &PaneTranscript {
        let state = self.panes.entry(pane).or_insert_with(|| PaneTranscript {
            session,
            list: ListState::new(item_count, ListAlignment::Top, LIST_OVERDRAW),
            input: cx.new(|cx| InputState::new(window, cx).placeholder(INPUT_PLACEHOLDER)),
            expanded: HashSet::new(),
        });
        if state.session != session {
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
        state
    }
}

fn pane_element_id(pane: PaneId, what: &str) -> ElementId {
    ElementId::Name(format!("{what}-{pane:?}").into())
}

fn item_element_id(pane: PaneId, ix: usize, what: &str) -> ElementId {
    ElementId::NamedInteger(format!("{what}-{pane:?}").into(), ix as u64)
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

/// An assistant message: its markdown, rendered without syntax highlighting.
fn render_assistant(pane: PaneId, ix: usize, markdown: &str) -> impl IntoElement {
    TextView::markdown(item_element_id(pane, ix, "assistant"), markdown.to_string())
        .selectable(true)
}

/// A tool call: its headline, and its input and output in monospace when expanded.
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
        );

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
                .child(monospace_block("Output", &call.output, cx)),
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
