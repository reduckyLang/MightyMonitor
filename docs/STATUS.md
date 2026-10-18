# Status

2026-10-18: repository bootstrapped (workspace, `core` skeleton, CI, docs, roadmap, issues). No milestone complete yet.

2026-10-18: M1-2 stream framing + FrameDecoder + docs/PROTOCOL.md implemented on branch feat/m1-2-framing.

2026-10-18: M1-1 workspace dependencies, stable toolchain, core module split, and CI concurrency configured.

2026-10-18: M1-4 frame-level async Transport + TCP implementation + loopback integration tests implemented on branch feat/m1-4-transport. Message-level API arrives with M1-3 integration.

2026-10-18: M1-2 review nits: isolated FrameError trait implementations, explained decoder validation, clarified poisoned-frame delivery, and added the 16 MiB payload boundary test.

2026-10-18: M1-5 frame-level reconnect interface, capped exponential backoff with injectable jitter, provisional Resume payload, bounded lifecycle events, and deterministic/unit + loopback recovery tests implemented. Typed Message::Resume integration awaits M1-3; mDNS remains M8.

2026-10-18: PR #16 review: reject oversized sends before connection work and preserve connections on local frame errors; only send I/O errors and write timeouts recover. Cancelled sends drop their transport. Backoff resets on the first received frame, delays have a 1 ms floor, and retry-budget docs count scheduled attempts (including cancelled sleeps). Added regression tests.
2026-10-18: M1-4 review nits: increased the loopback stream timeout, renamed the ping-pong roundtrip test, and documented send allocation, throughput, and sticky receive errors.

2026-10-18: Milestone 1 progress snapshot, historical (main at 35ea74f; later PRs have since merged, see ROADMAP.md). Merged: #12 (M1-2 framing), #13 and #15 (M1-4 TCP transport/loopback), #14 (M1-1 workspace), #16 (M1-5 reconnect). Open at the time: #17 (M1-6 bitrate controller). Not done at the time: M1-3 (typed messages; PR #18 was closed unmerged) and M1-7 (CI hardening/mocks); these have since landed (M1-3 redo, #17, M1-7 and the reader/writer split in #22 are all merged; see the later entries below). Reviewer follow-ups are tracked in the "M1 follow-ups from review" issue and listed below. Milestone 2 is planned in issue #3; follow-up (3) (split reader/writer) was a prerequisite for streaming and is now done.

## M1 follow-ups
- [ ] (1) Send-only `ReconnectingSession` never resets its retry budget when `max_attempts` is `Some`.
- [x] (2, checked against main `core/src/reconnect.rs`: resolved by documentation/behavior, not by recovery) A `TransportError::Closed` returned by a transport send is not treated as recoverable. Only `Io` and write `Timeout` trigger recovery in both `ReconnectingSession::send` and the split writer path; other errors, including `Closed`, retain the transport and are returned to the caller. `Closed` is used as the disconnect reason when a cancelled inline write drops the transport. Original item: Treat `TransportError::Closed` from send as recoverable, or document that it is not.
- [x] (3, done by #22) `&mut self` means recv blocks send during backoff: needs a split reader/writer or channel design before the streaming milestones (M2 prerequisite).
- [x] (4, done by #22) `JitterSource` should be `Send`.
- [x] (5, done on main: PROTOCOL.md states Resume is unauthenticated and clarifies `last_seq == 0`) PROTOCOL.md: state that Resume is unauthenticated (bind to pairing/auth later), clarify `last_seq == 0`, reword "Kind/flags".
- [x] (6, done on main by M1-3: kinds 0xfe/0xff reserved, `PROTOCOL_VERSION` is 2) M1-3 should reserve kinds `0xfe` and `0xff`, and bump `PROTOCOL_VERSION` when the Resume kind is finalized.
- [ ] (7) Add a server-side Resume handler.
- [ ] (8) `lib.rs` hygiene: group `pub mod` lines then re-exports; add docs to `error.rs` and `traits.rs`.

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

2026-10-18: M2-4 post-main integration: updated host and test Encoder adapters to
core's Result signatures. Pipeline::on_stats now returns Result; sessions log
rejected live ABR changes and continue at the previous encoder setting, while
initial bitrate failures remain fatal. Recovered poisoned host locks and added
a capture-thread drop guard so backend panic closes/wakes the frame slot.
Added configurable post-handshake idle timeout (--idle-timeout-secs, default 30s),
renewed by received controls including Pings, and a stderr warning for resolved
non-loopback binds without authentication/encryption. Existing tests already
assert the constant max_queue_len == 1 bound. New tests cover bad Hello version,
wrong role, handshake timeout, capture panic termination, and idle timeout/Ping
renewal; existing Resume-first coverage retained. docs/HOST.md updated.
Verified locally: cargo fmt --all -- --check; cargo clippy --workspace
--all-targets -- -D warnings; cargo test --workspace; cargo build --workspace.
All passed, including six host loopback/pipeline integration tests. The X11 smoke
test remains ignored. Live desktop capture/viewing, Wi-Fi latency, hardware
encoding and Windows build/runtime were not revalidated in this follow-up.
Deferred review item (d): validation before pipeline/session resource creation
requires restructuring the current API and CLI capture-dimension setup; invalid
Hello version/role is rejected today, but after resources have been created.
A capture backend that never returns can still retain its thread; idle timeout
ends the session's wait, and in-progress writes remain bounded by write timeout.
