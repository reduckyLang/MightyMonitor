# Trinix – Multi Monitor Control

Trinix is a free, open-source application being built to share, control,
and extend screens across Linux, Windows, and Android devices over a local
network. View another device in a window, use your keyboard and mouse or touch
to control it, and turn another device into an additional display as these
capabilities become available.

Built in Rust, with local connections and no cloud accounts. Wi-Fi is the primary
connection method; wired networking is part of the planned experience. macOS and
iOS are not currently targeted.

## Quick start

This is an early test version. Installers are on the project's
[Releases page](https://github.com/reduckyLang/Trinix/releases) as an
unsigned pre-release. You need two devices on the same Wi-Fi.

1. Install Trinix on both devices (Windows, Linux or Android).
2. On the device that shares its screen, open the app and press **Share this screen**, then press **Copy connection link**.
3. Send the link to the other device, open Trinix there, paste the link under **View a screen**, and press **Connect**.

Step-by-step guides: [Getting started](docs/GETTING_STARTED.md), [Can't connect?](docs/HELP.md), [Glossary](docs/GLOSSARY.md).

## Goals
- Share a screen between Linux, Windows and Android (host to viewer, any direction).
- View an Android phone on a PC and control it from the PC.
- Stream a PC screen (or later a virtual extended monitor) to a phone or other device.
- **Fully wireless**: everything works over Wi-Fi/LAN with no cable, including Android (wireless adb, Wi-Fi streaming), with adaptive bitrate and automatic reconnects.
- Automatic device discovery on the LAN (mDNS).

## Platform matrix
| Platform | Host (shares screen) | Viewer (displays screen) |
|----------|----------------------|--------------------------------|
| Linux    | implemented, desktop validation pending | implemented, desktop validation pending |
| Windows  | implemented, physical-device validation pending | implemented, physical-device validation pending |
| Android  | capture alpha; emulator tested | viewer alpha; Android 10/11/14/16 emulator tested |
| Browser (ChromeOS, TVs, old laptops, Fire tablets) | - | planned |

## Contributor starting point

Read the [development guide](docs/DEVELOPMENT.md) for current evidence, missing
features, architecture, next milestones and the PR workflow.

## Architecture (Cargo workspace)
- `core`: protocol (serde + bincode, version byte first), framing, input events, platform traits (Capture, Encoder, Decoder, InputInjector), adaptive bitrate and reconnect logic.
- `host-linux`: X11 capture and a PipeWire portal backend, selected automatically for the desktop session; optional keyboard/mouse control is implemented.
- Windows host: GStreamer DXGI desktop capture with shared H.264/TCP streaming; optional keyboard/mouse control is implemented.
- `viewer`: egui/eframe viewer for Linux and Windows.
- Browser streaming: planned; the static project website is not a streaming viewer.
- `android-bridge` / `android`: shared Rust networking via JNI, with native Java UI and MediaCodec decoding. Phone capture uses MediaProjection and surface H.264; optional accessibility and keyboard control are implemented. See [Android build/install instructions](android/README.md).
- Networking: TCP with desktop nearby-device discovery and persistent pairing. QUIC remains planned.

The workspace includes `core`, Linux capture, GStreamer H.264 codecs, a host,
a desktop viewer, and latency tools. Linux host/viewer paths are implemented;
real desktop and Wi-Fi acceptance is still pending. See [docs/HOST.md](docs/HOST.md),
[docs/VIEWER.md](docs/VIEWER.md), [docs/WINDOWS.md](docs/WINDOWS.md), and [docs/ROADMAP.md](docs/ROADMAP.md).

The cross-device product requirements and acceptance gates are in
[docs/CROSS_DEVICE.md](docs/CROSS_DEVICE.md). Mirroring, remote control, and
extended displays are separate capabilities. Remote control is in alpha testing;
experimental Wayland virtual displays are implemented on supported desktops. KDE
creation, video streaming and cleanup passed a live local test. Windows and X11 extension are not implemented. See
[remote control](docs/REMOTE_CONTROL.md) and [extended displays](docs/EXTENDED_DISPLAYS.md).

## Clean-room policy
Our protocol and code are original. No code, protocols, drivers, or assets are
copied from proprietary applications. Third-party open-source work may be studied
only where its license permits, with appropriate attribution in NOTICE.

## Known limits
- Linux sharing supports X11 and system-authorized Wayland capture. Wayland requires a working desktop portal backend; physical-device validation is pending.
- Android can view PC streams and explicitly share its own screen. Remote control requires explicit accessibility permission and a separate per-share grant; Android control acceptance testing is still in progress.
- Experimental Linux Wayland extension needs a compositor that supports virtual monitors. Windows virtual displays require a separate display driver.

## Status
Early development: core foundation complete; Linux streaming components exist.
Windows capture and H.264 decoding are implemented with GStreamer; physical-device
validation is pending. An Android viewer APK is built and real desktop video
decoding passed on Android 10, 11, 14 and 16 emulators; physical phone tests
remain pending. Desktop and Android sharing use private session links and pinned
TLS 1.3. Desktop devices on the same Wi-Fi can use Nearby and one-time mutual
code confirmation, followed by stored pinned reconnects. Nearby may not work on
guest or shared Wi-Fi; pasting a private viewing link remains the fallback.
Android discovery/pairing and in-app QR scanning are not implemented. Physical
multi-device verification remains pending. Input forwarding is implemented and
undergoing platform acceptance testing. Windows/X11 virtual displays remain future work. Wayland extension is experimental. Legacy plaintext compatibility is opt-in in
the viewer. See [secure connection instructions](docs/SECURE_CONNECTIONS.md). See [docs/STATUS.md](docs/STATUS.md).

## License
Apache-2.0, see [LICENSE](LICENSE) and [NOTICE](NOTICE).

GStreamer, libav (gst-libav), and x264 are loaded dynamically from the user's
system at run time. They are never bundled or statically linked by this project.
The application itself is Apache-2.0. Supply-chain license compliance for Cargo
dependencies is enforced in CI via `cargo deny` (see `deny.toml`). See
[NOTICE](NOTICE) for the runtime components table and [docs/CODEC.md](docs/CODEC.md)
for per-platform codec strategy.


## Linux alpha installer

Debian/Ubuntu package generation and local installation instructions are in
[docs/INSTALLERS.md](docs/INSTALLERS.md). This packages the Linux development
prototype; it does not mean all cross-device features are complete.

## App interface

**Share this screen** and **View a screen** are separate destinations.
Nearby devices and paired-device management are in View a screen. Private-link
paste is under **Can't find your device? Use a link**. Quality/network settings
are under Technical details, and live connection statistics under Details. Android sharing requires
fresh system consent and includes a Stop action in its ongoing notification.

![Light desktop interface](docs/images/desktop-light.png)
