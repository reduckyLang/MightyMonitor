//! Deterministic platform implementations for tests and examples.
//!
//! Enable the `mock` feature to use these synthetic frames and in-memory input
//! records. The reversible codec is for pipeline testing, not video compression.

use crate::{Bytes, Capture, Decoder, Encoder, InputInjector};

/// Finite synthetic capture source with a changing byte pattern per frame.
#[derive(Debug)]
pub struct MockCapture {
    remaining: usize,
    frame_len: usize,
    pattern: u8,
}

impl MockCapture {
    /// Produce `frame_count` frames, each containing `frame_len` bytes.
    pub fn new(frame_count: usize, frame_len: usize) -> Self {
        Self {
            remaining: frame_count,
            frame_len,
            pattern: 0,
        }
    }
}

impl Capture for MockCapture {
    fn next_frame(&mut self) -> Option<Bytes> {
        if self.remaining == 0 {
            return None;
        }
        let frame = (0..self.frame_len)
            .map(|index| (index as u8).wrapping_add(self.pattern))
            .collect();
        self.remaining -= 1;
        self.pattern = self.pattern.wrapping_add(1);
        Some(frame)
    }
}

/// Reversible XOR encoder that records the most recently requested bitrate.
#[derive(Debug, Default)]
pub struct MockEncoder {
    bitrate_kbps: u32,
}

impl MockEncoder {
    /// Return the last bitrate passed to [`Encoder::set_bitrate_kbps`], or zero.
    pub fn bitrate_kbps(&self) -> u32 {
        self.bitrate_kbps
    }
}

impl Encoder for MockEncoder {
    fn encode(&mut self, raw: &[u8]) -> Result<Bytes, crate::CodecError> {
        Ok(raw.iter().map(|byte| byte ^ 0xa5).collect())
    }

    fn set_bitrate_kbps(&mut self, kbps: u32) -> Result<(), crate::CodecError> {
        self.bitrate_kbps = kbps;
        Ok(())
    }
}

/// Decoder for the reversible XOR format produced by [`MockEncoder`].
#[derive(Debug, Default)]
pub struct MockDecoder;

impl Decoder for MockDecoder {
    fn decode(&mut self, encoded: &[u8]) -> Result<Bytes, crate::CodecError> {
        Ok(encoded.iter().map(|byte| byte ^ 0xa5).collect())
    }
}

/// Records copies of serialized input events in injection order.
#[derive(Debug, Default)]
pub struct MockInputInjector {
    events: Vec<Bytes>,
}

impl MockInputInjector {
    /// Return the recorded events without consuming them.
    pub fn events(&self) -> &[Bytes] {
        &self.events
    }
}

impl InputInjector for MockInputInjector {
    fn inject(&mut self, event: &[u8]) {
        self.events.push(event.to_vec());
    }
}
