# MightyMonitor

Free, LAN-only, open-source, spacedesk-style screen sharing written in Rust. Share a screen between Linux, Windows and Android in any direction. No cloud, no accounts, no cost. macOS and iOS are not targeted.

## Goals
- Share a screen between Linux, Windows and Android (host to viewer, any direction).
- View an Android phone on a PC and control it from the PC.
- Stream a PC screen (or later a virtual extended monitor) to a phone or other device.
- **Fully wireless**: everything works over Wi-Fi/LAN with no cable, including Android (wireless adb, Wi-Fi streaming), with adaptive bitrate and automatic reconnects.
- Automatic device discovery on the LAN (mDNS).

## Platform matrix
| Platform | Host (shares screen) | Viewer (displays and controls) |
|----------|----------------------|--------------------------------|
| Linux    | planned | planned |
| Windows  | planned | planned |
| Android  | planned | planned |
| Browser (ChromeOS, TVs, old laptops, Fire tablets) | - | planned |

## Architecture (Cargo workspace)
- `core`: protocol (serde + bincode, version byte first), framing, input events, platform traits (Capture, Encoder, Decoder, InputInjector), adaptive bitrate and reconnect logic.
- `host-linux`: PipeWire (ashpd) with X11 fallback, GStreamer H.264, uinput.
- `host-windows`: Windows.Graphics.Capture / DXGI, H.264, SendInput.
- `viewer-desktop`: egui/eframe viewer for Linux and Windows.
- `web-viewer`: static page served by the host (WebRTC/WebCodecs).
- `android`: Rust library (cargo-ndk, uniffi) plus a thin Kotlin app (MediaProjection, MediaCodec, Accessibility or wireless adb for input).
- Networking: tokio TCP first, QUIC (quinn) later. Discovery: mdns-sd.

Only the `core` crate exists so far. See [docs/ROADMAP.md](docs/ROADMAP.md).

## Clean-room policy
Our protocol and code are original. Nothing is copied from spacedesk or any proprietary app. Open-source projects (scrcpy, Weylus, Sunshine) may be studied only if their license allows, with attribution in NOTICE.

## Known limits
- Wayland requires a permission prompt per capture session.
- Android sharing needs one-time user approval; remote control needs the Accessibility service or (wireless) adb.
- Virtual extended monitors need OS-specific drivers and stay optional.

## Status
Bootstrap phase. See [docs/STATUS.md](docs/STATUS.md).

## License
Apache-2.0, see [LICENSE](LICENSE) and [NOTICE](NOTICE).
