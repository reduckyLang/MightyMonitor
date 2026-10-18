# MightyMonitor protocol

MightyMonitor uses versioned, length-prefixed frames to carry messages over TCP
and other byte streams. This protocol is intended for LAN-only use. Framing
provides message boundaries; it does not provide authentication or encryption.

## Stream wire format

| Offset | Size (bytes) | Field | Encoding |
| --- | --- | --- | --- |
| 0 | 1 | Version | Unsigned byte; always the first byte |
| 1 | 1 | Kind/flags | Unsigned byte; see reconnect kind below |
| 2 | 4 | Payload length | Unsigned 32-bit integer, little-endian |
| 6 | Payload length | Payload | Opaque bytes |

The header is exactly 6 bytes. `PROTOCOL_VERSION = 1`; receivers reject other
versions. Every frame starts with its version byte, including frames after the
first frame in a connection. The kind byte is reserved for message kinds defined
in M1-3, except the provisional Resume code documented below; `0` means
reserved/unspecified for now. The framing layer preserves all
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
Message-level API arrives with M1-3 integration.

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

Until M1-3 is integrated, Resume uses provisional frame kind `0xfe` (254).
This code will be mapped to `Message::Resume` when the typed message API lands;
applications must not treat this provisional allocation as a stable message table.
Its payload is exactly 16 bytes, without serde or bincode:

| Payload offset | Size | Field | Encoding |
| --- | --- | --- | --- |
| 0 | 8 | session_id | u64 little-endian |
| 8 | 8 | last_seq | u64 little-endian |

After reconnect the client sends Resume first. The peer replays frames **after**
`last_seq` for the identified logical session. The caller updates `set_last_seq`
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
that ambiguity. Cancellation of an unsplit send drops its transport because
partial output may have been written; the next operation recovers. The transport never replays automatically.

`ReconnectingSession::split(self)` returns `SessionReader` and `SessionWriter`.
The reader owns the transport, reconnect/backoff, and application-supplied
`set_last_seq`; call its `connect_initial` if splitting before initial connection.
Both halves are `Send` (`JitterSource` requires `Send`) and expose shared `state`
and `take_events`. Draining events from either half consumes the same bounded
history. The unsplit API remains available with its existing recovery behavior.

The reader's `recv` acts as the command loop: keep polling it to service writes.
A bounded channel (one pending command) carries writer requests. While connected,
the reader selects between cancellation-safe transport receive and send requests;
no mutex is held across receive, write, connect, or backoff. Writes serialize with
receives at the transport, so a write in progress can delay receiving. The shared
mutex protects only lifecycle metadata. Custom transports must preserve partial
input when receive is cancelled, as required by `Transport::recv`.

`SessionWriter::send(kind, payload, timeout)` applies the supplied deadline to
both dispatch and writing. During reconnect it immediately returns
`SendOutcome::NotConnected` and queues nothing; after exhaustion it returns
`SessionError::GaveUp`. Requests crossing a disconnection are rejected rather
than replayed on a replacement transport. A successful send returns `Sent`;
write I/O errors/timeouts are returned directly and leave recovery to the reader.
Cancellation or deadline expiry during an active write drops that transport;
requests cancelled before writing are discarded. No application frame is
replayed automatically. If the reader stops polling, sends remain bounded by
their deadline; dropping the reader closes the command channel. Resume is fully
written before the replacement transport becomes available to the writer.

Retries use exponential backoff (100 ms initially, multiplier 2), symmetric
20% jitter, and a hard five-second cap including jitter by default. The policy,
seeded randomness, and attempt limit are configurable; the default retry budget
is unlimited. A zero attempt limit gives up immediately after disconnection.
Sleep uses Tokio time, allowing paused-time tests. Each scheduled attempt
consumes retry budget, including a sleep cancelled before connecting. Backoff
and budget reset only on the first received frame, never on Resume alone. Exhaustion is terminal for the
session. Lifecycle events retain only the newest 64 entries; `take_events()`
drains them, so callers needing a complete history should drain regularly.

IP addresses may change during Wi-Fi roaming. `TcpConnector` stores `host:port`
and re-resolves the address through Tokio `ToSocketAddrs` on every attempt,
within the connect timeout (five seconds by default). A numeric address remains
that literal address; hostname discovery via mDNS is a later milestone, M8.
