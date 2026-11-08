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

- **Killed from outside.** Start a session and send a message. In a terminal, `pkill -TERM -f 'claude -p'` (or `kill -9` one pid from `pgrep -f 'claude -p'`). The pane ends with an Error item naming the exit ("claude exited (code 143)." or "claude was killed (signal 9).") and the session's status is done; the sidebar dot goes muted. Kill it while a tool call is still pending (for instance a long `sleep` it was asked to run) and that call's output reads "No result: the Agent exited."
- **Garbage on stdout.** Point `LODESTAR_CLAUDE_BIN` at a shell script that echoes a non-JSON line, then runs the real `claude "$@"`. The turn still arrives; nothing is shown for the stray line.
- **Failed turn.** Point `LODESTAR_CLAUDE_BIN` at a script that runs `claude --max-turns 1 "$@"` and ask for something that needs a tool and then an answer. The turn ends with an Error item "The turn failed (error_max_turns)." and the session waits for input again; another message starts a new turn.
- **Denied tool.** Ask the Agent to run a shell command (Bash is denied under ADR 0003). The call shows one output, "Error: Claude requested permissions …", and the turn ends waiting for input.
- **Missing binary.** `LODESTAR_CLAUDE_BIN=no-such-claude cargo run`, start a session. The new session's Transcript holds one Error item: "Couldn't start no-such-claude: not found on PATH. Install Claude Code or set LODESTAR_CLAUDE_BIN." and its status is done.
- **Missing Project.** Start a session in a folder, delete the folder, start another session there. The Error item reads "The Project <path> no longer exists on disk."
- **No orphans on quit.** Start two or three sessions, check `pgrep -f 'claude -p'` lists them, then close the window (the last window quits the app). `pgrep -f 'claude -p'` lists nothing. Note: ctrl-c on `cargo run` does not run the quit hook. An idle `claude` still exits by itself about half a second after its stdin closes, which happens when Lodestar dies; one mid-turn may linger until its turn ends (not verified).

## Working indicator

Start Lodestar with a real `claude`, start a session and send a message.

- [ ] While the Agent works, one muted line sits just above the input box, in place of the
      "Agent is running" notice: three orange dots
      with a bright one sweeping across them, then `12s · 340 tokens · Thinking`. It is gone
      once the reply lands and the session waits for input, and after a failed turn or an exit.
- [ ] The seconds count up every quarter second or so, from the moment the message was sent,
      and switch to `1m 05s` past a minute; the tokens grow as the reply is written
      (`1.5k tokens` past a thousand) and keep adding up across tool calls.
- [ ] The last word follows the work: `Working` while a request is sent, `Thinking`, `Writing`,
      and a tool's label (`Running Bash`, `Reading`, `Editing`, `Searching`, `Using <tool>`)
      from the call until the next request.
- [ ] The Transcript still gets one Assistant item per message, never a partial one, and a
      running pane keeps following its tail; the indicator stays put as the list scrolls.
- [ ] With `LODESTAR_FIXTURES=1`, a session running at startup shows the indicator counting
      from startup.

## Prompt suggestions

Issue #28. Start Lodestar with a real `claude`, start a session and ask for something with
an obvious next step, for instance "Show a fixed version of
`fn add(a: i32, b: i32) -> i32 { a - b }`, no tools".

- [ ] When the turn ends with a suggestion, the empty input box shows it as its muted
      placeholder in place of "Message the agent …". The box itself stays empty: `enter`
      sends nothing.
- [ ] `tab` in that box makes the suggestion its text, with the cursor at the end; `enter`
      then sends it, or edit it first.
- [ ] Typing a letter instead hides the suggestion; `tab` then indents as usual. Deleting
      everything shows the suggestion again.
- [ ] Type something while the Agent is still running. When the turn ends, the box still
      holds exactly what you typed, and `tab` does not replace it.
- [ ] Once a message is sent, the next turn shows "Agent is running" again, not the old
      suggestion; with the session in two panes, sending from one clears the other's too.
- [ ] A turn that ends without a suggestion (short factual answers often do) leaves the usual
      placeholder, and `tab` indents as before.
- [ ] With `LODESTAR_FIXTURES=1` nothing is ever suggested, and the boxes behave as before.
