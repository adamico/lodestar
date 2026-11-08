//! The Milestone 2 session source: Claude Code run headless, one Agent per session.
//!
//! `process` owns the child processes and their reader threads; `mapper` turns the
//! stream-json lines they produce into `SessionUpdate`s without touching either.

pub mod mapper;
