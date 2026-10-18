# Desktop viewer

On Linux install the GStreamer development libraries and runtime plugins from
[CODEC.md](CODEC.md), especially `gstreamer1.0-libav` (avdec_h264). The GUI uses
OpenGL and X11 or Wayland; CI installs libxkbcommon-dev, libwayland-dev,
libgl1-mesa-dev and libx11-dev. Run:

```sh
cargo run -p mightymonitor-viewer --bin mm-viewer
```

Enter the host IP:port and click Connect. The default is 127.0.0.1:5000.
Windows builds the GUI but Connect displays an explicit unsupported decoder
message. Real decoding currently requires Linux. For a headless core build:

```sh
cargo test -p mightymonitor-viewer --no-default-features
```

The initial handshake sends Hello (current PROTOCOL_VERSION, Viewer role), then
requires an accepted HelloAck and H.264 VideoConfig. This depends on the M2-4
host pipeline PR #29, which is not yet merged. See [HOST.md](HOST.md).
After a network failure, core ReconnectingSession's split reader/writer retries
with backoff and sends Resume using the acknowledged session id and last received
video sequence. The host responds with HelloAck/VideoConfig and a fresh keyframe;
the viewer skips dependent frames until that keyframe. Connecting, Connected,
Reconnecting and Closed are exposed in the headless output and displayed by egui.
Initial dial failures retry every second. Rejected initial handshakes also retry;
Bye, invalid stream messages or decode errors close the session. Connect replaces
and cancels the current session.

The overlay reports decoded fps and encoded payload kbit/s over roughly one-second
windows, RTT from matching Ping/Pong, and decode-completion to texture-submission
latency. That last measurement is logged to stderr per submitted frame. It includes
UI scheduling, I420 conversion and texture upload submission; it does not measure
GPU completion, monitor scanout, capture-to-display or glass-to-glass latency.
NetStats and Ping are sent approximately once per second once configured. TCP
loss is not observable here, so loss_pct is zero; throughput excludes framing.
UI frames use a latest-frame slot; encoded input remains ordered and bounded so
reference frames are decoded even if the UI misses a refresh. There is no input
forwarding, audio, mDNS, authentication or encryption; use a trusted LAN.

## Manual smoke test (requires a desktop)

1. Start `mm-host` using HOST.md, then start mm-viewer on Linux. Connect to the
   listener. Confirm Connected, moving desktop video, sensible colors/aspect,
   fps/bitrate, RTT, and latency log lines. Resize the window.
2. Kill the host. Confirm Reconnecting remains visible and the GUI responds.
   Restart the host at the same address/session id. Confirm a new configuration
   and keyframe restore video without pressing Connect.
3. Repeat with a LAN peer; interrupt its network for several seconds and recover.
   Record end-to-end latency separately following LATENCY.md.
4. Click Connect again and close the window during an outage; confirm clean exit.
5. On Windows, click Connect and confirm the clear Linux-only decoder message.

Loopback tests use a minimal fake protocol host and dummy decoder, covering
Resume/session/sequence, post-reconnect frames, Closed and feedback cadence.
Real H.264 host streaming, desktop presentation, Windows runtime and Wi-Fi latency
remain manual validation work. Color conversion currently assumes limited-range
BT.601 I420; negotiated colorimetry and hardware texture conversion are absent.
