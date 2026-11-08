# Manual checklist

Checks that need a real `claude` and a window, run by hand before a milestone is called done.

## Failure and shutdown

Start Lodestar (`cargo run`) with a real `claude` on PATH, or `LODESTAR_CLAUDE_BIN` pointing at one.

- **Killed from outside.** Start a session and send a message. In a terminal, `pkill -TERM -f 'claude -p'` (or `kill -9` one pid from `pgrep -f 'claude -p'`). The pane ends with an Error item naming the exit ("claude exited (code 143)." or "claude was killed (signal 9).") and the session's status is done; the sidebar dot goes muted.
- **Garbage on stdout.** Point `LODESTAR_CLAUDE_BIN` at a shell script that echoes a non-JSON line, then runs the real `claude "$@"`. The turn still arrives; nothing is shown for the stray line.
- **Missing binary.** `LODESTAR_CLAUDE_BIN=no-such-claude cargo run`, start a session. The new session's Transcript holds one Error item: "Couldn't start no-such-claude: not found on PATH. Install Claude Code or set LODESTAR_CLAUDE_BIN." and its status is done.
- **Missing Project folder.** Start a session in a folder, delete the folder, start another session there. The Error item says the Project folder does not exist.
- **No orphans on quit.** Start two or three sessions, check `pgrep -f 'claude -p'` lists them, then close the window (the last window quits the app). `pgrep -f 'claude -p'` lists nothing. Note: ctrl-c on `cargo run` does not run the quit hook; orphans after that are expected for now.
