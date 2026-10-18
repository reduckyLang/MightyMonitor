# Linux capture

`host-linux` implements M2-2 step A with pure-Rust `x11rb` (MIT OR
Apache-2.0), without unsafe code or Xlib/C bindings. The crate and its dependencies
are Linux-only; on other targets it is empty.

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

## Smoke test

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

## Wayland

This backend captures X11 sessions. Under XWayland, visibility is partial and
must not be treated as capture of the full Wayland desktop. Native Wayland
capture through the PipeWire portal is planned for the next PR, with the
portal's user-consent prompt. No PipeWire support is included here.
