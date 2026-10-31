# H.264 codec (M2-3)

`codec-gst` provides synchronous GStreamer H.264 encoding and decoding on
Linux and Windows with GStreamer >=1.20 (appsrc queue limits). The deterministic default is software x264/FFmpeg.
The gstreamer-rs crates are pinned to the 0.23 series (gstreamer 0.23.7,
gstreamer-app 0.23.5, gstreamer-video 0.23.6), with Cargo.lock committed.
The locked workspace is checked with Rust 1.85.0 as well as the repository's
`stable` rust-toolchain.toml; no newer GStreamer binding APIs are needed.

Ubuntu dependencies (development headers plus runtime plugins):

```sh
sudo apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-ugly gstreamer1.0-libav
cargo test -p codec-gst -- --nocapture
```

CI installs these packages on Ubuntu. On Windows the crate uses the official
GStreamer MSVC runtime (see [WINDOWS.md](WINDOWS.md) and the Windows section
below). Android has no GStreamer dependency: the crate's code, tests and
dependencies are gated with `any(target_os = "linux", target_os = "windows")`,
so it compiles as an empty crate on other platforms.

## API and frame contract

```rust
use codec_gst::{encoder::{Encoder, EncoderConfig, PixelFormat}, decoder::Decoder};
let mut encoder = Encoder::new(EncoderConfig {
    width: 640, height: 360, format: PixelFormat::I420,
    bitrate_kbps: 4000, ..Default::default()
})?;
let mut decoder = Decoder::new()?;
let raw = vec![128; 640 * 360 * 3 / 2];
let packet = encoder.encode_frame(&raw, 0)?;
let decoded = decoder.decode_frame(&packet.data, packet.timestamp_us)?;
assert_eq!((decoded.width, decoded.height), (640, 360));
encoder.set_bitrate_kbps(2000)?; // updates the running encoder
encoder.force_keyframe()?;      // next frame requests IDR plus SPS/PPS
Ok::<(), codec_gst::CodecError>(())
```

Configure fixed dimensions, pixel format, frame rate, bitrate (kbit/s), and
`key_int_max` at construction. Defaults: 1920x1080 I420, 30 fps, 8000 kbit/s,
GOP at most 30 frames. Dimensions must be positive, even, and at most 8192.
I420 input is tightly packed Y, U, V; RGBA input is tightly packed RGBA bytes.
The implementation copies rows into GStreamer's padded video layout and strips
padding on decode. Decoder output is tightly packed I420 with width/height.
Changing resolution or input format requires a new encoder.

Each `encode_frame` returns one complete Annex B byte-stream access unit,
including parameter sets on IDRs. `is_keyframe` reflects GStreamer's delta flag.
`decode_frame` accepts complete access units in stream order, starting with an
IDR containing SPS/PPS. Arbitrary H.264 streams with reordering/B-frames or
multiple access units per call are outside this synchronous one-in/one-out API.
Pass the capture timestamp (monotonic microseconds) through both calls: returned
timestamps preserve this caller metadata, independent of GStreamer's internal
time offsets. Submit increasing timestamps. Each codec is used by one caller;
the synchronous mutable methods allow only one outstanding input.

The core `Encoder`/`Decoder` traits return `Result` with a core `CodecError`
(boxed backend error, `Send + Sync`) for encoding, decoding and bitrate changes.
The `adapt` helper also propagates bitrate errors. Adapters use construction-time
settings and generate timestamps at the configured frame rate (30 fps for decoder).
Use the inherent methods to carry dimensions and timestamps. Invalid frame lengths and bitrate
values return errors before submission; pipeline errors/timeouts require
recreating the codec so delayed output cannot be mistaken for a later frame.
`encode_frame` and `decode_frame` can block up to 3 seconds waiting for output.
Decoder access units are limited to 16 MiB; output dimensions must be nonzero,
even, and at most 8192 each. Output packing uses checked size and row arithmetic. Pipelines transition to NULL on drop.

## Low latency and hardware

Software encoding uses `x264enc tune=zerolatency speed-preset=ultrafast bframes=0`
with two encoder threads, configurable `key-int-max` and bitrate. There are no
queue elements, no sink clock synchronization, and appsrc/appsink have a
one-buffer queue limit. The bitrate setter changes the live element property.
Force-keyframe sends an upstream GStreamer force-key-unit event on the encoder's
source pad with `all_headers=true`.

