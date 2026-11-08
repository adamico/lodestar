# Manual checklist

What the GPUI side of Lodestar is checked for by hand, since the views have no automated
tests. Run with `cargo run` (real Agents) unless a step says otherwise. Each section belongs
to a group of user stories from the milestone specs; tickets add their own sections.

## Directing

Spec #18, stories 5-7. Needs the `claude` CLI on PATH (or `LODESTAR_CLAUDE_BIN`).

- [ ] Start a new session from an empty pane. Its status dot is waiting for input, the
      Transcript is empty, and nothing was sent to the Agent on its own.
- [ ] Click the input box and type a message. Typed letters go to the box only: `j`/`k`
      do not move the sidebar, `g`/`G` and `ctrl+u`/`ctrl+d` do not scroll.
- [ ] `shift+enter` inserts a newline; the box grows with the text (up to six rows).
- [ ] `enter` sends: the message appears at the end of the Transcript at once, the box
      empties, and the status dot turns running before the Agent's reply arrives.
- [ ] While the Agent is running, the box shows "Agent is running; wait for it to finish"
      above it (and as the placeholder when empty); `enter` leaves the typed text in place and
      sends nothing. Once the reply lands and the dot is waiting for input, `enter` sends.
- [ ] `enter` on an empty or whitespace-only box sends nothing.
- [ ] `escape` in the box hands the keyboard back to the window: `g`/`G` and
      `ctrl+u`/`ctrl+d` scroll the Transcript again, `ctrl+space` leader commands work.
- [ ] Clicking the input box of another pane focuses that pane (header highlight moves) and
      takes the keyboard from the sidebar if it had it.
- [ ] A session whose Agent is done (for instance a fixture session with
      `LODESTAR_FIXTURES=1`) shows a disabled box reading "Session is done".
