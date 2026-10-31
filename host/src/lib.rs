//! Host capture, encoding and bounded TCP streaming.
#![forbid(unsafe_code)]
pub mod abr;
pub mod input;
pub mod pipeline;
pub mod resume;
pub mod session;

/// Fallible host operations, including platform backend errors.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub mod pairing;
