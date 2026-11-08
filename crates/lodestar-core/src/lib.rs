//! Lodestar's core: the window model and sessions, in plain Rust.
//!
//! Nothing here depends on GPUI. The window model is the one test seam for layout and
//! session-placement rules; the views only render it and send it commands.

pub mod geometry;
pub mod session;
pub mod sidebar;
pub mod window;
