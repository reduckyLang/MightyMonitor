# H.264 codec (M2-3)

`codec-gst` provides synchronous GStreamer H.264 encoding and decoding on
Linux with GStreamer >=1.20 (appsrc queue limits). The deterministic default is software x264/FFmpeg.
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

CI installs these packages on Ubuntu. Windows and Android have no GStreamer dependency:
both the crate's code/tests and its dependencies are gated with `target_os = "linux"`.
The crate compiles as an empty crate on other platforms. A Windows viewer will need a subsequent Windows codec backend
or deliberate GStreamer packaging and removal of this gate.

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
The `x264enc` element is GPL. MightyMonitor loads these libraries dynamically
from the system and never bundles them. Bundling any of them needs a separate
license review. A future Windows or Android backend should avoid GPL
components (use a hardware or permissively licensed encoder). See NOTICE.
