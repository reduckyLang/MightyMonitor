use crate::CodecError;

/// Raw encoded or captured frame bytes.
pub type Bytes = Vec<u8>;

/// Screen capture source.
pub trait Capture {
    /// Grab the next raw frame.
    fn next_frame(&mut self) -> Option<Bytes>;
}

/// Video encoder (e.g. H.264).
pub trait Encoder {
    /// Encode a raw frame, returning backend failures to the caller.
    fn encode(&mut self, raw: &[u8]) -> Result<Bytes, CodecError>;
    /// Adapt target bitrate in kbit/s (adaptive bitrate).
    /// Must be callable mid-stream; backend rejection is returned to the caller.
    fn set_bitrate_kbps(&mut self, kbps: u32) -> Result<(), CodecError>;
}

/// Video decoder.
pub trait Decoder {
    /// Decode an encoded frame, returning backend failures to the caller.
    fn decode(&mut self, encoded: &[u8]) -> Result<Bytes, CodecError>;
}

/// Injects remote input events on the host.
pub trait InputInjector {
    /// Inject a serialized input event.
    fn inject(&mut self, event: &[u8]);
}
