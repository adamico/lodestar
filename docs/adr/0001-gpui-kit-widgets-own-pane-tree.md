# Use gpui-kit for widgets, own the pane tree

We depend on Longbridge's `gpui-kit` (Apache-2.0), pinned to an exact version, for widgets: markdown, virtual lists, sidebar, collapsibles, resizable dividers. Its GPUI snapshot (`gpui-pre`) becomes our GPUI. We do not use its `dock`, which is a panel-and-tabs model that conflicts with one session per pane. The pane tree is our own code, modelled on Zed's `pane_group` design but not copied from it, because Zed's `workspace` crate is GPL-3.0 and copying it would relicense Lodestar.

## Consequences

Our GPUI version moves only when gpui-kit releases. Upstream GPUI breaks its API often, so bumps are deliberate, one at a time.
