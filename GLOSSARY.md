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
A tile in the window that shows at most one session. An empty pane offers to open an existing session or start a new one. Closing a pane hides its session; it does not end it.
_Avoid_: Tile, view, split

**Project**:
The folder a session works in, whether or not it is a git repository. A project can have many sessions.
_Avoid_: Workspace, repo, working directory, folder

**Session status**:
Where a session stands right now: running, waiting for input, or done.
_Avoid_: State

**Turn**:
One message from the user and the Agent's work on it, up to its result. A session waits for input between turns.
_Avoid_: Exchange, round, request

**Error item**:
The Transcript item that explains why a turn failed or why the session ended.
_Avoid_: Error message, failure entry, crash note

**Pending tool call**:
A tool call in a Transcript whose result has not arrived yet. It is shown collapsed with a pending marker.
_Avoid_: In-flight call, running tool, incomplete tool call

**Prompt suggestion**:
The Agent's guess at the user's next message, made after a turn. It is offered in the input box, selected, and only while the session waits for that message.
_Avoid_: Prediction, autocomplete, hint