Set `prefer_hardware=true` to try installed `nvh264enc` then `vaapih264enc`,
configured without B-frames, before x264. A throwaway frame probes actual device
availability and immediate output; failed initialization/negotiation or a
three-second output timeout falls back. A fresh pipeline preserves the first
caller's IDR/headers. `backend()` reports the selected element. Hardware is
optional, driver-dependent, and unverified on this software-only host; fallback
and the software path are tested. Decode uses `avdec_h264 max-threads=1` to avoid
frame-thread buffering; hardware decoding is not implemented.

## Verification and measurements

Integration tests print `SKIP GStreamer codec tests: ...` when required runtime
plugins are absent; installed plugins with broken pipelines cause test failures.
Tests cover padded-width I420 roundtrips (PSNR >30 dB and dimensions/timestamps),
RGBA and core trait adapters, dynamic bitrate output size, IDR NAL detection after
force-keyframe, hardware preference/fallback, and 1080p encode latency.

On the local GStreamer 1.26.2 host (debug build), the synthetic I420 roundtrip
measured 67.7–69.4 dB. Increasing bitrate from 300 to 6000 kbit/s increased the
sampled noisy-frame output from 37,746 to 748,084 bytes. A 35-frame 1080p run,
discarding five warmup frames, measured encode p50 about 3.9 ms. Every frame is
logged. Timing includes row packing, submission, encoding and output copy, but
excludes synthetic frame generation. The engineering target is p50 <=10 ms;
only a generous p50 <40 ms sanity bound is asserted. Content, CPU and contention
change these figures; they are not end-to-end or Wi-Fi measurements.

## Licensing

The gstreamer-rs bindings are MIT/Apache-2.0. The GStreamer runtime is LGPL.
The `avdec_h264` decoder (libav/gst-libav) is LGPL, like the rest of the runtime.
The `x264enc` element is GPL. Trinix loads these libraries dynamically
from the system runtime (on Windows, the official GStreamer runtime installed by
Setup) and never bundles them in Trinix's binaries or installer.
Bundling any of them needs a separate license review. The application itself is
Apache-2.0. See NOTICE.

## Platform codec strategy

Each platform uses a codec path that does not bundle GPL libraries in our
binaries or installers, and prefers system or hardware acceleration where available.

### Linux

x264 is the default software encoder, loaded as a GStreamer system plugin
(`gst-plugins-ugly`). VAAPI (`vaapih264enc`) and NVENC (`nvh264enc`) are
preferred when the `prefer_hardware=true` flag is set and the driver is
present; the encoder falls back to x264 automatically. The decoder uses
`avdec_h264` from `gst-libav`, loaded dynamically from the system.

### Windows

The Windows host and viewer use the same `codec-gst` crate as Linux. Capture
uses GStreamer's `d3d11screencapturesrc` (Windows DXGI desktop duplication,
GStreamer 1.22 or newer). The host encodes H.264 with `x264enc` and the viewer
decodes with `avdec_h264`; both need `appsrc`/`appsink` and `videoconvert`
(see [WINDOWS.md](WINDOWS.md)).

The Windows Setup executable does not contain GStreamer. At install time it
downloads the pinned official GStreamer MSVC runtime over HTTPS, verifies its
SHA-256 and installs it for the current user. Setup requires
`gstreamer-1.0-0.dll`, `gstlibav.dll` and `gstx264.dll` to be present. The
runtime, including the GPL-licensed x264 plugin, is therefore installed and used
on Windows hosts, but it is obtained from the GStreamer project, loaded
dynamically, and not bundled or statically linked in Trinix's installer
or binaries. See [INSTALLERS.md](INSTALLERS.md) and NOTICE.

Possible future alternatives for Windows are Windows Media Foundation H.264 or
the Cisco OpenH264 runtime (BSD-2-Clause, runtime download under Cisco's terms).
Either needs a new backend and a code change and review before inclusion in an
installer.

### Android

Android encoding and decoding use **MediaCodec**, the platform's built-in
hardware codec API. No GStreamer dependency exists on Android; the
`codec-gst` crate is gated to Linux and Windows. MediaCodec is part of the Android
SDK and does not require any additional library.

