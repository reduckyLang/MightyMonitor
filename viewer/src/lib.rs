//! Desktop viewer with an independently testable network session.
#![forbid(unsafe_code)]
#[cfg(feature = "gui")]
pub mod app;
#[cfg(feature = "gui")]
pub mod host;
#[cfg(feature = "gui")]
pub mod input;
pub mod net;
pub mod render;

pub mod devices;
pub mod ui;
