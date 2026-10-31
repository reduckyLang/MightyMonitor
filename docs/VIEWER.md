# Desktop viewer

On Linux install the GStreamer development libraries and runtime plugins from
[CODEC.md](CODEC.md), especially `gstreamer1.0-libav` (avdec_h264). The GUI uses
OpenGL and X11 or Wayland; the required development libraries
(`libgl1-mesa-dev`, `libx11-dev`, `libxkbcommon-dev`, `libwayland-dev`) are
documented in [LINUX.md](LINUX.md). Run:


```sh
cargo run -p trinix-viewer --bin trinix-viewer
```

Paste the host's private viewing link and click Connect. Bare IP:port connections
require explicitly enabling legacy unencrypted viewing.
Windows uses the same GStreamer H.264 decoder; see [WINDOWS.md](WINDOWS.md) for
the native runtime/development prerequisites. Physical Windows playback still
needs validation. For a headless core build:

```sh
cargo test -p trinix-viewer --no-default-features
```

The initial handshake sends Hello (current PROTOCOL_VERSION, Viewer role), then
requires an accepted HelloAck and H.264 VideoConfig. The M2-4
host pipeline is merged. See [HOST.md](HOST.md).
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
reference frames are decoded even if the UI misses a refresh. Pinned TLS and private session links protect normal viewing. Optional focused
input forwarding, desktop Nearby discovery and persistent pairing are implemented;
audio and Android pairing are not.

## Manual smoke test (requires a desktop)

1. Start `trinix-host` using HOST.md, then start trinix-viewer on Linux. Connect to the
   listener. Confirm Connected, moving desktop video, sensible colors/aspect,
   fps/bitrate, RTT, and latency log lines. Resize the window.
2. Kill the host. Confirm Reconnecting remains visible and the GUI responds.
   Restart the host at the same address/session id. Confirm a new configuration
   and keyframe restore video without pressing Connect.
3. Repeat with a LAN peer; interrupt its network for several seconds and recover.
   Record end-to-end latency separately following LATENCY.md.
4. Click Connect again and close the window during an outage; confirm clean exit.
5. On Windows, verify moving H.264 playback from Linux and Windows hosts.

Loopback tests use a minimal fake protocol host and dummy decoder, covering
Resume/session/sequence, post-reconnect frames, Closed and feedback cadence.
Real H.264 host streaming, desktop presentation, Windows runtime and Wi-Fi latency
remain manual validation work. Color conversion currently assumes limited-range
BT.601 I420; negotiated colorimetry and hardware texture conversion are absent.

See [ABR.md](ABR.md) for host feedback policy. Sequence gaps are not a packet-loss
measurement: the host deliberately skips frames before writing. Current RTT also
includes control scheduling and ordered TCP backlog, which is useful congestion
feedback but is not a pure wire RTT.

## Desktop control panel

Build both binaries with `cargo build --release --workspace`, then run
`target/release/trinix-viewer`. The viewer finds `trinix-host` beside its own executable;
packaging must keep the two together. The generated app icon is embedded in the
viewer window. The icon does not constitute a Windows executable resource or a
Linux desktop-menu installation; those are packaging work.

On Linux X11/Wayland or Windows, **Share this screen** launches the sibling host without
a shell (trinix-host on Linux, trinix-host.exe on Windows).
Enter a local listen IP/port, frame rate, and bitrate. The loopback default is
for this computer only; use the computer's LAN address for another device.
Normal sharing uses private links and pinned TLS. Optional **Allow remote control**
requires a separate viewer request; Wayland also requires system input consent.
See [REMOTE_CONTROL.md](REMOTE_CONTROL.md). Supported Wayland desktops additionally
offer **Extend**; see [EXTENDED_DISPLAYS.md](EXTENDED_DISPLAYS.md).
**Stop sharing**, or closing the application, terminates and reaps the owned
host process. A separately started host is unaffected. Configuration controls
are locked while the owned host runs. A sharing failure displays the last 4 KiB
of host diagnostics; the private temporary log is removed during normal cleanup.
Logs can remain after a crash or forced termination of the viewer.

**View a screen** lists nearby desktop shares and keeps a private-link fallback. **Disconnect** cancels the
viewer session and clears the last displayed frame. Viewing and sharing can run
at the same time on Linux, but this does not create independent extended displays
or provide multi-viewer hosting. Sharing captures the X11 root window: avoid
viewing a stream of your own captured desktop to prevent recursive imagery.
Windows now uses real GStreamer decoding and the DXGI host. Native dependency
setup and physical desktop acceptance are described in WINDOWS.md.

Manual acceptance (not yet performed on physical devices):

1. Launch the bundled viewer with its host binary present; confirm the app icon,
   controls, and empty state. Try invalid addresses and missing host binary.
2. Start sharing on a Linux X11 LAN address, connect from another Linux device,
   and verify moving video. Confirm settings cannot change during sharing.
3. Stop sharing and confirm the host process and stream end. Restart sharing.
4. Disconnect the viewer and confirm the displayed frame clears. Reconnect.
5. Close the app while sharing and verify no owned trinix-host process remains.
6. Attempt sharing outside X11 or with an occupied port and verify readable errors.
7. Verify Windows host/viewer playback using the acceptance steps in WINDOWS.md.

## Nearby and paired devices

Open **View a screen**, then choose a device under **Nearby devices**. A new
device displays a six-digit code derived from the secure connection on both
devices. Compare them, choose **Yes, codes match**, and choose **Allow** on the
sharing device only if the codes match. **No** or **Don't allow** aborts; a
60-second timeout stores nothing. A successful pair is persisted and viewing
starts. Choose a paired device next time for view-only viewing without a code.
A changed key fails against the stored pin; discovery names/IDs cannot replace it.

**Paired devices** lists sanitized friendly names, confirmed **Forget** and an
editable name for this device. Saving a name persists it; an active advertisement
uses the name from when sharing started, so restart sharing to advertise a new name.
Forget removes the local stored pairing; the other device's record is independent.
An existing stream is not stopped by Forget; use Stop sharing for immediate closure.

Nearby runs bounded background browse windows and refreshes while the View page
is open; replacement snapshots remove stale entries. The pure `ui` state module
and bounded local parsers are headless-testable. The routing worker keeps network
routes out of display rows. Nearby may not work on guest or shared Wi-Fi. Open
**Can't find your device? Use a link**, paste a private viewing link, and Connect.
The same fallback accepts a single-use `mightymonitor-pair://` offer copied from
a QR externally; the application does not scan QR codes yet. An offer still needs
Allow and matching-code confirmation on both devices.

Android pairing, physical multi-device/Wi-Fi/guest-network checks and Windows
runtime acceptance are not covered by automated tests. Egui rendering has not
been visually checked for this change. Home labels remain **Share this screen**
and **View a screen**; the Home redesign and text-size control remain separate work.
