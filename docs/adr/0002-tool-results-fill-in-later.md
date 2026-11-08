# Tool results fill in a pending tool call

Claude Code reports a tool call and its result as two separate events linked by the tool_use id, but Milestone 1 pushed `ToolCall` into the Transcript as a finished item. `ToolCall` now carries a stable `id` (Claude's tool_use id), and a new `SessionUpdate::ToolResult { session, call_id, output }` fills the output in once it arrives. Until then the call is a pending tool call and renders collapsed with a pending marker.

## Considered Options

Buffering each call until its result arrives was rejected: a long-running command would show nothing in the Transcript until it finished, which hides exactly the work the user wants to watch.

## Consequences

A `ToolResult` for an unknown `call_id` is a protocol error, not a Transcript item. The fixture source must assign ids too, so fixtures and real Agents produce the same updates.
