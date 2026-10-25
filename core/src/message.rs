//! Typed protocol payloads, independent of stream framing.

use crate::{frame::MAX_PAYLOAD_LEN, input::InputEvent};
use serde::{Deserialize, Serialize};

// Bincode 2 serde's legacy configuration preserves the prior fixed-int,
// little-endian payload layout; size and trailing-byte checks are protocol rules.

/// Never assigned to messages; reserved for protocol extension/escape.
pub const KIND_RESERVED_FE: u8 = 0xfe;
/// Never assigned to messages; reserved for protocol extension/escape.
pub const KIND_RESERVED_FF: u8 = 0xff;

/// Role advertised during the handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Host,
    Viewer,
}

/// Video codec selected for a stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Codec {
    H264,
    Vp8,
    Vp9,
    Av1,
    Raw,
}

/// Receiver network measurements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetStats {
    pub rtt_ms: u32,
    pub loss_pct: f32,
    pub recv_kbps: u32,
}

/// A typed frame payload. Variant order is part of the wire format.
///
/// Non-exhaustive for source compatibility; unknown wire variants are rejected.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Message {
    Hello {
        proto_version: u8,
        device_name: String,
        role: Role,
        capabilities: Vec<String>,
    },
    HelloAck {
        session_id: u64,
        accepted: bool,
    },
    VideoConfig {
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
    },
    VideoFrame {
        seq: u64,
        timestamp_us: u64,
        keyframe: bool,
        data: Vec<u8>,
    },
    Input(InputEvent),
    Ping {
        nonce: u64,
        t_send_us: u64,
    },
    Pong {
        nonce: u64,
        t_send_us: u64,
        t_peer_recv_us: u64,
        t_peer_send_us: u64,
    },
    NetStats(NetStats),
    Resume {
        session_id: u64,
        last_seq: u64,
    },
    Bye {
        reason: String,
    },
    /// Sent only to viewers advertising input-v1. Original wire variants retain their indices.
    HelloAckV2 {
        session_id: u64,
        accepted: bool,
        control_available: bool,
    },
    ControlRequest {
        enabled: bool,
    },
    ControlStatus {
        enabled: bool,
    },
    KeyframeRequest,
}

/// Calculate network round-trip time from a Ping/Pong timestamp exchange.
/// Saturating arithmetic keeps malformed or reversed timestamps from wrapping.
pub fn pong_rtt_us(
    t_send_us: u64,
    t_peer_recv_us: u64,
    t_peer_send_us: u64,
    t_recv_us: u64,
) -> u64 {
    t_recv_us
        .saturating_sub(t_send_us)
        .saturating_sub(t_peer_send_us.saturating_sub(t_peer_recv_us))
}

