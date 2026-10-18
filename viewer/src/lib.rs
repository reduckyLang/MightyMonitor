//! Desktop viewer with an independently testable network session.
#![forbid(unsafe_code)]
#[cfg(feature = "gui")]
pub mod app;
pub mod net;
pub mod render;
