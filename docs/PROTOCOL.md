# MightyMonitor protocol

MightyMonitor uses versioned, length-prefixed frames to carry messages over TCP
and other byte streams. This protocol is intended for LAN-only use. Framing
provides message boundaries; it does not provide authentication or encryption.

## Stream wire format

| Offset | Size (bytes) | Field | Encoding |
| --- | --- | --- | --- |
| 0 | 1 | Version | Unsigned byte; always the first byte |
| 1 | 1 | Kind | Unsigned byte; see message kinds below |
| 2 | 4 | Payload length | Unsigned 32-bit integer, little-endian |
| 6 | Payload length | Payload | Opaque bytes |

The header is exactly 6 bytes. `PROTOCOL_VERSION = 2`; receivers reject other
versions. Every frame starts with its version byte, including frames after the
first frame in a connection. The framing layer preserves all kind values without
interpreting them; the typed message layer validates the kind and payload together.

`MAX_PAYLOAD_LEN` is 16 MiB (16 × 1024 × 1024 bytes). This limit prevents an
untrusted length claim from causing an arbitrarily large payload allocation
(denial of service). Applications should consume decoded frames promptly and
apply their own limits to queued data and connection lifetimes.

## Encoding and decoding

`encode_stream_frame` returns a new frame buffer. `encode_frame_into` appends a
frame to an existing vector and leaves it unchanged if the payload is too large.
`FrameHeader::encode` serializes the supplied fields without validation;
`FrameHeader::decode` validates the version and length and ignores trailing bytes.

TCP reads can contain partial headers, partial payloads, or several frames.
`FrameDecoder::push` accepts those bytes incrementally and validates each header
as soon as all six bytes arrive, before buffering its payload. `next_frame`
returns `Ok(None)` when more bytes are needed and `Ok(Some(Frame))` when a frame
is complete. Call it repeatedly to consume concatenated frames. A read offset
and occasional compaction avoid repeatedly copying a growing partial payload.

A malformed version or oversized length poisons the decoder. Subsequent
`next_frame` calls return the original error, further input is ignored, and the
caller must drop the connection. Header errors are reported by `next_frame`
because `push` has no return value. Earlier complete frames still buffered when the error occurs are
dropped from delivery, whether buffered by the same or an earlier `push`; they
can no longer be retrieved. Their bytes remain counted by `buffered_len` until
the decoder is dropped.

At end-of-stream, `finish` returns `Truncated` if a partial header or payload
remains. It succeeds at a clean frame boundary, including when complete frames
remain buffered for consumption. On a poisoned decoder it returns the original
error. `buffered_len` counts bytes not yet consumed by `next_frame`.

## Messages

`Message` is a non-exhaustive typed enum. Payloads use bincode 1.3 with fixed-width
integers, little-endian byte order, a 16 MiB size limit, and rejection of trailing
bytes. The payload includes the enum's zero-based u32 discriminant (declaration
order); it is distinct from the one-based frame kind. Variant order is part of
the wire format. `to_bytes`/`from_bytes` handle payloads alone;
`to_frame_parts`/`from_frame_parts` also handle the kind and verify it matches the
decoded variant. Malformed payloads, unknown variants, unknown kinds, reserved
kinds, and kind/variant mismatches return `MessageError`.

| Kind id | Variant | Fields |
| --- | --- | --- |
| 1 | Hello | proto_version: u8, device_name: String, role: Role, capabilities: Vec<String> |
| 2 | HelloAck | session_id: u64, accepted: bool |
| 3 | VideoConfig | codec: Codec, width: u32, height: u32, fps: u32 |
| 4 | VideoFrame | seq: u64, timestamp_us: u64, keyframe: bool, data: Vec<u8> |
| 5 | Input | InputEvent |
| 6 | Ping | nonce: u64, t_send_us: u64 |
| 7 | Pong | nonce: u64, t_send_us: u64 |
| 8 | NetStats | rtt_ms: u32, loss_pct: f32, recv_kbps: u32 |
| 9 | Resume | session_id: u64, last_seq: u64 |
| 10 | Bye | reason: String |

`KIND_RESERVED_FE = 0xfe` and `KIND_RESERVED_FF = 0xff` are never assigned to
messages; they are reserved for protocol extension/escape. All other unassigned
kinds (including zero) are unknown and rejected by `from_frame_parts`.

`Role` is Host or Viewer. `Codec` is H264, Vp8, Vp9, Av1, or Raw.
`InputEvent` is MouseMove { x: f32, y: f32 }, MouseButton { button: MouseButton,
pressed: bool }, Scroll { dx: f32, dy: f32 }, Key { code: u32, pressed: bool }, or
Touch { id: u32, phase: TouchPhase, x: f32, y: f32 }. MouseButton is Left, Right,
Middle, Back, or Forward; TouchPhase is Start, Move, End, or Cancel. Coordinates
are normalized to `0..=1` by caller contract, without decode-time validation.

## Errors

