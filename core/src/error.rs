/// Errors from framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    /// Frame has no bytes.
    #[error("frame is empty")]
    Empty,
    /// Version byte is not supported.
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u8),
    /// Payload length exceeds the permitted maximum.
    #[error("payload length {len} exceeds maximum {max}")]
    TooLarge {
        /// Rejected payload length in bytes.
        len: u64,
        /// Maximum allowed payload length in bytes.
        max: u64,
    },
    /// Stream ended before a complete header or payload arrived.
    #[error("stream ended with an incomplete frame")]
    Truncated,
}
