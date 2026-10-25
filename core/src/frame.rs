//! Versioned, length-prefixed framing for TCP and other byte streams.

use crate::error::FrameError;

/// Current wire protocol version, always the first byte of a frame.
pub const PROTOCOL_VERSION: u8 = 3;

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

/// Size of a stream frame header in bytes.
pub const HEADER_LEN: usize = 6;
/// Maximum payload size (16 MiB), limiting untrusted length claims.
pub const MAX_PAYLOAD_LEN: usize = 16 * 1024 * 1024;

/// Fixed-size stream header; the version byte always comes first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    /// Protocol version.
    pub version: u8,
    /// Reserved message kind/flags byte.
    pub kind: u8,
    /// Payload size in bytes, encoded little-endian.
    pub payload_len: u32,
}

impl FrameHeader {
    /// Encode the header as supplied, without validating its fields.
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let len = self.payload_len.to_le_bytes();
        [self.version, self.kind, len[0], len[1], len[2], len[3]]
    }

    /// Decode and validate the first six bytes; trailing bytes are ignored.
    pub fn decode(buf: &[u8]) -> Result<Self, FrameError> {
        if buf.len() < HEADER_LEN {
            return Err(FrameError::Truncated);
        }
        if buf[0] != PROTOCOL_VERSION {
            return Err(FrameError::UnsupportedVersion(buf[0]));
        }
        let payload_len = u32::from_le_bytes([buf[2], buf[3], buf[4], buf[5]]);
        check_len(u64::from(payload_len))?;
        Ok(Self {
            version: buf[0],
            kind: buf[1],
            payload_len,
        })
    }
}

fn check_len(len: u64) -> Result<(), FrameError> {
    if len > MAX_PAYLOAD_LEN as u64 {
        Err(FrameError::TooLarge {
            len,
            max: MAX_PAYLOAD_LEN as u64,
        })
    } else {
        Ok(())
    }
}

/// Append a stream frame. On an oversized payload, `out` remains unchanged.
pub fn encode_frame_into(out: &mut Vec<u8>, kind: u8, payload: &[u8]) -> Result<(), FrameError> {
    check_len(payload.len() as u64)?;
    out.extend_from_slice(
        &FrameHeader {
            version: PROTOCOL_VERSION,
            kind,
            payload_len: payload.len() as u32,
        }
        .encode(),
    );
    out.extend_from_slice(payload);
    Ok(())
}

/// Encode a stream frame into a new byte vector.
pub fn encode_stream_frame(kind: u8, payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    let mut out = Vec::new();
    encode_frame_into(&mut out, kind, payload)?;
    Ok(out)
}

/// A decoded stream frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Reserved message kind/flags byte.
    pub kind: u8,
    /// Owned payload bytes.
    pub payload: Vec<u8>,
}

/// Incremental stream decoder supporting partial reads and concatenated frames.
///
/// Headers are validated during `push`, before their payload bytes are buffered.
/// An invalid header poisons the decoder: every subsequent `next_frame` and
/// `finish` returns the same error. Drop the connection after such an error.
/// Consumed bytes are compacted geometrically to avoid repeated payload copies.
#[derive(Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
    read: usize,
    scan: usize,
    pending_end: Option<usize>,
    error: Option<FrameError>,
}

impl FrameDecoder {
    /// Create an empty decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append bytes, validating each header before buffering its payload.
    /// Bytes supplied after a malformed header are ignored; `next_frame`
    /// reports the persistent error.
    pub fn push(&mut self, mut data: &[u8]) {
        if self.error.is_some() {
            return;
        }
        loop {
            if self.pending_end == Some(self.buffer.len()) {
                self.scan = self.buffer.len();
                self.pending_end = None;
            }
            if data.is_empty() {
                break;
            }
            let target = self.pending_end.unwrap_or(self.scan + HEADER_LEN);
            let count = (target - self.buffer.len()).min(data.len());
            self.buffer.extend_from_slice(&data[..count]);
            data = &data[count..];
            if self.buffer.len() < target {
                break;
            }
            if self.pending_end.is_some() {
                self.scan = target;
                self.pending_end = None;
            } else {
                match FrameHeader::decode(&self.buffer[self.scan..]) {
                    Ok(header) => self.pending_end = Some(target + header.payload_len as usize),
                    Err(error) => {
                        self.error = Some(error);
                        return;
                    }
                }
            }
        }
    }

