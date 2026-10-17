//! Formatting and standard error support for framing errors.

use crate::FrameError;

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "frame is empty"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported protocol version {v}"),
            Self::TooLarge { len, max } => write!(f, "payload length {len} exceeds maximum {max}"),
            Self::Truncated => write!(f, "stream ended with an incomplete frame"),
        }
    }
}

impl std::error::Error for FrameError {}
