//! A pane's Transcript: a virtualised list of its items and the input box below.
//!
//! The list is GPUI's `list`, which measures each row as it is drawn, so markdown of any
//! height works. Its `ListState` is the per-pane scroll handle. A pane whose session is
//! running follows the tail: new rows keep the end in view until the user scrolls up, which
//! pauses following. Once the last item is out of view, a "Jump to latest" pill offers to
//! resume it.
//!
//! While the session runs, the working indicator sits just above the input box: a spinner
//! and the store's working line (`55s · 1.5k tokens · Thinking`). It stands in for the
//! refusal notice, which would only repeat that the Agent is running. The window re-renders
//! at least every poll while a session runs, so its seconds tick.
//!
//! The input box sends on `enter` (`shift+enter` inserts a newline) and `escape` hands the
//! keyboard back to the window. When the Agent suggests the next message, an empty box shows
//! it as its placeholder and `tab` makes it the box's text, to send or edit; typed text
//! hides it and is never replaced. Whether a session can take a message, and what sending
//! does to it, are rules of `lodestar_core::session::SessionStore`; this only forwards.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use lodestar_core::session::{SendRefusal, Session, SessionId, SessionStatus, TranscriptItem};
use lodestar_core::window::PaneId;

use crate::main_window::{MainWindow, pane_element_id};

/// Rows GPUI lays out beyond the visible part of the list, so scrolling stays smooth.
const LIST_OVERDRAW: Pixels = px(400.);
const INPUT_PLACEHOLDER: &str = "Message the agent (enter sends, shift+enter for a newline)";
/// How many rows the input box grows to before it scrolls.
const INPUT_MAX_ROWS: usize = 6;
const USER_MARKER: &str = "You";
const ERROR_MARKER: &str = "Error";
const COLLAPSED_MARKER: &str = "\u{25B8}";
const EXPANDED_MARKER: &str = "\u{25BE}";
/// Shown after a pending tool call's headline, until its result arrives.
const PENDING_MARKER: &str = "\u{22EF} pending";
const PENDING_OUTPUT: &str = "(pending)";
const JUMP_TO_LATEST: &str = "\u{2193} Jump to latest";
/// How many dots the working spinner has, and how long one sweep across them takes.
const SPINNER_DOTS: usize = 3;
const SPINNER_PERIOD: Duration = Duration::from_millis(900);
/// The spinner's frame rate cap: smooth enough, without drawing every frame.
const SPINNER_FPS: f32 = 30.;

