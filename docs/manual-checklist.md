# Manual checklist

Checks that need a real `claude` and a window, run by hand before a milestone is called done.

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

## Failure and shutdown

Start Lodestar (`cargo run`) with a real `claude` on PATH, or `LODESTAR_CLAUDE_BIN` pointing at one.

- **Killed from outside.** Start a session and send a message. In a terminal, `pkill -TERM -f 'claude -p'` (or `kill -9` one pid from `pgrep -f 'claude -p'`). The pane ends with an Error item naming the exit ("claude exited (code 143)." or "claude was killed (signal 9).") and the session's status is done; the sidebar dot goes muted.
- **Garbage on stdout.** Point `LODESTAR_CLAUDE_BIN` at a shell script that echoes a non-JSON line, then runs the real `claude "$@"`. The turn still arrives; nothing is shown for the stray line.
- **Missing binary.** `LODESTAR_CLAUDE_BIN=no-such-claude cargo run`, start a session. The new session's Transcript holds one Error item: "Couldn't start no-such-claude: not found on PATH. Install Claude Code or set LODESTAR_CLAUDE_BIN." and its status is done.
- **Missing Project folder.** Start a session in a folder, delete the folder, start another session there. The Error item says the Project folder does not exist.
- **No orphans on quit.** Start two or three sessions, check `pgrep -f 'claude -p'` lists them, then close the window (the last window quits the app). `pgrep -f 'claude -p'` lists nothing. Note: ctrl-c on `cargo run` does not run the quit hook; orphans after that are expected for now.
