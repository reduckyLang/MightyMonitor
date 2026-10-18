//! MightyMonitor core: protocol framing and platform traits (skeleton).

/// Adaptive bitrate control from receiver feedback.
pub mod abr;
pub mod error;
pub mod input;
pub mod message;
#[cfg(feature = "mock")]
pub mod mock;

pub use input::{InputEvent, MouseButton, TouchPhase};
pub use message::{
    Codec, Message, MessageError, NetStats, Role, KIND_RESERVED_FE, KIND_RESERVED_FF,
};
/// Length-prefixed framing for byte streams.
pub mod frame;
pub mod traits;

pub use abr::{adapt, AimdConfig, AimdController, BitrateController};
pub use error::{CodecError, FrameError};
/// Asynchronous frame transports.
pub mod transport;

/// Reconnection policy and session lifecycle.
pub mod reconnect;

pub use reconnect::{
    Backoff, BackoffPolicy, Connector, JitterSource, ReconnectingSession, SendOutcome,
    SessionError, SessionEvent, SessionReader, SessionState, SessionWriter, SplitMix64,
    TcpConnector,
};
pub use transport::{TcpTransport, Transport, TransportError};

pub use frame::{
    decode_frame, encode_frame, encode_frame_into, encode_stream_frame, Frame, FrameDecoder,
    FrameHeader, HEADER_LEN, MAX_PAYLOAD_LEN, PROTOCOL_VERSION,
};
pub use traits::{Bytes, Capture, Decoder, Encoder, InputInjector};

/// Monotonic timestamps and clock offset estimation.
pub mod clock;
pub use clock::{MonotonicClock, OffsetEstimate, OffsetEstimator, TimeSource};
