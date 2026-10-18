# MightyMonitor protocol

MightyMonitor uses versioned, length-prefixed frames to carry messages over TCP
and other byte streams. This protocol is intended for LAN-only use. Framing
provides message boundaries; it does not provide authentication or encryption.

## Stream wire format

| Offset | Size (bytes) | Field | Encoding |
| --- | --- | --- | --- |
| 0 | 1 | Version | Unsigned byte; always the first byte |
| 1 | 1 | Kind/flags | Unsigned byte; reserved |
| 2 | 4 | Payload length | Unsigned 32-bit integer, little-endian |
| 6 | Payload length | Payload | Opaque bytes |

The header is exactly 6 bytes. `PROTOCOL_VERSION = 1`; receivers reject other
versions. Every frame starts with its version byte, including frames after the
first frame in a connection. The kind byte remains reserved; `0` means reserved/unspecified. Typed message
variants are encoded inside the payload. The framing layer preserves all
kind values without interpreting them.

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

## Errors

| Error | Meaning |
| --- | --- |
| `UnsupportedVersion(v)` | Version byte differs from `PROTOCOL_VERSION`; poisons the stream decoder. |
| `TooLarge { len, max }` | Payload length exceeds 16 MiB; poisons the stream decoder when received. Encoding rejects it without modifying the output. |
| `Truncated` | Insufficient bytes for a direct header decode, or a partial frame at end-of-stream. Incremental `next_frame` returns `Ok(None)` while waiting. |
| `Empty` | Legacy whole-buffer decoder received no bytes. An empty stream is valid for the stream decoder. |

Errors implement `std::fmt::Display` and `std::error::Error`.

## Worked example

A version 1 frame with kind 0 and payload `"hi"` is:

```text
01 00 02 00 00 00 68 69
│  │  └─────────┘ └───┘
│  │   length = 2  h  i
│  kind = 0
version = 1
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
Callers serialize and decode `Message` payloads explicitly.

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

## Messages

`Message::to_bytes` and `Message::from_bytes` encode only the frame payload;
framing still supplies the version byte first. Transport remains frame-level.
The wire configuration is bincode 1.3 with fixed-width integer encoding and
little-endian byte order (`DefaultOptions` with `with_fixint_encoding` and
`with_little_endian`). Enum discriminants are zero-based `u32` values in the
listed order; string and vector lengths are `u64`. Changing variant order or
these settings changes the wire format. Both directions use a
`MAX_PAYLOAD_LEN` byte budget, and decoding rejects trailing bytes, malformed
payloads, and unknown discriminants through `MessageError`.

`Message` is `#[non_exhaustive]` for forward source compatibility: downstream
matches need a wildcard arm. This does not make unknown wire variants decodable;
protocol changes still require version coordination.

| Variant | Fields | Notes |
| --- | --- | --- |
| `Hello` | `proto_version: u8`, `device_name: String`, `role: Role`, `capabilities: Vec<String>` | Role is `Host` or `Viewer`. |
| `HelloAck` | `session_id: u64`, `accepted: bool` | Handshake response. |
| `VideoConfig` | `codec: Codec`, `width: u32`, `height: u32`, `fps: u32` | Codec is `H264`, `Vp8`, `Vp9`, `Av1`, or `Raw`. |
| `VideoFrame` | `seq: u64`, `timestamp_us: u64`, `keyframe: bool`, `data: Vec<u8>` | Encoded video data; timestamp in microseconds. |
| `Input` | `InputEvent` | Remote mouse, scroll, key, or touch input; see below. |
| `Ping` | `nonce: u64`, `t_send_us: u64` | Sender timestamp in microseconds. |
| `Pong` | `nonce: u64`, `t_send_us: u64` | Echoes ping identifiers. |
| `NetStats` | `NetStats { rtt_ms: u32, loss_pct: f32, recv_kbps: u32 }` | Round-trip milliseconds, loss percentage, receive kilobits per second. |
| `Resume` | `session_id: u64`, `last_seq: u64` | Message definition only; no resume or reconnect behavior is implemented. |
| `Bye` | `reason: String` | Session end reason. |

`InputEvent` variants are ordered as follows:

| Variant | Fields | Notes |
| --- | --- | --- |
| `MouseMove` | `x: f32`, `y: f32` | Normalized coordinates in `0..=1`. |
| `MouseButton` | `button: MouseButton`, `pressed: bool` | Button is `Left`, `Right`, `Middle`, `Back`, or `Forward`. |
| `Scroll` | `dx: f32`, `dy: f32` | Scroll deltas. |
| `Key` | `code: u32`, `pressed: bool` | Key code and state. |
| `Touch` | `id: u32`, `phase: TouchPhase`, `x: f32`, `y: f32` | Phase is `Start`, `Move`, `End`, or `Cancel`; coordinates normalized to `0..=1`. |

Coordinate ranges and application semantics are caller contracts; decoding does
not validate them. All listed types support serde serialization and cloning.
Types containing floating-point fields implement `PartialEq` without `Eq`.
