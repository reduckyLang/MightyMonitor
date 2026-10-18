//! Typed protocol payloads, independent of stream framing.

use bincode::Options;
use serde::{Deserialize, Serialize};

use crate::{frame::MAX_PAYLOAD_LEN, input::InputEvent};

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
    },
    NetStats(NetStats),
    Resume {
        session_id: u64,
        last_seq: u64,
    },
    Bye {
        reason: String,
    },
}

/// Serialization or decoding failure.
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("message codec error: {0}")]
    Bincode(#[from] bincode::Error),
}

// These settings are part of the wire format: bincode 1.3, fixed-width integers,
// little-endian, no trailing bytes. Bound both encoding and decoding to a frame.
fn options() -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .with_limit(MAX_PAYLOAD_LEN as u64)
        .reject_trailing_bytes()
}

impl Message {
    /// Serialize a payload without a frame header or version prefix.
    pub fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
        Ok(options().serialize(self)?)
    }

    /// Decode exactly one payload with a bounded bincode byte budget.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MessageError> {
        if bytes.len() > MAX_PAYLOAD_LEN {
            return Err(Box::new(bincode::ErrorKind::SizeLimit).into());
        }
        Ok(options().deserialize(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello() -> Message {
        Message::Hello {
            proto_version: 1,
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
        ];
        for message in messages {
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
            proto_version: 1,
            device_name: String::new(),
            capabilities: vec![],
        };
        assert_eq!(
            Message::from_bytes(&viewer.to_bytes().unwrap()).unwrap(),
            viewer
        );
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
    fn serialized_size_hello_under_512_bytes() {
        assert!(hello().to_bytes().unwrap().len() < 512);
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
