# Linux capture

`host-linux` implements M2-2 step A with pure-Rust `x11rb` (MIT OR
Apache-2.0), without unsafe code or Xlib/C bindings. The crate and its dependencies
are Linux-only; on other targets it is empty.

## GUI and display build prerequisites

The desktop viewer (`mm-viewer`) uses OpenGL and supports X11 and Wayland. The
following development libraries are required to build and run it on Linux. CI
installs them on Ubuntu 24.04:

```sh
sudo apt-get install \
  libgl1-mesa-dev \
  libx11-dev \
  libxkbcommon-dev \
  libwayland-dev
```

These packages are in addition to the GStreamer development libraries listed in
[docs/CODEC.md](CODEC.md) and the PipeWire libraries listed below. See
[docs/VIEWER.md](VIEWER.md) for viewer usage and smoke-test instructions.

## X11 capture

`X11Capture::open(display)` connects to the requested display (`None` uses
`DISPLAY`) and captures the default screen's root window with xproto GetImage
in ZPixmap mode. Supported layouts have depth 24 or 32, 32 bits per pixel
(four bytes per pixel), and a TrueColor visual with three non-overlapping
8-bit color masks. Server byte order and color masks determine conversion.
Other layouts return `CaptureError::UnsupportedFormat`.

Output is tightly packed BGRA, four bytes per pixel, row-major, top to bottom,
with alpha forced to 255 and no row padding. Dimensions are fixed at connection
time; reopen after a screen resolution change. The cursor is not included.
There is no damage tracking yet: every grab transfers the entire root image.
GetImage incurs a server round trip, full image transfer, and CPU conversion;
large screens may be expensive.

`FrameSource` abstracts grabbing BGRA bytes for `LinuxCapture<S>`. The wrapper
validates frame lengths, returns metadata through `next_captured()`, and implements
core's `Capture::next_frame()` by delegating and returning only the bytes.
Callers retain the returned `CapturedFrame` when they need the last frame.
Errors become `None` in this interface; direct source calls retain typed errors.
`timestamp_us` is elapsed monotonic time since that wrapper was created, measured
with `Instant` after each grab, rather than wall-clock time. `with_fps(n)` limits
grab start frequency by sleeping between attempts; zero and the default are
unpaced. `last_grab_duration()` includes the source grab and pixel conversion,
including failed attempts, but excludes pacing sleep.

## X11 capture smoke test

With an existing X11 session:

```sh
cargo test -p host-linux -- --ignored x11_capture_smoke
```

With a virtual 1280x720, depth-24 display (install `xvfb` first):

```sh
xvfb-run -a -s "-screen 0 1280x720x24" cargo test -p host-linux -- --ignored x11_capture_smoke
```

Or start Xvfb manually, then stop it afterwards:

```sh
Xvfb :99 -screen 0 1280x720x24 &
xvfb_pid=$!
DISPLAY=:99 cargo test -p host-linux -- --ignored x11_capture_smoke
kill "$xvfb_pid"
```

The ignored test captures 30 frames, checks dimensions and BGRA size, and requires
at least 20 effective fps. If `DISPLAY` is unset it prints setup instructions
and returns without capturing. Unit tests use a fake source without a display.

## Wayland / PipeWire (M2-2 step B)

XWayland capture only sees X11 surfaces. For the native Wayland desktop, use portal-authorized
PipeWire capture. The host enables `pipewire` by default; the standalone capture
crate still keeps it optional. Linux host builds require native development libs:

```sh
sudo apt-get install libpipewire-0.3-dev libspa-0.2-dev clang pkg-config
cargo build --workspace --features pipewire --locked
cargo test -p host-linux --features pipewire -- --ignored wayland_capture_smoke
```

Run the smoke test inside a Wayland desktop with PipeWire, a session D-Bus,
`xdg-desktop-portal`, and the compositor's ScreenCast portal backend installed.
A portal dialog asks you to select and authorize one monitor. Cancellation or
missing services return `PipeWireError`; authorization is never bypassed or saved.
The ignored test requires interactive consent and a compositor producing frames
(move a window if needed); it cannot run under Xvfb.

```rust,no_run
use host_linux::PipeWireCapture;
let mut capture = PipeWireCapture::open()?;
let frame = capture.next_captured().ok_or("no frame within five seconds")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`open()` blocks during consent; call it off the UI/executor thread. A dedicated
worker owns the portal session, D-Bus runtime and PipeWire stream. Dropping the
capturer stops and joins the worker and closes the portal session. The stream
requests CPU-mapped BGRx/RGBx frames only; DMA-BUF-only, YUV, negative strides,
invalid chunks and unsupported layouts are discarded. No unsafe code is used
in this crate. PipeWire's bindings contain the native FFI.

Output is tightly packed opaque BGRA with per-frame negotiated dimensions, so
resolution changes do not require reopening. Row padding and chunk offsets are
respected. `timestamp_us` uses `Instant` elapsed from worker stream setup, after
conversion, before queueing; it is not the compositor's presentation timestamp.
A one-frame queue bounds memory; frames arriving while it is full are dropped.
`poll_captured(timeout)` distinguishes an idle desktop from a closed stream.
The host retains a single latest frame across viewer reconnects.
`next_captured()` waits up to five seconds and returns `None` on timeout or worker
termination. The core `Capture` implementation returns the same frame bytes.
The cursor is embedded when the portal supports it, otherwise hidden. Conversion/stride/timestamp tests run without the feature,
native libraries, or a desktop.

Both native dependencies are optional and Linux-target-only, so default Linux
and Windows builds need no PipeWire headers or libraries. ashpd 0.10.2 (MSRV
1.75) and pipewire 0.8.0 (MSRV 1.65) are pinned MIT releases. Newer transitive
releases can require newer Rust; the checked-in lockfile and MSRV-aware resolver
preserve the workspace's Rust 1.85 minimum. The dedicated CI job builds, clippies
and tests the feature on 1.85. `rust-toolchain.toml` tracks stable; Cargo metadata
is the source of the declared MSRV.