actions!(
    lodestar,
    [
        ScrollHalfPageUp,
        ScrollHalfPageDown,
        ScrollToTop,
        ScrollToBottom,
        LeaveInput,
        AcceptSuggestion
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

/// Set on the window's key context only while the focused pane's input box is empty and its
/// session has a prompt suggestion, so `tab` accepts it there and indents everywhere else.
pub const SUGGESTION_FLAG: &str = "suggestion";

/// Keys that scroll the focused pane's Transcript.
///
/// They are plain keys, so they are bound only under [`TRANSCRIPT_FLAG`] and outside any
/// focused text input.
pub fn bind_keys(cx: &mut App) {
    const CONTEXT: Option<&str> = Some("MainWindow && transcript && !Input");
    /// Bindings that apply only while one of the window's text inputs has the keyboard.
    const INPUT_CONTEXT: Option<&str> = Some("MainWindow > Input");
    cx.bind_keys([
        KeyBinding::new("ctrl-u", ScrollHalfPageUp, CONTEXT),
        KeyBinding::new("ctrl-d", ScrollHalfPageDown, CONTEXT),
        KeyBinding::new("g", ScrollToTop, CONTEXT),
        KeyBinding::new("shift-g", ScrollToBottom, CONTEXT),
        KeyBinding::new("escape", LeaveInput, INPUT_CONTEXT),
        KeyBinding::new(
            "tab",
            AcceptSuggestion,
            Some("MainWindow && suggestion > Input"),
        ),
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
        .on_action(
            cx.listener(|this, _: &AcceptSuggestion, window, cx| {
                this.accept_suggestion(window, cx)
            }),
        )
        .on_action(cx.listener(|this, _: &LeaveInput, window, cx| {
            window.focus(&this.focus_handle(), cx);
            cx.notify();
        }))
}

/// What the store says about a session's input box as it renders.
pub struct InputFacts<'a> {
    /// Whether the session takes a message now; a refusal is shown by the box.
    pub can_send: Result<(), SendRefusal>,
    /// The working indicator's line, shown just above the box while the session runs.
    pub working: Option<String>,
    /// The Agent's guess at the next message, which fills an empty box.
    pub suggestion: Option<&'a str>,
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
    input: Entity<TextareaState>,
    /// Keeps the input's events (enter, focus, edits) flowing to the window while the pane
    /// lives.
    _input_events: Subscription,
    /// The placeholder the input shows: the invitation to type, the Agent's prompt
    /// suggestion, or why sending is refused.
    placeholder: SharedString,
    /// Indices of the tool calls the user has expanded.
    expanded: HashSet<usize>,
    /// Whether the list was told to follow its tail, which it does for a running session.
    follows_tail: bool,
    /// How many Transcript items the list has rows for.
    items: usize,
}

impl TranscriptViews {
    /// Render `session`'s Transcript in `pane`: the list above, the input box below.
    ///
    /// While the session runs, the list follows new items; once the user scrolls up, a pill
    /// over the bottom of the list offers to jump back to the latest. `input` is what the
    /// store says about the input box.
    pub fn render(
        &mut self,
        pane: PaneId,
        session: &Session,
        input: InputFacts<'_>,
        window: &mut Window,
        cx: &mut Context<MainWindow>,
    ) -> impl IntoElement + use<> {
        let InputFacts {
            can_send,
            working,
            suggestion,
        } = input;
        let transcript = &session.transcript;
        let streaming = session.status == SessionStatus::Running;
        let state = self.sync(
            pane,
            session.id,
            transcript.items().len(),
            streaming,
            window,
            cx,
        );
        let placeholder = match can_send {
            Ok(()) => suggestion.unwrap_or(INPUT_PLACEHOLDER),
            Err(refusal) => refusal.reason(),
        };
        if state.placeholder != placeholder {
            state.placeholder = SharedString::from(placeholder.to_owned());
            state.input.update(cx, |input, cx| {
                input.set_placeholder(state.placeholder.clone(), window, cx)
            });
        }
        let notice = can_send.err().filter(|_| working.is_none()).map(|refusal| {
            div()
                .px_1()
                .pb_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(refusal.reason())
        });
        let disabled = can_send == Err(SendRefusal::SessionDone);
        let paused = streaming && !state.list.is_following_tail() && !last_item_shown(&state.list);
        let items = transcript.items().to_vec();
        let expanded = state.expanded.clone();
        let this = cx.weak_entity();
        let list_state = state.list.clone();
        let list_for_scrollbar = list_state.clone();

        let list = list(list_state, move |ix, _, cx| {
            let Some(item) = items.get(ix) else {
                return div().into_any_element();
            };
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
                v_flex()
                    .id(pane_element_id("input", pane))
                    .flex_shrink_0()
                    .p_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .children(working.map(|line| render_working(pane, line, cx)))
                    .children(notice)
                    .child(Textarea::new(&state.input).disabled(disabled)),
            )
    }

    /// What the user has typed into `pane`'s input box.
    pub fn input_text(&self, pane: PaneId, cx: &App) -> String {
        self.panes
            .get(&pane)
            .map(|state| state.input.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// Whether `pane`'s input box holds no text at all.
    pub fn input_is_empty(&self, pane: PaneId, cx: &App) -> bool {
        self.panes
            .get(&pane)
            .is_none_or(|state| state.input.read(cx).value().is_empty())
    }

    /// Make `text` the content of `pane`'s empty input box, with the cursor at its end. A box
    /// that holds text keeps it.
    pub fn accept_suggestion(
        &mut self,
        pane: PaneId,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(state) = self.panes.get(&pane) {
            state.input.update(cx, |input, cx| {
                if input.value().is_empty() {
                    input.set_value(text.to_owned(), window, cx);
                    input.set_selected_range(text.len()..text.len(), cx);
                }
            });
        }
    }

    /// Empty `pane`'s input box, once its text was sent.
    pub fn clear_input(&mut self, pane: PaneId, window: &mut Window, cx: &mut App) {
        if let Some(state) = self.panes.get(&pane) {
            state.input.update(cx, |input, cx| input.clean(window, cx));
        }
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
    ) -> &mut PaneTranscript {
        let mut fresh = false;
        let state = self.panes.entry(pane).or_insert_with(|| {
            fresh = true;
            let input = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(INPUT_PLACEHOLDER)
                    .submit_on_enter(true)
                    .auto_grow(1, INPUT_MAX_ROWS)
            });
            let input_events = cx.subscribe_in(
                &input,
                window,
                move |this, _, event, window, cx| match event {
                    InputEvent::PressEnter { shift: false, .. } => {
                        this.send_from_input(pane, window, cx)
                    }
                    InputEvent::Focus => this.input_focused(pane, cx),
                    // Emptiness decides whether `tab` accepts a suggestion (SUGGESTION_FLAG).
                    InputEvent::Change => cx.notify(),
                    _ => {}
                },
            );
            PaneTranscript {
                session,
                list: ListState::new(item_count, ListAlignment::Top, LIST_OVERDRAW),
                input,
                _input_events: input_events,
                placeholder: INPUT_PLACEHOLDER.into(),
                expanded: HashSet::new(),
                follows_tail: false,
                items: item_count,
            }
        });
        if state.session != session {
            fresh = true;
            state.session = session;
            state.expanded.clear();
            state.list.reset(item_count);
        } else if item_count != state.items {
            let kept = state.items.min(item_count);
            state.list.splice(kept..state.items, item_count - kept);
        }
        state.items = item_count;
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

/// Whether any part of the list's last item was on screen at the last layout.
///
/// The "Jump to latest" pill shows only when the user scrolled up far enough to lose it: a
/// paused list whose last item is still in view needs no way back to it.
fn last_item_shown(list: &ListState) -> bool {
    let Some(last) = list.item_count().checked_sub(1) else {
        return true;
    };
    let viewport = list.viewport_bounds();
    list.bounds_for_item(last)
        .is_some_and(|item| item.top() < viewport.bottom() && item.bottom() > viewport.top())
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

/// The working indicator: an orange spinner, then `line` in muted text.
fn render_working(pane: PaneId, line: String, cx: &App) -> impl IntoElement {
    h_flex()
        .px_1()
        .pb_2()
        .gap_2()
        .text_color(cx.theme().muted_foreground)
        .child(render_spinner(pane))
        .child(line)
}

/// Three orange dots on a diagonal, a bright one sweeping across them and fading behind.
fn render_spinner(pane: PaneId) -> impl IntoElement {
    let dot = |ix: usize| {
        let lead = ix as f32 / SPINNER_DOTS as f32;
        div()
            .size(px(4.))
            .rounded_full()
            .bg(gpui_kit::component::theme::orange_500())
            .with_animation(
                item_element_id(pane, ix, "working-dot"),
                Animation::new(SPINNER_PERIOD)
                    .repeat_synced()
                    .with_max_fps(SPINNER_FPS),
                move |dot, delta| {
                    let since_lit = (delta - lead).rem_euclid(1.);
                    dot.opacity(0.25 + 0.75 * (1. - since_lit))
                },
            )
    };
    div()
        .relative()
        .size(px(12.))
        .flex_shrink_0()
        .children((0..SPINNER_DOTS).map(|ix| {
            let offset = px(4. * ix as f32);
            div().absolute().left(offset).top(offset).child(dot(ix))
        }))
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
