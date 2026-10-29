# Lodestar

A desktop app for watching and directing several coding agents at once, side by side.

## Language

**Session**:
One conversation with a coding agent, with its history and its current status. The unit the sidebar lists and a pane shows.
_Avoid_: Agent session, chat, thread

**Agent**:
The running process that drives a session.
_Avoid_: Bot, worker

**Transcript**:
The ordered messages and tool calls that make up a session's history.
_Avoid_: Conversation, log, chat history

**Pane**:
A tile in the window that shows exactly one session. Never empty. Closing a pane hides the session; it does not end it.
_Avoid_: Tile, view, split

**Session status**:
Where a session stands right now: running, waiting for input, or done.
_Avoid_: State
