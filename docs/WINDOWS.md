# Windows desktop streaming

The host and viewer now have Windows capture and H.264 paths rather than an
unsupported-decoder placeholder. These paths compile in the local GNU Windows
cross-check; physical Windows capture, playback, and network testing remain
required before claiming Windows support is release-ready.

## Build and run

Use 64-bit Windows 10/11, Rust's x86_64-pc-windows-msvc toolchain, and an official
64-bit MSVC GStreamer installation (runtime plus development components).
Windows capture requires the d3d11 plugin (GStreamer 1.22 or newer). Both the
encoder and decoder need appsrc/appsink, videoconvert, x264enc, and avdec_h264.
Keep GStreamer's bin on PATH and lib/pkgconfig on PKG_CONFIG_PATH when building
and running. For reproducible CI, scripts/windows/install-gstreamer.ps1 downloads
GStreamer 1.28.8, verifies the vendor-published pinned SHA256, installs its devel
profile, and verifies the required plugins before testing. It is a CI helper,
not the application's installer.

```powershell
cargo build --release --workspace
.\target\release\trinix-host.exe --secure --bind 192.168.1.10:5000 --fps 30 --bitrate 4000
.\target\release\trinix-viewer.exe
```

Use the host computer's actual LAN IP instead of the example. On a second Linux
or Windows computer, paste the private connection link printed by the host into the viewer. Windows firewall access
must allow the chosen local-network port. Keep this development listener on a local network. Secure links use pinned TLS
and expire when sharing stops; see [connection security](SECURE_CONNECTIONS.md).

The viewer can also start/stop its sibling trinix-host.exe from Share this screen.
Keep both executables together. Windows codec errors are now surfaced from the
actual GStreamer decoder rather than an unconditional unsupported message.

## Capture behavior

The original Rust capture adapter uses GStreamer's d3d11screencapturesrc to call
the native DXGI desktop-duplication backend. It requests system-memory raw video,
converts to RGBA, validates row strides and buffer bounds, and strips padding.
Only a final odd row/column is cropped to obtain the even dimensions required by
H.264/I420. A one-frame dropping appsink bounds capture backlog. Capture requests
are limited to 1–240 fps, monitor indices -1 and up, and dimensions 2–8192.

The default monitor is the primary display. `--monitor 0` (or another zero-based
index) selects a different display. `--monitor` selection is Windows-only.
A resolution change ends the current capture so reconnect can renegotiate it.
Capture startup/frame waits are bounded to three seconds. Pipeline cleanup sets
GStreamer to NULL; no raw pointers or new unsafe Rust were added.

This does not implement virtual extended monitors, remote input, or persistent
trusted-device pairing. It streams an existing display. CPU H.264 remains the default; hardware
encoder improvements are separate work.

## Acceptance on physical devices (pending)

1. Verify Linux -> Windows and Windows -> Linux moving video on Wi-Fi.
2. Verify Windows -> Windows. Confirm selected monitor, cursor, colors and size.
3. Interrupt Wi-Fi, restore it, and verify automatic reconnect plus a keyframe.
4. Change monitor resolution and reconnect. Try odd dimensions if available.
5. Start/stop sharing from the GUI and confirm no owned host process remains.
6. Lock/unlock the desktop; unplug a selected monitor. Confirm failure is readable.
7. Measure capture-to-display latency, CPU use and congestion response separately.

`cargo test -p codec-gst windows_desktop_capture_smoke -- --ignored` exercises
real Windows capture and must run in an interactive session. CI uses videotestsrc
to test the same sample/buffer path without pretending a hosted runner is a real
desktop. It also runs the actual encode/decode tests with installed plugins.

Platform references:
- https://gstreamer.freedesktop.org/documentation/d3d11/d3d11screencapturesrc.html
- https://gstreamer.freedesktop.org/download/
- https://gstreamer.freedesktop.org/documentation/deploying/windows.html