    /// Return the next complete frame, or `None` when more bytes are needed.
    /// Once poisoned, always returns the original error.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        if let Some(error) = &self.error {
            return Err(*error);
        }
        let bytes = &self.buffer[self.read..];
        if bytes.len() < HEADER_LEN {
            return Ok(None);
        }
        // `push` already validated every complete header; any failure poisoned
        // the decoder and was returned above, so this decode cannot fail.
        let header = FrameHeader::decode(bytes)?;
        let size = HEADER_LEN + header.payload_len as usize;
        if bytes.len() < size {
            return Ok(None);
        }
        let frame = Frame {
            kind: header.kind,
            payload: bytes[HEADER_LEN..size].to_vec(),
        };
        self.read += size;
        if self.read >= self.buffer.len() / 2 {
            self.buffer.drain(..self.read);
            self.scan -= self.read;
            if let Some(end) = &mut self.pending_end {
                *end -= self.read;
            }
            self.read = 0;
        }
        Ok(Some(frame))
    }

    /// Number of buffered bytes that have not been consumed.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len() - self.read
    }

    /// Check end-of-stream, returning `Truncated` for a partial header or payload.
    /// Complete frames may remain buffered for later consumption.
    /// A poisoned decoder returns its original error.
    pub fn finish(&self) -> Result<(), FrameError> {
        if let Some(error) = &self.error {
            return Err(*error);
        }
        if self.scan == self.buffer.len() && self.pending_end.is_none() {
            Ok(())
        } else {
            Err(FrameError::Truncated)
        }
    }
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

    fn drain(decoder: &mut FrameDecoder, out: &mut Vec<Frame>) {
        while let Some(frame) = decoder.next_frame().unwrap() {
            out.push(frame);
        }
    }

    #[test]
    fn roundtrip_many_sizes() {
        for (size, kind) in [(0, 0), (1, 1), (1024, 127), (1024 * 1024, 255)] {
            let payload = vec![42; size];
            let bytes = encode_stream_frame(kind, &payload).unwrap();
            let header = FrameHeader::decode(&bytes).unwrap();
            assert_eq!(header.encode(), bytes[..HEADER_LEN]);
            let mut decoder = FrameDecoder::new();
            decoder.push(&bytes);
            assert_eq!(decoder.finish(), Ok(()));
            assert_eq!(decoder.next_frame(), Ok(Some(Frame { kind, payload })));
            assert_eq!(decoder.buffered_len(), 0);
            assert_eq!(decoder.next_frame(), Ok(None));
        }
    }

    #[test]
    fn payload_length_boundary() {
        let mut payload = vec![42; MAX_PAYLOAD_LEN];
        let bytes = encode_stream_frame(9, &payload).unwrap();
        assert_eq!(bytes.len(), HEADER_LEN + MAX_PAYLOAD_LEN);
        let mut decoder = FrameDecoder::new();
        decoder.push(&bytes);
        assert_eq!(decoder.finish(), Ok(()));
        assert_eq!(
            decoder.next_frame(),
            Ok(Some(Frame {
                kind: 9,
                payload: payload.clone(),
            }))
        );
        assert_eq!(decoder.next_frame(), Ok(None));
        assert_eq!(decoder.buffered_len(), 0);

        payload.push(42);
        assert_eq!(
            encode_stream_frame(9, &payload),
            Err(FrameError::TooLarge {
                len: MAX_PAYLOAD_LEN as u64 + 1,
                max: MAX_PAYLOAD_LEN as u64,
            })
        );
    }

    #[test]
    fn decoder_handles_byte_by_byte_input() {
        let payload = vec![7; 1024 * 1024];
        let bytes = encode_stream_frame(9, &payload).unwrap();
        let mut decoder = FrameDecoder::new();
        for byte in &bytes[..bytes.len() - 1] {
            decoder.push(&[*byte]);
            assert_eq!(decoder.next_frame(), Ok(None));
        }
        decoder.push(&bytes[bytes.len() - 1..]);
        assert_eq!(decoder.next_frame(), Ok(Some(Frame { kind: 9, payload })));
        assert_eq!(decoder.finish(), Ok(()));
    }

    #[test]
    fn decoder_handles_concatenated_frames() {
        let expected: Vec<_> = (0..5)
            .map(|kind| Frame {
                kind,
                payload: vec![kind; kind as usize],
            })
            .collect();
        let mut bytes = Vec::new();
        for frame in &expected {
            encode_frame_into(&mut bytes, frame.kind, &frame.payload).unwrap();
        }
        for split in [0, 1, 6, 9, bytes.len()] {
            let mut decoder = FrameDecoder::new();
            let mut actual = Vec::new();
            decoder.push(&bytes[..split]);
            drain(&mut decoder, &mut actual);
            decoder.push(&bytes[split..]);
            drain(&mut decoder, &mut actual);
            assert_eq!(actual, expected);
            assert_eq!(decoder.finish(), Ok(()));
        }
    }

    #[test]
    fn rejects_oversize_len() {
        let header = FrameHeader {
            version: PROTOCOL_VERSION,
            kind: 0,
            payload_len: MAX_PAYLOAD_LEN as u32 + 1,
        }
        .encode();
        let expected = FrameError::TooLarge {
            len: MAX_PAYLOAD_LEN as u64 + 1,
            max: MAX_PAYLOAD_LEN as u64,
        };
        let mut decoder = FrameDecoder::new();
        decoder.push(&header);
        assert_eq!(decoder.next_frame(), Err(expected));
        decoder.push(&[0; 100]);
        assert_eq!(decoder.buffered_len(), HEADER_LEN);
        let mut out = vec![99];
        assert_eq!(
            encode_frame_into(&mut out, 0, &vec![0; MAX_PAYLOAD_LEN + 1]),
            Err(expected)
        );
        assert_eq!(out, [99]);
    }

    #[test]
    fn rejects_bad_version() {
        let mut bytes = encode_stream_frame(0, b"hi").unwrap();
        bytes[0] = 99;
        assert_eq!(
            FrameHeader::decode(&bytes),
            Err(FrameError::UnsupportedVersion(99))
        );
        let mut decoder = FrameDecoder::new();
        decoder.push(&bytes);
        assert_eq!(
            decoder.next_frame(),
            Err(FrameError::UnsupportedVersion(99))
        );
    }

    #[test]
    fn truncated_reports_need_more() {
        let bytes = encode_stream_frame(0, b"hi").unwrap();
        let mut decoder = FrameDecoder::new();
        assert_eq!(decoder.finish(), Ok(()));
        for byte in &bytes[..bytes.len() - 1] {
            decoder.push(&[*byte]);
            assert_eq!(decoder.next_frame(), Ok(None));
            assert_eq!(decoder.finish(), Err(FrameError::Truncated));
        }
        assert_eq!(FrameHeader::decode(&bytes[..5]), Err(FrameError::Truncated));
        decoder.push(&bytes[bytes.len() - 1..]);
        decoder.next_frame().unwrap().unwrap();
        assert_eq!(decoder.finish(), Ok(()));
    }

    #[test]
    fn poisoned_decoder_stays_errored() {
        let mut decoder = FrameDecoder::new();
        decoder.push(&[99, 0, 0, 0, 0, 0]);
        for _ in 0..3 {
            decoder.push(&encode_stream_frame(0, b"valid").unwrap());
            assert_eq!(
                decoder.next_frame(),
                Err(FrameError::UnsupportedVersion(99))
            );
            assert_eq!(decoder.finish(), Err(FrameError::UnsupportedVersion(99)));
        }
    }

    #[test]
    fn fuzz_lite_random_chunking() {
        let mut seed = 0x12345678u32;
        let mut random = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..200 {
            let mut expected = Vec::new();
            let mut bytes = Vec::new();
            for _ in 0..random() % 20 {
                let kind = random() as u8;
                let payload: Vec<_> = (0..random() % 512).map(|_| random() as u8).collect();
                encode_frame_into(&mut bytes, kind, &payload).unwrap();
                expected.push(Frame { kind, payload });
            }
            let mut decoder = FrameDecoder::new();
            let mut actual = Vec::new();
            let mut offset = 0;
            while offset < bytes.len() {
                let end = (offset + 1 + random() as usize % 97).min(bytes.len());
                decoder.push(&bytes[offset..end]);
                drain(&mut decoder, &mut actual);
                offset = end;
            }
            assert_eq!(actual, expected);
            assert_eq!(decoder.finish(), Ok(()));
            let garbage: Vec<_> = (0..random() % 2048).map(|_| random() as u8).collect();
            let mut decoder = FrameDecoder::new();
            let mut offset = 0;
            while offset < garbage.len() {
                let end = (offset + 1 + random() as usize % 31).min(garbage.len());
                decoder.push(&garbage[offset..end]);
                while let Ok(Some(_)) = decoder.next_frame() {}
                offset = end;
            }
            let _ = decoder.finish();
        }
    }
}
