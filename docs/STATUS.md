# Status

2026-10-18: repository bootstrapped (workspace, `core` skeleton, CI, docs, roadmap, issues). No milestone complete yet.

2026-10-18: M1-2 stream framing + FrameDecoder + docs/PROTOCOL.md implemented on branch feat/m1-2-framing.

2026-10-18: M1-1 workspace dependencies, stable toolchain, core module split, and CI concurrency configured.

2026-10-18: M1-4 frame-level async Transport + TCP implementation + loopback integration tests implemented on branch feat/m1-4-transport. Message-level API arrives with M1-3 integration.

2026-10-18: M1-2 review nits: isolated FrameError trait implementations, explained decoder validation, clarified poisoned-frame delivery, and added the 16 MiB payload boundary test.

2026-10-18: M1-5 frame-level reconnect interface, capped exponential backoff with injectable jitter, provisional Resume payload, bounded lifecycle events, and deterministic/unit + loopback recovery tests implemented. Typed Message::Resume integration awaits M1-3; mDNS remains M8.

2026-10-18: PR #16 review: reject oversized sends before connection work and preserve connections on local frame errors; only send I/O errors and write timeouts recover. Cancelled sends drop their transport. Backoff resets on the first received frame, delays have a 1 ms floor, and retry-budget docs count scheduled attempts (including cancelled sleeps). Added regression tests.
2026-10-18: M1-4 review nits: increased the loopback stream timeout, renamed the ping-pong roundtrip test, and documented send allocation, throughput, and sticky receive errors.

