# Trinix protocol

Trinix uses versioned, length-prefixed frames to carry messages over TCP
and other byte streams. This protocol is intended for LAN-only use. Framing
provides message boundaries; it does not provide authentication or encryption.

## Stream wire format

| Offset | Size (bytes) | Field | Encoding |
| --- | --- | --- | --- |
| 0 | 1 | Version | Unsigned byte; always the first byte |
| 1 | 1 | Kind | Unsigned byte; see message kinds below |
| 2 | 4 | Payload length | Unsigned 32-bit integer, little-endian |
| 6 | Payload length | Payload | Opaque bytes |

The header is exactly 6 bytes. `PROTOCOL_VERSION = 3`; receivers reject other
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

`Message` is a non-exhaustive typed enum. Payloads use bincode 2's serde
`config::legacy()` configuration, byte-compatible with the prior bincode 1.3
fixed-width integer encoding, with a 16 MiB size limit and rejection of trailing
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
| 7 | Pong | nonce: u64, t_send_us: u64, t_peer_recv_us: u64, t_peer_send_us: u64 |
| 8 | NetStats | rtt_ms: u32, loss_pct: f32, recv_kbps: u32 |
| 9 | Resume | session_id: u64, last_seq: u64 |
| 10 | Bye | reason: String |
| 11 | HelloAckV2 | session_id: u64, accepted: bool, control_available: bool |
| 12 | ControlRequest | enabled: bool |
| 13 | ControlStatus | enabled: bool |
| 14 | KeyframeRequest | no fields |

`KIND_RESERVED_FE = 0xfe` and `KIND_RESERVED_FF = 0xff` are never assigned to
messages; they are reserved for protocol extension/escape. All other unassigned
kinds (including zero) are unknown and rejected by `from_frame_parts`.

The framing version is validated by the framing layer. `Hello.proto_version` is
a separate payload field and must equal 3; a different version is rejected with
an explicit unsupported-version error.

`Role` is Host or Viewer. `Codec` is H264, Vp8, Vp9, Av1, or Raw.
`InputEvent` is MouseMove { x: f32, y: f32 }, MouseButton { button: MouseButton,
pressed: bool }, Scroll { dx: f32, dy: f32 }, Key { code: u32, pressed: bool }, or
Touch { id: u32, phase: TouchPhase, x: f32, y: f32 }. MouseButton is Left, Right,
Middle, Back, or Forward; TouchPhase is Start, Move, End, or Cancel. Coordinates
are normalized to `0..=1` by caller contract, without decode-time validation.

Pong echoes the Ping nonce and sender timestamp. `t_peer_recv_us` and
`t_peer_send_us` are the responder's monotonic microsecond timestamps at receipt
and immediately before sending. The measured RTT is
`(t_recv_us - t_send_us) - (t_peer_send_us - t_peer_recv_us)`, using saturating
subtraction to exclude peer processing time. `KeyframeRequest` asks the host to
force the next available video frame to be an IDR.

## Errors

| Error | Meaning |
| --- | --- |
| `UnsupportedVersion(v)` | Version byte differs from `PROTOCOL_VERSION`; poisons the stream decoder. |
| `TooLarge { len, max }` | Payload length exceeds 16 MiB; poisons the stream decoder when received. Encoding rejects it without modifying the output. |
| `Truncated` | Insufficient bytes for a direct header decode, or a partial frame at end-of-stream. Incremental `next_frame` returns `Ok(None)` while waiting. |
| `Empty` | Legacy whole-buffer decoder received no bytes. An empty stream is valid for the stream decoder. |

Errors implement `std::fmt::Display` and `std::error::Error`.

## Worked example

A version 3 framing-only example with kind 0 and opaque payload `"hi"` is
(kind 0 is not a typed message):

```text
03 00 02 00 00 00 68 69
│  │  └─────────┘ └───┘
│  │   length = 2  h  i
│  kind = 0
version = 3
```

## Legacy whole-buffer helpers

The existing `encode_frame` and `decode_frame` helpers remain available for now.
Their format is only `[version][payload]`, without a kind byte or length. They
require an externally known buffer boundary and cannot delimit TCP messages.
Use the stream framing functions and `FrameDecoder` for byte streams.

## Transport

`Transport` is an asynchronous, object-safe, `Send` trait that sends a kind byte
and opaque payload and receives `Frame` values. It can be used as
`Box<dyn Transport>`. It does not interpret kinds or serialize messages;
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
to the session/application above the transport; transport does not automatically
reconnect or replay frames.

## Reconnect and resume

Resume is `Message::Resume`, stable kind **9**, encoded with the same bincode
settings as all typed messages. The host session handles Resume: it validates
the logical session id, continues the sequence after `last_seq`, and forces an
IDR; the core reconnect module only sends the request and does not replay video.
`PROTOCOL_VERSION` is **3**; peers using another version are rejected. Resume is unauthenticated: bind it
to pairing/authentication in a later milestone before trusting a session claim.

