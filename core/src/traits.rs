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
    /// Must be callable mid-stream.
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
