# Host streaming

On Linux, install the X11/PipeWire and GStreamer prerequisites in [LINUX.md](LINUX.md)
and [CODEC.md](CODEC.md), including the H.264 encoder plugins. Wayland uses the system screen-selection dialog; X11 uses `DISPLAY`:

```sh
cargo run -p mightymonitor-host --bin mm-host -- --secure --bind 127.0.0.1:5000 --fps 30 --bitrate 4000
```

`--bind` is the TCP listen address (default `127.0.0.1:5000`; use a LAN address
for a remote viewer). `--fps` accepts 1–240 (default 30). `--bitrate` is the
initial H.264 target in kbit/s, 1–1000000 (default 4000); viewer NetStats updates
adapt it through core's AIMD controller. `--help` prints usage. Use `--secure` for a private viewing link over pinned TLS. The GUI always uses
this mode. Legacy CLI mode remains unencrypted and prints a non-loopback warning.
See [connection security](SECURE_CONNECTIONS.md).
`--idle-timeout-secs` sets the post-handshake viewer silence limit (default 30
seconds, positive values only); Pings and other valid controls reset it.

The executable selects a portal-authorized screen on Wayland or the X11 root
window, converts packed BGRA to RGBA, and uses codec-gst's low-latency H.264 encoder.
`--capture auto` is the default; `--capture x11` and `--capture wayland` explicitly
select a backend. Default Linux builds enable PipeWire. Minimal X11-only builds
can use `--no-default-features`. Wayland permission is requested before publishing
the connection link, and one portal session survives viewer reconnection. Static
screens reuse the last frame. Resolution changes end the current video session;
a reconnect renegotiates dimensions without a new system prompt. Stopping the
host closes the portal. A missing/denied portal never silently falls back to X11. It accepts up to four simultaneous viewers, with one controlling viewer at a time. The Windows executable uses DXGI desktop capture through GStreamer and the same
H.264 pipeline. See [WINDOWS.md](WINDOWS.md) for setup and pending acceptance.

## Session and pipeline

The viewer sends Hello with the current protocol version and Viewer role. The
host replies HelloAck followed by VideoConfig, then VideoFrame messages. Ping
receives Pong with the same nonce and timestamp; Pong and NetStats are accepted.
Bye ends a session. Invalid handshakes are rejected, with a five-second deadline.
Secure peers complete TLS and token authorization before per-viewer encoder
setup. The core Hello/Resume validation follows that authorization. Wayland
capture permission belongs to the sharing session and precedes network listening.
See [PROTOCOL.md](PROTOCOL.md) for framing and typed payloads.

The pipeline uses core Capture and Encoder through a HostEncoder extension for
fallible encoding, forced keyframes and actual keyframe metadata. Capture runs
on a dedicated thread; encoding runs on Tokio's blocking pool. TCP uses core
Transport and the ReconnectingSession reader/writer split. The accepted-socket
connector disables dialing/retry: the main listener accepts replacement sockets.

The first frame on each connection is a keyframe. A viewer requests another by
sending Resume with the logical session id from HelloAck. Resume as the first
message also receives HelloAck/VideoConfig and starts sequence numbers after
last_seq. TLS sessions receive random 64-bit logical IDs bound to the authenticated
transport principal. A shared registry retains at most 16 sessions for five
minutes after the last sent frame; Resume rechecks identity, pairing revocation
and `last_seq <= highest sequence reserved for sending`. Invalid Resume receives
a generic rejected HelloAck and closes before video. Sequence continues after
the server's highest reserved sequence. Explicit legacy plaintext uses ID 1 and
retains unauthenticated compatibility behavior. Live video intentionally skips replay of historical frames: resume
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
the slot during writes. Input and revocation are handled by the session reader even while encoding or
writing waits. Outgoing control replies use a bounded channel; flooding ends the
session instead of growing memory without bound.
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

Adaptive bitrate policy, write-pressure feedback, simulation and limitations are
specified in [ABR.md](ABR.md). Live bitrate rejection leaves the controller at its
last accepted target so subsequent feedback can retry.


Optional input uses `--secure --allow-control` and a separate viewer grant.
Wayland extension uses `--extend` and system Virtual-monitor support. See
[REMOTE_CONTROL.md](REMOTE_CONTROL.md) and [EXTENDED_DISPLAYS.md](EXTENDED_DISPLAYS.md).

## Pairing CLI

`mm-host --pair` (alias `--pairing-mode`) opens a separate 60-second pairing
window on `--bind ADDRESS:PORT`, accepts one attempt and exits after confirmation
or failure. It does not open screen capture. Run the security helper with
`cargo run -p mightymonitor-security --features async --bin mm-devices -- --pair ADDRESS:PORT`
on the other device. Compare the SAS on both devices and type `yes` locally on
each. No unattended approval is provided.

`mm-host --paired --bind ADDRESS:PORT` listens for pinned, secret-authenticated
stored peers and shares view-only video. `--allow-control` has no effect in this
mode. The private-link `--secure` flow remains separate. Pairing cannot be
combined with `--paired`, `--managed`, `--allow-control` or `--extend`.
`mm-host --list-paired` prints the short ID, friendly name, paired-at and last-seen
timestamps; `mm-host --unpair ID` removes a local pairing without capture.
The `mm-devices` helper supports `--list-paired`, `--unpair ID` and `--forget-all`
on either device. No command prints long-lived pairing credentials. The
intentional SAS prompt is for local comparison, not diagnostics collection.
See SECURE_CONNECTIONS.md for configuration paths, API hooks and revocation limits.

## Desktop Share pairing

Each `--secure` share now opens a 60-second pairing window and a separate device
endpoint on the selected interface. Only that endpoint's filtered private
on-subnet route and allow-listed metadata are advertised while sharing; dropping
the advertisement stops discovery. The existing private-link endpoint is unchanged.
The GUI shows the countdown, pairing QR, Renew when expired/locked, incoming
six-digit code with Allow / Don't allow, and paired names with confirmed Forget.
Allow also confirms that the two displayed codes match. Both devices must confirm;
the offer never bypasses local Allow. Paired viewing stays view-only.

Three failed attempts lock the window across connections. Renew replaces the
mode and invalidates the previous single-use offer. A used, expired or old offer
is rejected. The limiter counts failed confirmations/offers rather than typed
codes. The local channel, bounds and device endpoint preface are specified in
[SECURE_CONNECTIONS.md](SECURE_CONNECTIONS.md#desktop-share-endpoint-and-local-channel).
Managed stdin closure stops sharing even while pairing is pending.

Nearby may not work on guest or shared Wi-Fi. Copy a private viewing link from
Connect another way and paste it on the other device. Android discovery/pairing,
in-app QR scanning and physical Wi-Fi/Windows runtime acceptance remain unverified
or absent; the new rendering has not been visually checked on a desktop.
