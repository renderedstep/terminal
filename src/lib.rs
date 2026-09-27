//! A terminal client for the text-adventure engine protocol v1.
//!
//! [`engine::Engine`] is the only way the interface reaches a game: the HTTP
//! implementation speaks protocol v1 to a remote engine, and the fake one
//! serves tests.

pub mod config;
pub mod engine;
pub mod line;
pub mod present;
pub mod protocol;
pub mod sse;
pub mod tui;
pub mod upgrade;
