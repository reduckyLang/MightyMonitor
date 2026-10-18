//! MightyMonitor core: protocol framing and platform traits (skeleton).

/// Adaptive bitrate control from receiver feedback.
pub mod abr;
pub mod error;
/// Length-prefixed framing for byte streams.
pub mod frame;
pub mod traits;

pub use abr::{adapt, AimdConfig, AimdController, BitrateController, NetStats};
pub use error::FrameError;
/// Asynchronous frame transports.
pub mod transport;

/// Reconnection policy and session lifecycle.
pub mod reconnect;

pub use reconnect::{
    Backoff, BackoffPolicy, Connector, JitterSource, ReconnectingSession, ResumePayloadError,
    ResumeRequest, SendOutcome, SessionError, SessionEvent, SessionState, SplitMix64, TcpConnector,
    KIND_RESUME,
};
pub use transport::{TcpTransport, Transport, TransportError};

pub use frame::{
    decode_frame, encode_frame, encode_frame_into, encode_stream_frame, Frame, FrameDecoder,
    FrameHeader, HEADER_LEN, MAX_PAYLOAD_LEN, PROTOCOL_VERSION,
};
pub use traits::{Bytes, Capture, Decoder, Encoder, InputInjector};