| Error | Meaning |
| --- | --- |
| `UnsupportedVersion(v)` | Version byte differs from `PROTOCOL_VERSION`; poisons the stream decoder. |
| `TooLarge { len, max }` | Payload length exceeds 16 MiB; poisons the stream decoder when received. Encoding rejects it without modifying the output. |
| `Truncated` | Insufficient bytes for a direct header decode, or a partial frame at end-of-stream. Incremental `next_frame` returns `Ok(None)` while waiting. |
| `Empty` | Legacy whole-buffer decoder received no bytes. An empty stream is valid for the stream decoder. |

Errors implement `std::fmt::Display` and `std::error::Error`.

## Worked example

A version 2 framing-only example with kind 0 and opaque payload `"hi"` is
(kind 0 is not a typed message):

```text
02 00 02 00 00 00 68 69
│  │  └─────────┘ └───┘
│  │   length = 2  h  i
│  kind = 0
version = 2
```

## Legacy whole-buffer helpers

The existing `encode_frame` and `decode_frame` helpers remain available for now.
Their format is only `[version][payload]`, without a kind byte or length. They
require an externally known buffer boundary and cannot delimit TCP messages.
Use the stream framing functions and `FrameDecoder` for byte streams.

## Transport

`Transport` is an asynchronous, object-safe, `Send` trait that sends a kind byte
and opaque payload and receives `Frame` values. It can be used as
`Box<dyn Transport>`. It does not interpret kinds or serialize messages; the
`Message::to_frame_parts` supplies the kind and payload for `send`;
`Message::from_frame_parts` validates received frames.

`TcpTransport::new` wraps an accepted/connected Tokio TCP stream and enables
TCP_NODELAY. `connect` resolves and connects with a five-second deadline;
`connect_with_timeout` accepts a custom deadline. Writes default to ten seconds,
including `write_all` and flush. Reads have no deadline by default.
`set_read_timeout` and `set_write_timeout` accept `Option<Duration>`; `None`
disables the deadline. A receive deadline covers the entire frame, including
all partial reads, so a trickling peer cannot extend it indefinitely.

`TransportError::Io` wraps socket errors, `Frame` wraps framing failures,
`Closed` means EOF at a frame boundary, and `Timeout` identifies the timed-out
operation (`connect`, `read`, or `write`). EOF in a header or payload reports
`Frame(Truncated)`. Buffered complete frames are received before clean EOF.
Malformed versions and oversized lengths fail promptly and poison the decoder.

A canceled or timed-out receive retains partial bytes and can be retried. A
canceled or timed-out send may have written part of a frame: drop that connection
rather than retrying on it. Drop and reconnect after malformed input, socket
errors, or EOF. Reconnection and any handshake, backoff, or replay policy belong
to the caller; transport does not automatically reconnect or replay frames.

## Reconnect and resume

Resume is now `Message::Resume`, stable kind **9**, encoded with the same bincode
settings as all typed messages. `PROTOCOL_VERSION` was bumped to **2** to finalize
this wire format; version 1 peers are rejected. Resume is unauthenticated: bind it
to pairing/authentication in a later milestone before trusting a session claim.

After reconnect the client sends Resume first. The peer replays frames **after**
`last_seq` for the identified logical session. The initial `last_seq` is zero;
`last_seq == 0` requests replay after sequence zero, not an unspecified-position
sentinel. The session forwards this value without implementing peer-side replay. The caller updates `set_last_seq`
only after consuming a frame; opaque frame payloads are never interpreted by the
session. `Resumed` means the request was written, not acknowledged by the peer.
The initial connection performs no Resume or application handshake automatically.
`connect_initial` makes one attempt and returns initial connection failures directly.

`ReconnectingSession` drops connections on I/O, framing, EOF, and write timeout
errors. `recv()` reconnects and continues receiving. Read timeouts return to the
caller with the same transport and its partial input intact. `send()` never
replays application frames: after successful recovery it returns
`SendOutcome::ReconnectedNotSent`. Some or all of the original send may already
have reached the peer; application sequence numbers and replay rules must resolve
that ambiguity. Cancellation of a send requires dropping the session because
partial output may have been written. The transport never replays automatically.

Retries use exponential backoff (100 ms initially, multiplier 2), symmetric
20% jitter, and a hard five-second cap including jitter by default. The policy,
seeded randomness, and attempt limit are configurable; the default retry budget
is unlimited. A zero attempt limit gives up immediately after disconnection.
Sleep uses Tokio time, allowing paused-time tests. Exhaustion is terminal for the
session. Lifecycle events retain only the newest 64 entries; `take_events()`
drains them, so callers needing a complete history should drain regularly.

IP addresses may change during Wi-Fi roaming. `TcpConnector` stores `host:port`
and re-resolves the address through Tokio `ToSocketAddrs` on every attempt,
within the connect timeout (five seconds by default). A numeric address remains
that literal address; hostname discovery via mDNS is a later milestone, M8.
