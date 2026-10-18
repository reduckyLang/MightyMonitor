# Host streaming

On Linux, install the X11 and GStreamer prerequisites in [LINUX.md](LINUX.md)
and [CODEC.md](CODEC.md), including the H.264 encoder plugins. Run in an X11
session with `DISPLAY` set:

```sh
cargo run -p mightymonitor-host --bin mm-host -- --bind 127.0.0.1:5000 --fps 30 --bitrate 4000
```

`--bind` is the TCP listen address (default `127.0.0.1:5000`; use a LAN address
for a remote viewer). `--fps` accepts 1–240 (default 30). `--bitrate` is the
initial H.264 target in kbit/s, 1–1000000 (default 4000); viewer NetStats updates
adapt it through core's AIMD controller. `--help` prints usage. The protocol
currently provides no authentication or encryption; use a trusted LAN. The host
warns on stderr when the resolved listen address is non-loopback.
`--idle-timeout-secs` sets the post-handshake viewer silence limit (default 30
seconds, positive values only); Pings and other valid controls reset it.

The executable captures the X11 root window, converts packed BGRA to RGBA, and
uses codec-gst's low-latency H.264 encoder. It serves one viewer at a time and
accepts another after disconnection. The portable library and tests compile on
Windows; the executable's real capture/codec backend is Linux-only.

## Session and pipeline

The viewer sends Hello with the current protocol version and Viewer role. The
host replies HelloAck followed by VideoConfig, then VideoFrame messages. Ping
receives Pong with the same nonce and timestamp; Pong and NetStats are accepted.
Bye ends a session. Invalid handshakes are rejected, with a five-second deadline.
Invalid peers currently cause capture/encoder setup before handshake validation;
moving validation ahead of resource creation remains a follow-up.
See [PROTOCOL.md](PROTOCOL.md) for framing and typed payloads.

The pipeline uses core Capture and Encoder through a HostEncoder extension for
fallible encoding, forced keyframes and actual keyframe metadata. Capture runs
on a dedicated thread; encoding runs on Tokio's blocking pool. TCP uses core
Transport and the ReconnectingSession reader/writer split. The accepted-socket
connector disables dialing/retry: the main listener accepts replacement sockets.

The first frame on each connection is a keyframe. A viewer requests another by
sending Resume with the logical session id from HelloAck. Resume as the first
message also receives HelloAck/VideoConfig and starts sequence numbers after
last_seq. The current executable uses logical session id 1 across accepted
connections. Live video intentionally skips replay of historical frames: resume
starts from fresh capture and a keyframe. This is the host's live-stream policy
rather than the generic replay behavior described in PROTOCOL.md.

## Backpressure and metrics

A single raw-frame slot implements latest-frame-wins. Capture replaces an
unconsumed frame and increments the drop counter. The writer takes and encodes
only the latest frame after its previous send completes. There is at most one
queued raw frame and one frame being encoded/sent; there is no encoded backlog.
Frames aged two frame intervals or more are discarded before encoding and again
before sending (including serialization). Dropping an encoded reference forces
a keyframe on the next attempt. An encoder slower than this budget can therefore
produce no output until it catches up.

Pipeline::monitor returns snapshots with queue_len, max_queue_len, captured,
dropped, sent and max_send_age. Age is capture-start to send initiation; TCP
buffers and viewer read delays can still add delivery latency. A blocked send
has a two-second deadline in mm-host; a partial/timed-out write ends that
connection rather than retrying a corrupt stream. Capture continues replacing
the slot during writes. Control handling can wait for an in-progress write or
encode, and its bounded eight-message channel disconnects a flooding viewer.
Dropping the pipeline stops capture after its current backend call completes.
A capture-thread panic closes the slot and wakes a waiting consumer; poisoned
slot locks are recovered. A backend call that never returns remains a limitation.
The idle deadline also bounds waiting for capture or encoding. Received Pings
renew it during that wait; control replies can wait for encoding, and an
in-progress write can delay timeout reporting until its write deadline.

`Pipeline::on_stats` returns bitrate rejection errors. The session logs these
and continues at the encoder's previous setting; initial bitrate rejection ends
setup. AIMD keeps its requested target and may request another change on later
feedback.

Loopback integration tests use fake capture and a dummy encoder, requiring
neither DISPLAY nor GStreamer plugins at runtime. They check normal streaming,
slow-reader drops and freshness, Ping/Pong, requested/resumed keyframes, invalid
Hello versions/roles, handshake silence, idle silence/Ping resets, and capture
panic termination.
Linux builds still require the native GStreamer development libraries because
the real binary links codec-gst. Live X11/H.264 viewing, Wi-Fi delivery latency,
and hardware encoding need separate desktop validation.

The host adds no new third-party packages to the workspace lockfile: Tokio
(MIT) and async-trait (MIT OR Apache-2.0) are reused, alongside the existing
workspace backends. See CODEC.md for GStreamer/plugin licensing considerations.
