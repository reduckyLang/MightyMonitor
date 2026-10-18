//! Linux host capture. Empty on other platforms.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

pub mod capture_x11;
pub use capture_x11::{CaptureError, CapturedFrame, FrameSource, LinuxCapture, X11Capture};

#[cfg(any(feature = "pipewire", test))]
pub mod capture_pipewire;
#[cfg(feature = "pipewire")]
pub use capture_pipewire::{PipeWireCapture, PipeWireError};
