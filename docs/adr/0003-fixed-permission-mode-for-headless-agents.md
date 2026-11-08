# Start every Agent with a fixed permission mode

Headless Claude Code (`claude -p`) cannot show its approval prompts, so any tool use that needs approval is denied and a session would be useless. For Milestone 2 every Agent is started with `--permission-mode acceptEdits`, the same for all Projects.

## Considered Options

- A per-Project allowlist (`--allowedTools`) needs configuration UI that M2 has no room for.
- Real approve/deny prompts in the pane via `--permission-prompt-tool` are the "direct the work" ideal, but a milestone of their own.

## Consequences

File reads and edits are auto-approved. Bash still needs approval and is denied in M2, which is acceptable for a first slice: the Agent can read and change code but not run it. The follow-up to replace the fixed mode with prompts in the pane is tracked in a separate issue under #18.