After reconnect the client sends Resume first. The host session accepts it only
for the matching logical session id, continues sequence numbering from
`last_seq`, and forces an IDR; previous video frames are not replayed. The
initial `last_seq` is zero, so the next video frame has sequence one. The
reconnecting client forwards the caller's last-consumed sequence; callers update
`set_last_seq` only after consuming a frame. `Resumed` means the Resume request
was written, not acknowledged by the peer. The initial connection performs no
Resume or application handshake automatically. `connect_initial` makes one
attempt and returns initial connection failures directly.

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
their deadline. Expiry before the reader starts the write (including waiting
for channel capacity) returns `TransportError::Timeout("queue")`; expiry after
the write starts returns `TransportError::Timeout("write")`. Both phases share
one deadline. Dropping a pending `SessionReader::recv` during an inline write
drops the transport and emits `Disconnected` with the `Closed` reason; the next
receive recovers without replay. Cancelling while reading retains the transport,
and cancelling recovery retains its retry budget. Dropping the reader closes
the command channel. Resume is fully
written before the replacement transport becomes available to the writer.

Retries use exponential backoff (100 ms initially, multiplier 2), symmetric
20% jitter, and a hard five-second cap including jitter by default. The policy,
seeded randomness, and attempt limit are configurable; the default retry budget
is unlimited. A zero attempt limit gives up immediately after disconnection.
Sleep uses Tokio time, allowing paused-time tests. Each scheduled attempt
consumes retry budget, including a sleep cancelled before connecting. Backoff
and budget reset on the first received frame or a successful application send,
never on Resume alone. Exhaustion is terminal for the session. Lifecycle events
retain only the newest 64 entries; `take_events()`
drains them, so callers needing a complete history should drain regularly.

IP addresses may change during Wi-Fi roaming. `TcpConnector` stores `host:port`
and re-resolves the address through Tokio `ToSocketAddrs` on every attempt,
within the connect timeout (five seconds by default). A numeric address remains
that literal address; M8 has a standalone mDNS discovery library, not yet wired into host/viewer.
Discovery does not grant trust; pairing is not implemented.

## Adaptive bitrate

`NetStats` carries receiver feedback: `rtt_ms: u32` (round-trip milliseconds),
`loss_pct: f32` (packet loss percentage), and `recv_kbps: u32` (received kbit/s).
ABR uses `crate::message::NetStats`, also exported at the crate root. The controller
currently uses RTT and loss; received throughput is available for future policies.

`AimdController` implements `BitrateController` with deterministic AIMD rules.
Each valid timed sample updates the baseline to the minimum RTT observed within
`AimdConfig::baseline_window` (30 seconds by default). Loss strictly
above `loss_threshold_pct`, or RTT strictly above
`max(baseline * rtt_rise_factor, baseline + rtt_slack_ms)` (computed in `f64`),
causes a multiplicative decrease and resets the stability
counter. Otherwise, `stable_samples_required` consecutive samples cause one
additive increase and reset the counter. Decreases round to the nearest whole
kbit/s. All targets remain within the configured bounds; unchanged targets return
`None`. Non-finite loss or loss outside 0–100 is ignored and resets stability.
There are no timers, sleeps, or random decisions; callers choose feedback cadence.

`AimdConfig` defaults to `min_kbps = 500`, `max_kbps = 20000`,
`start_kbps = 4000`, `increase_kbps = 250`, `decrease_factor = 0.7`,
`loss_threshold_pct = 2.0`, `rtt_rise_factor = 1.5`, `rtt_slack_ms = 20`, and
`stable_samples_required = 3`. `AimdController::new` rejects invalid bounds,
zero steps/counts, non-finite factors/thresholds, a decrease factor outside
(0, 1), a loss threshold outside [0, 100], or an RTT factor at or below one.
The initial target is clamped to the bounds. Initialize the encoder with
`target_kbps()`; `adapt` then forwards changed targets to
`Encoder::set_bitrate_kbps`, which must support calls mid-stream.

The absolute RTT slack tolerates small LAN jitter even with a zero or one millisecond
baseline.


## Optional input negotiation

A viewer advertising `input-v1` can receive HelloAckV2 (kind 11) with
`session_id`, `accepted` and `control_available`. Hosts retain the original
HelloAck for viewers without this capability. ControlRequest (kind 12, `enabled`)
requests/relinquishes input; ControlStatus (kind 13, `enabled`) confirms the grant.
A private viewing token alone does not grant input. Local host consent, secure
transport and an active viewer grant are required.

Input events use normalized display coordinates and USB HID keyboard usages,
not peer-provided native OS scan codes. Text is bounded Unicode; ReleaseAll
releases held remote input. Reconnect discards queued input and requires a fresh
control request. See REMOTE_CONTROL.md and the current core message definitions
for validation bounds. Existing message kind values remain unchanged.

## Protocol identifiers

| Identifier | Value |
| --- | --- |
| TLS ALPN | `mightymonitor/secure-v1` |
| TLS server name | `mightymonitor.local` |
| Connection link scheme | `mightymonitor://` |
| DNS-SD service type | `_mightymonitor._tcp.local.` |
| Android application id | `com.mightymonitor.app` |
| Android JNI library | `mightymonitor_android` |
| Android stop-sharing intent action | `com.mightymonitor.STOP_SHARING` |
| Invitation line prefix | `MM_INVITE` |
| Environment variables | `MM_ALLOW_INPUT_SMOKE`, `MM_REQUIRE_CODECS`, `MM_STARTUP_ERROR` |

These values are part of the wire and install contract. Changing any of them
breaks interoperability with existing builds and needs a protocol version bump.
