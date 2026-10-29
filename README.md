# Lodestar

A native, OS-independent Rust desktop app for directing AI agents. It brings together Herder's versatility and Claude desktop's rich rendering, in a modern, Zed-like interface. No Electron and no TUI.

The agents write the code. Lodestar is for seeing and directing their work, so there is no code editor or first-class code view.

## Core principles

- UI first, plumbing later
- Split panes with several agents side by side
- Keyboard-driven navigation
- Rich previews of designs and web pages, with 3D as a minor nice-to-have
- Works on Linux, macOS and Windows (developed on Linux)

## Stack

- **UI:** [GPUI](https://www.gpui.rs/), the framework Zed is built on
- **Rich previews (later):** [wry](https://github.com/tauri-apps/wry), which embeds the OS webview with no bundled Chromium. Test WebGL early on Linux, because WebKitGTK is the weak spot.
- **Agent backend (later):** Claude Code run headless as a subprocess (`claude -p ... --output-format stream-json --verbose`)

## Build and run

On Ubuntu, install GPUI's system libraries first:

```sh
sudo apt-get install libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libx11-xcb-dev libxcb1-dev libfontconfig-dev libfreetype-dev libvulkan-dev
```

Then `cargo run -p lodestar`. It runs under X11 and Wayland.

The workspace has two crates:

- `lodestar-core`: the window model and sessions, in plain Rust with no GPUI. Its tests cover the layout and focus rules.
- `lodestar`: the GPUI app. Views render the window model and send it commands. Fixture sessions live in `crates/lodestar/fixtures/`.

## Roadmap

### Milestone 1: UI mockup with fake data

- [ ] GPUI window with a sidebar listing agent sessions
- [ ] Tiling panes you can split, close and resize
- [ ] Keyboard shortcuts to move between panes and sessions
- [ ] Each pane shows a fake agent conversation

Tip: study Zed's `workspace` and `pane` crates as a reference.

### Milestone 2: Real agents

- [ ] Spawn Claude Code sessions and stream their output into panes
- [ ] Send instructions to an agent from its pane

### Milestone 3: Notifications

- [ ] Desktop notification when an agent finishes or needs input
- [ ] Status badges in the sidebar

### Milestone 4: Rich previews

- [ ] wry preview pane for HTML or design output
- [ ] Optional 3D (Three.js) support

## Open questions

- Which Herder features matter most beyond panes and keyboard navigation?
- How should agent status and progress be visualized?
