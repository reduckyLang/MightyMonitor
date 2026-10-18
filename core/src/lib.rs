//! MightyMonitor core: protocol framing and platform traits (skeleton).

pub mod error;
/// Length-prefixed framing for byte streams.
pub mod frame;
pub mod input;
pub mod message;
pub mod traits;

pub use error::FrameError;
/// Asynchronous frame transports.
pub mod transport;

pub use transport::{TcpTransport, Transport, TransportError};

pub use frame::{
    decode_frame, encode_frame, encode_frame_into, encode_stream_frame, Frame, FrameDecoder,
    FrameHeader, HEADER_LEN, MAX_PAYLOAD_LEN, PROTOCOL_VERSION,
};
pub use traits::{Bytes, Capture, Decoder, Encoder, InputInjector};

pub use input::{InputEvent, MouseButton, TouchPhase};
pub use message::{Codec, Message, MessageError, NetStats, Role};
