//! MightyMonitor core: protocol framing and platform traits (skeleton).

/// Length-prefixed framing for byte streams.
pub mod frame;

/// Asynchronous frame transports.
pub mod transport;

pub use transport::{TcpTransport, Transport, TransportError};

pub use frame::{
    encode_frame_into, encode_stream_frame, Frame, FrameDecoder, FrameHeader, HEADER_LEN,
    MAX_PAYLOAD_LEN,
};

/// Current wire protocol version, always the first byte of a frame.
pub const PROTOCOL_VERSION: u8 = 1;

/// Errors from framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// Frame has no bytes.
    Empty,
    /// Version byte is not supported.
    UnsupportedVersion(u8),
    /// Payload length exceeds the permitted maximum.
    TooLarge {
        /// Rejected payload length in bytes.
        len: u64,
        /// Maximum allowed payload length in bytes.
        max: u64,
    },
    /// Stream ended before a complete header or payload arrived.
    Truncated,
}

/// Prefix a payload with the protocol version byte.
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 1);
    out.push(PROTOCOL_VERSION);
    out.extend_from_slice(payload);
    out
}

/// Check the version byte and return the payload.
pub fn decode_frame(frame: &[u8]) -> Result<&[u8], FrameError> {
    match frame.split_first() {
        None => Err(FrameError::Empty),
        Some((&PROTOCOL_VERSION, rest)) => Ok(rest),
        Some((&v, _)) => Err(FrameError::UnsupportedVersion(v)),
    }
}

/// Raw encoded or captured frame bytes.
pub type Bytes = Vec<u8>;

/// Screen capture source.
pub trait Capture {
    /// Grab the next raw frame.
    fn next_frame(&mut self) -> Option<Bytes>;
}

/// Video encoder (e.g. H.264).
pub trait Encoder {
    /// Encode a raw frame.
    fn encode(&mut self, raw: &[u8]) -> Bytes;
    /// Adapt target bitrate in kbit/s (adaptive bitrate).
    fn set_bitrate_kbps(&mut self, kbps: u32);
}

/// Video decoder.
pub trait Decoder {
    /// Decode an encoded frame.
    fn decode(&mut self, encoded: &[u8]) -> Bytes;
}

/// Injects remote input events on the host.
pub trait InputInjector {
    /// Inject a serialized input event.
    fn inject(&mut self, event: &[u8]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_byte_first() {
        let f = encode_frame(b"hi");
        assert_eq!(f[0], PROTOCOL_VERSION);
        assert_eq!(decode_frame(&f), Ok(&b"hi"[..]));
    }

    #[test]
    fn rejects_bad_version_and_empty() {
        assert_eq!(decode_frame(&[]), Err(FrameError::Empty));
        assert_eq!(
            decode_frame(&[99, 1]),
            Err(FrameError::UnsupportedVersion(99))
        );
    }
}
