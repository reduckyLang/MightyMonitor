//! User-facing messages and error classification.

#![forbid(unsafe_code)]

/// Catalog of supported user-facing errors.
pub mod catalog;
/// Classification of standard and transport errors.
pub mod classify;
/// Catalog of interface strings and their audiences.
pub mod strings;