2026-10-18: M2-2 step A implemented: Linux-only host-linux crate, pure-Rust X11 GetImage capture with packed BGRA, monotonic frame metadata, optional pacing, fake-source unit tests, and Xvfb smoke coverage in CI. PipeWire portal capture remains the next PR.
2026-10-18: M2-0 reader/writer split implemented with a bounded command channel, prompt NotConnected during backoff, deadline-bounded writes, shared lifecycle state/events, and Send JitterSource (covers #19 items 3 and 4). Added deterministic backoff/concurrent-task, replacement-transport, cancelled-write, and static Send checks; existing unsplit reconnect and loopback behavior retained.
2026-10-18: M1-3 typed Message enum and input events integrated with bounded bincode encoding and validated frame kinds; kinds 0xfe/0xff reserved for protocol extension/escape; PROTOCOL_VERSION=2; provisional Resume replaced by Message::Resume kind 9 in reconnect sessions.

2026-10-18: M2-1 monotonic clock and caller-supplied offset estimator, TCP loopback latency/RTT bench, Linux netem helper, CI smoke, and docs/LATENCY.md added; release loopback results recorded (64 KiB p99 7.493 ms, 1 MiB p99 18.760 ms); 2 ms unpaced target remains unmet, real Wi-Fi and glass-to-glass measurements pending.
2026-10-18: PR #22 review: merged typed Message::Resume support into split sessions, made cancelled inline reader writes disconnect with Closed, and distinguished queue deadlines from write deadlines with regression tests and cancellation documentation.

2026-10-18: M1-6 ABR controller implemented on branch feat/m1-6-abr.

2026-10-18: PR #17 review: added absolute RTT slack to tolerate LAN jitter and unified ABR feedback with message::NetStats.

2026-10-18: M2-3 GStreamer H.264 codec crate added on feat/m2-3-h264: low-latency x264, live bitrate changes, upstream force-key-unit, optional probed NVENC/VAAPI with software fallback, avdec_h264 decoding, fallible frame APIs and core trait adapters. I420/RGBA input and packed I420 output; PSNR, bitrate, IDR and 1080p timing tests added. Local 1080p encode p50 ~3.9 ms (10 ms target, 40 ms sanity bound). Ubuntu CI installs GStreamer; Windows compiles an empty gated crate. See docs/CODEC.md for API/platform limits and measurements; hardware runtime remains unverified.

2026-10-18: M1-7 done: M1 wrap-up complete, feature-gated mock platform traits and TCP mock pipeline test added, CI doc job enabled on Linux and Windows, and protocol/contribution docs reviewed. M1 done.

2026-10-18: M2-2 step B implemented: optional Linux PipeWire/Wayland capture with ashpd ScreenCast consent, mapped BGRx/RGBx to opaque BGRA conversion, stride/chunk validation, monotonic metadata, bounded frame queue and session cleanup. Default builds require no PipeWire libraries. Fake conversion tests run without the feature; interactive Wayland smoke test is ignored by default. Dedicated Ubuntu Rust 1.85 feature build/clippy/test job added. Live Wayland validation requires a desktop session.

M2-2B validation: native dev libraries installed locally; fmt, default workspace clippy/tests/build, feature build/clippy and all-feature tests pass, including feature verification on Rust 1.85. Windows cross-target all-targets check passes; full cross-linking is unavailable locally because MinGW dlltool is absent. Interactive Wayland smoke remains unrun because this environment has no Wayland session.

2026-10-18: M2-4 host streaming pipeline implemented on feat/m2-4-host-pipeline.
Added the portable mightymonitor-host library and Linux mm-host binary using X11
capture and GStreamer H.264, core split reader/writer sessions, Hello/Resume
handshakes, Ping/Pong, NetStats-driven ABR, and forced keyframes on request and
reconnect. A one-frame latest-capture slot drops replaced/stale frames; exposed
metrics bound queue occupancy and measure capture-to-send-initiation age. Added
two queue unit tests and the three loopback acceptance tests with fake capture
and dummy encoding. Usage, CLI flags, architecture and resume/backpressure limits
are documented in docs/HOST.md. No new third-party packages; reused Tokio (MIT)
and async-trait (MIT OR Apache-2.0). Required workspace fmt, clippy with warnings
denied, mock-enabled tests, rustdoc with warnings denied, and build all pass.
Windows GNU cross-target all-targets cargo check also passes. Live X11-to-viewer
H.264 streaming, Wi-Fi/end-to-end latency, hardware encoding, and Windows linking
or runtime remain unverified; the existing X11 smoke test is ignored by default.
The roadmap tracks M2 as a whole, so its completion status is unchanged.

2026-10-18: M2-3 review follow-ups: codec-gst now gated on target_os = "linux" (empty crate elsewhere), `#![forbid(unsafe_code)]`, decode input/dimension bounds with overflow-checked even-size chroma math, pipeline setup without unwraps, and core `Encoder`/`Decoder` traits return `Result<_, CodecError>` so the adapters no longer panic. NOTICE and docs/CODEC.md note avdec_h264 (libav) is LGPL. Not done (out of scope): exact version pins, serial/looser latency tests, vah264enc migration, hardware encoder verification.

2026-10-18: M2-6 desktop viewer implemented on feat/m2-6-viewer. Added
mightymonitor-viewer / mm-viewer with pinned eframe 0.31.1, Linux codec-gst H.264
I420 decoding, RGB texture presentation, connection controls, lifecycle state,
fps/payload bitrate/RTT overlay and logged decode-to-texture-submission latency.
The GUI feature can be disabled for the headless generic core ViewerSession;
its split ReconnectingSession reader/writer uses the acknowledged session id and
last received sequence for Resume and waits for a keyframe after reconnect.
Bounded ordered encoded input and a latest decoded-picture slot limit backlog.
NetStats and matching Ping/Pong run at approximately one-second intervals.
Windows compiles the GUI and displays an explicit unsupported decoder error.
Added minimal TCP fake-host acceptance tests for reconnect/Resume/keyframe,
feedback cadence, invalid configuration/Closed, and I420 conversion. Updated
Linux CI GUI prerequisites and NOTICE. Also repaired inherited M2-4 call sites
for M2-3's now-fallible Encoder trait, propagating bitrate failures.

Validation: workspace fmt, clippy with warnings denied, tests, build, mock-feature
tests, PipeWire-feature build/clippy, all-feature tests, rustdoc with warnings
denied, Rust 1.85 all-targets workspace check and PipeWire-feature build,
and Windows GNU cross-target
all-targets clippy pass. Live desktop rendering, real host H.264 streaming,
Wi-Fi recovery/latency, physical presentation timing and Windows linking/runtime
remain unverified. Decode-to-present here means texture submission, not scanout.
M2-4 PR #29 is still unmerged; usage, dependency and manual smoke steps are in
VIEWER.md. Windows real decoding, input forwarding and negotiated colorimetry
remain future work; M2 as a whole is not complete.