/// Serialization or decoding failure.
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("reserved message kind: {0:#04x}")]
    ReservedKind(u8),
    #[error("unknown message kind: {0:#04x}")]
    UnknownKind(u8),
    #[error("frame kind {kind} does not match decoded message kind {decoded_kind}")]
    KindMismatch { kind: u8, decoded_kind: u8 },
    #[error("message encode error: {0}")]
    Encode(#[from] bincode::error::EncodeError),
    #[error("message decode error: {0}")]
    Decode(#[from] bincode::error::DecodeError),
    #[error("message payload contains trailing bytes")]
    TrailingBytes,
    #[error("message payload exceeds the maximum size")]
    PayloadTooLarge,
}

impl Message {
    /// Stable wire kinds: Hello=1, HelloAck=2, VideoConfig=3, VideoFrame=4,
    /// Input=5, Ping=6, Pong=7, NetStats=8, Resume=9, Bye=10,
    /// HelloAckV2=11, ControlRequest=12, ControlStatus=13, KeyframeRequest=14.
    pub fn kind(&self) -> u8 {
        match self {
            Self::Hello { .. } => 1,
            Self::HelloAck { .. } => 2,
            Self::VideoConfig { .. } => 3,
            Self::VideoFrame { .. } => 4,
            Self::Input(_) => 5,
            Self::Ping { .. } => 6,
            Self::Pong { .. } => 7,
            Self::NetStats(_) => 8,
            Self::Resume { .. } => 9,
            Self::Bye { .. } => 10,
            Self::HelloAckV2 { .. } => 11,
            Self::ControlRequest { .. } => 12,
            Self::ControlStatus { .. } => 13,
            Self::KeyframeRequest => 14,
        }
    }

    /// Encode the kind and typed payload for the frame transport API.
    pub fn to_frame_parts(&self) -> Result<(u8, Vec<u8>), MessageError> {
        Ok((self.kind(), self.to_bytes()?))
    }

    /// Decode a typed payload and verify that its variant agrees with the frame kind.
    pub fn from_frame_parts(kind: u8, payload: &[u8]) -> Result<Self, MessageError> {
        match kind {
            KIND_RESERVED_FE | KIND_RESERVED_FF => return Err(MessageError::ReservedKind(kind)),
            1..=14 => {}
            _ => return Err(MessageError::UnknownKind(kind)),
        }
        let message = Self::from_bytes(payload)?;
        let decoded_kind = message.kind();
        if kind != decoded_kind {
            return Err(MessageError::KindMismatch { kind, decoded_kind });
        }
        Ok(message)
    }

    /// Serialize a payload without a frame header or version prefix.
    pub fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
        let config = bincode::config::legacy().with_limit::<MAX_PAYLOAD_LEN>();
        Ok(bincode::serde::encode_to_vec(self, config)?)
    }

    /// Decode exactly one payload with a bounded bincode byte budget.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MessageError> {
        if bytes.len() > MAX_PAYLOAD_LEN {
            return Err(MessageError::PayloadTooLarge);
        }
        let config = bincode::config::legacy().with_limit::<MAX_PAYLOAD_LEN>();
        let (message, consumed) = bincode::serde::decode_from_slice(bytes, config)?;
        if consumed != bytes.len() {
            return Err(MessageError::TrailingBytes);
        }
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TimeSource;
    use std::cell::Cell;

    fn hello() -> Message {
        Message::Hello {
            proto_version: crate::PROTOCOL_VERSION,
            device_name: "Desktop".into(),
            role: Role::Host,
            capabilities: vec!["h264".into(), "input".into()],
        }
    }

    #[test]
    fn every_message_roundtrips_bincode() {
        let messages = [
            hello(),
            Message::HelloAck {
                session_id: 42,
                accepted: true,
            },
            Message::VideoConfig {
                codec: Codec::H264,
                width: 1920,
                height: 1080,
                fps: 60,
            },
            Message::VideoFrame {
                seq: 7,
                timestamp_us: 123456,
                keyframe: true,
                data: vec![0, 1, 255],
            },
            Message::Input(InputEvent::Key {
                code: 13,
                pressed: false,
            }),
            Message::Ping {
                nonce: 1,
                t_send_us: 123,
            },
            Message::Pong {
                nonce: 1,
                t_send_us: 123,
                t_peer_recv_us: 456,
                t_peer_send_us: 789,
            },
            Message::NetStats(NetStats {
                rtt_ms: 12,
                loss_pct: 0.5,
                recv_kbps: 8000,
            }),
            Message::Resume {
                session_id: 42,
                last_seq: 7,
            },
            Message::Bye {
                reason: "finished".into(),
            },
            Message::HelloAckV2 {
                session_id: 42,
                accepted: true,
                control_available: true,
            },
            Message::ControlRequest { enabled: true },
            Message::ControlStatus { enabled: true },
            Message::KeyframeRequest,
        ];
        for (index, message) in messages.into_iter().enumerate() {
            let (kind, payload) = message.to_frame_parts().unwrap();
            assert_eq!(kind, index as u8 + 1);
            assert_eq!(Message::from_frame_parts(kind, &payload).unwrap(), message);
            assert_eq!(
                Message::from_bytes(&message.to_bytes().unwrap()).unwrap(),
                message
            );
        }
        for codec in [Codec::H264, Codec::Vp8, Codec::Vp9, Codec::Av1, Codec::Raw] {
            let message = Message::VideoConfig {
                codec,
                width: 1,
                height: 1,
                fps: 1,
            };
            assert_eq!(
                Message::from_bytes(&message.to_bytes().unwrap()).unwrap(),
                message
            );
        }
        let viewer = Message::Hello {
            role: Role::Viewer,
            proto_version: crate::PROTOCOL_VERSION,
            device_name: String::new(),
            capabilities: vec![],
        };
        assert_eq!(
            Message::from_bytes(&viewer.to_bytes().unwrap()).unwrap(),
            viewer
        );
    }

    #[test]
    fn golden_bytes_all_variants_v3() {
        let messages = [
            hello(),
            Message::HelloAck {
                session_id: 42,
                accepted: true,
            },
            Message::VideoConfig {
                codec: Codec::H264,
                width: 1920,
                height: 1080,
                fps: 60,
            },
            Message::VideoFrame {
                seq: 7,
                timestamp_us: 123456,
                keyframe: true,
                data: vec![0, 1, 255],
            },
            Message::Input(InputEvent::Key {
                code: 13,
                pressed: false,
            }),
            Message::Ping {
                nonce: 1,
                t_send_us: 123,
            },
            Message::Pong {
                nonce: 1,
                t_send_us: 123,
                t_peer_recv_us: 456,
                t_peer_send_us: 789,
            },
            Message::NetStats(NetStats {
                rtt_ms: 12,
                loss_pct: 0.5,
                recv_kbps: 8000,
            }),
            Message::Resume {
                session_id: 42,
                last_seq: 7,
            },
            Message::Bye {
                reason: "finished".into(),
            },
            Message::HelloAckV2 {
                session_id: 42,
                accepted: true,
                control_available: true,
            },
            Message::ControlRequest { enabled: true },
            Message::ControlStatus { enabled: true },
            Message::KeyframeRequest,
        ];
        let expected = [
            "03 01 39 00 00 00 00 00 00 00 03 07 00 00 00 00 00 00 00 44 65 73 6b 74 6f 70 00 00 00 00 02 00 00 00 00 00 00 00 04 00 00 00 00 00 00 00 68 32 36 34 05 00 00 00 00 00 00 00 69 6e 70 75 74",
            "03 02 0d 00 00 00 01 00 00 00 2a 00 00 00 00 00 00 00 01",
            "03 03 14 00 00 00 02 00 00 00 00 00 00 00 80 07 00 00 38 04 00 00 3c 00 00 00",
            "03 04 20 00 00 00 03 00 00 00 07 00 00 00 00 00 00 00 40 e2 01 00 00 00 00 00 01 03 00 00 00 00 00 00 00 00 01 ff",
            "03 05 0d 00 00 00 04 00 00 00 03 00 00 00 0d 00 00 00 00",
            "03 06 14 00 00 00 05 00 00 00 01 00 00 00 00 00 00 00 7b 00 00 00 00 00 00 00",
            "03 07 24 00 00 00 06 00 00 00 01 00 00 00 00 00 00 00 7b 00 00 00 00 00 00 00 c8 01 00 00 00 00 00 00 15 03 00 00 00 00 00 00",
            "03 08 10 00 00 00 07 00 00 00 0c 00 00 00 00 00 00 3f 40 1f 00 00",
            "03 09 14 00 00 00 08 00 00 00 2a 00 00 00 00 00 00 00 07 00 00 00 00 00 00 00",
            "03 0a 14 00 00 00 09 00 00 00 08 00 00 00 00 00 00 00 66 69 6e 69 73 68 65 64",
            "03 0b 0e 00 00 00 0a 00 00 00 2a 00 00 00 00 00 00 00 01 01",
            "03 0c 05 00 00 00 0b 00 00 00 01",
            "03 0d 05 00 00 00 0c 00 00 00 01",
            "03 0e 04 00 00 00 0d 00 00 00",
        ];
        assert_eq!(messages.len(), expected.len());
        for (message, expected) in messages.into_iter().zip(expected) {
            let (kind, payload) = message.to_frame_parts().unwrap();
            let frame = crate::frame::encode_stream_frame(kind, &payload).unwrap();
            let expected: Vec<u8> = expected
                .split_whitespace()
                .map(|byte| u8::from_str_radix(byte, 16).unwrap())
                .collect();
            assert_eq!(frame, expected, "message kind {kind}");
        }
    }

    #[test]
    fn unknown_variant_errors_not_panics() {
        let valid = hello().to_bytes().unwrap();
        let mut trailing = valid.clone();
        trailing.push(0);
        // Hello's device-name length follows its discriminant and version.
        let mut huge_string = vec![0, 0, 0, 0, 1];
        huge_string.extend_from_slice(&u64::MAX.to_le_bytes());
        // Hello's capabilities vector follows an empty name and the role.
        let mut huge_vec = vec![0, 0, 0, 0, 1];
        huge_vec.extend_from_slice(&0u64.to_le_bytes());
        huge_vec.extend_from_slice(&0u32.to_le_bytes());
        huge_vec.extend_from_slice(&u64::MAX.to_le_bytes());
        for bytes in [
            u32::MAX.to_le_bytes().to_vec(),
            vec![],
            valid[..valid.len() - 1].to_vec(),
            trailing,
            huge_string,
            huge_vec,
            vec![0; MAX_PAYLOAD_LEN + 1],
        ] {
            assert!(Message::from_bytes(&bytes).is_err());
        }
        for end in 0..valid.len() {
            assert!(Message::from_bytes(&valid[..end]).is_err());
        }
    }

    #[test]
    fn pong_roundtrip_excludes_peer_processing_time() {
        let local = Cell::new(0);
        let peer = Cell::new(1_000);
        let local_clock = || local.get();
        let peer_clock = || peer.get();

        let t_send_us = TimeSource::now_us(&local_clock);
        peer.set(peer.get() + 40_000);
        let t_peer_send_us = TimeSource::now_us(&peer_clock);
        local.set(70_000);
        let t_recv_us = TimeSource::now_us(&local_clock);

        assert_eq!(
            pong_rtt_us(t_send_us, 1_000, t_peer_send_us, t_recv_us),
            30_000
        );
    }

    #[test]
    fn serialized_size_hello_under_512_bytes() {
        assert!(hello().to_bytes().unwrap().len() < 512);
    }

    #[test]
    fn reserved_and_unknown_kinds_rejected() {
        let payload = hello().to_bytes().unwrap();
        for kind in [KIND_RESERVED_FE, KIND_RESERVED_FF] {
            assert!(
                matches!(Message::from_frame_parts(kind, &payload), Err(MessageError::ReservedKind(k)) if k == kind)
            );
            assert!(Message::from_frame_parts(kind, &[]).is_err());
        }
        for kind in [0, 15, 253] {
            assert!(
                matches!(Message::from_frame_parts(kind, &payload), Err(MessageError::UnknownKind(k)) if k == kind)
            );
        }
    }

    #[test]
    fn kind_variant_mismatch_rejected() {
        assert!(matches!(
            Message::from_frame_parts(2, &hello().to_bytes().unwrap()),
            Err(MessageError::KindMismatch {
                kind: 2,
                decoded_kind: 1
            })
        ));
    }

    #[test]
    fn fixint_little_endian_wire_format() {
        let message = Message::Ping {
            nonce: 1,
            t_send_us: 2,
        };
        let mut expected = 5u32.to_le_bytes().to_vec();
        expected.extend_from_slice(&1u64.to_le_bytes());
        expected.extend_from_slice(&2u64.to_le_bytes());
        assert_eq!(message.to_bytes().unwrap(), expected);
    }
}
