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
- [x] (7, implemented 2026-10-25; unit/loopback evidence) Server-side desktop TLS Resume binds random logical IDs to authenticated principals, checks revocation/sequence bounds and expires/evicts bounded entries. Explicit legacy plaintext remains unauthenticated.
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



2026-10-18: Android viewer implementation added: native Android Activity and
MediaCodec surface decoding, a JNI bridge into the shared Rust protocol, bounded
ordered video queue, Hello/Resume reconnect, configuration/keyframe gating, and
Ping/NetStats feedback. Rust loopback tests cover actual protocol events, reconnect
and shutdown under queue pressure. Android APK/emulator build workflow added;
APK compilation passed for arm64, 32-bit ARM and x86_64. Real PC H.264
playback passed on Android 10, 11, 14 and 16 emulators; physical phone
acceptance remains pending. This milestone does not include phone capture,
remote input, secure pairing, virtual displays or a production-signed release.


2026-10-18: Android screen hosting and light interface milestone added. Android
MediaProjection consent and a foreground capture service feed surface H.264 to
an original Rust protocol server. Native queue bounds, overflow IDR gating,
handshake, timed writes, keepalive, stop lifecycle and bitrate feedback are
implemented. Capture-to-Rust-to-Android-decoder tests passed on Android 10, 11,
14 and 16 emulators. Real desktop H.264 decoding of Android capture also passed on
all four emulator versions. Desktop and Android now separate View/Share with a light neutral
and green palette; technical settings are collapsed. Physical devices, secure
pairing, remote input, extended monitors, concurrent peers and production
Windows packaging remain incomplete.

2026-10-18: Windows desktop streaming implementation added: shared GStreamer H.264
encode/decode enabled on Windows, DXGI capture via d3d11screencapturesrc with
bounded appsink, validated/cropped packed RGBA, CLI monitor selection, and real
viewer decoding/host launch instead of unsupported placeholders. Windows CI
installs the pinned official 1.28.8 SDK with SHA256 validation and verifies codec
plugins before running tests. Linux synthetic-capture and buffer-boundary tests
exercise the common conversion path; a real Windows capture smoke is opt-in.
This is implementation progress, not physical Windows acceptance. Remote input,
secure pairing, virtual displays, and Windows installers remain separate milestones.
Linux Debian alpha packaging is included from main.


2026-10-19: Private session viewing invitations and pinned TLS 1.3 implemented
across desktop and Android. Authentication failures are terminal; there is no
secure-to-plaintext fallback. Android workers can cancel stalled TLS handshakes.
Local workspace tests include real H.264 over TLS, wrong pin/token rejection,
blocking/async interoperability, Android client/server video and stop lifecycle.
Platform CI/emulator validation is tracked in the milestone PR. Persistent device
pairing, remote control, extended displays and physical-device acceptance remain
incomplete; this is a viewing-security milestone, not a finished product.


2026-10-19: Linux host now selects portal-authorized Wayland capture automatically
when built with its default PipeWire feature. A bounded shared latest-frame source
keeps one portal session across viewer reconnects, handles idle desktops and
renegotiates resolution changes. Synthetic lifecycle/crop/resize tests supplement
native builds; interactive compositor acceptance remains pending.


2026-10-19: Remote-control milestone (development branch): optional desktop
keyboard/mouse input, focused-viewer forwarding, and Android accessibility/IME
input are implemented behind local and viewer grants. Wayland uses one system
RemoteDesktop/ScreenCast consent session. Desktop sharing accepts up to four
viewers with one controller at a time; disconnect/revocation releases held input.
Full workspace tests and warning-free clippy passed locally, along with actual
X11 input and two encrypted H.264 viewer sessions with controller handoff.
Android APK builds pass; automated Android control acceptance is currently blocked
by the test service not rebinding after instrumentation restarts the app. This
is not an Android control acceptance pass. Physical Windows/Android, interactive
Wayland input, virtual displays, discovery and production release signing remain
unfinished. See REMOTE_CONTROL.md for permission and platform details.


2026-10-19: Remote-control acceptance follow-up: secure phone control, real
Android button gestures, text entry through the selected optional keyboard, and
revocation passed on Android 10/11/14/16 emulators. Existing PC-to-phone video
and phone-to-desktop H.264 acceptance tests also passed on these versions. Test
setup now grants accessibility after instrumentation restarts the target process
and waits for the actual target editor's input connection. Shipped app permission
behavior is unchanged. Both desktop installer builds passed, including Windows
installation/runtime/uninstallation checks. Physical devices and interactive
Wayland input remain unverified. PR #44 is the remote-control review milestone.

2026-10-19: Experimental Wayland extension is implemented on the separate
`codex/virtual-display` branch after the remote-control milestone. It requests
the portal's Virtual source, checks advertised support and the returned stream
type, reuses encrypted H.264 viewing/control, and exposes Mirror/Extend in Share.
Unsupported X11/Windows extension fails before publishing a viewing invitation.
Local Linux build, clippy, rejection integration test and Windows cross-check pass.
This Linux desktop advertises virtual-monitor support, but actual creation,
streaming and removal require interactive system consent and remain unverified.
No Windows display driver or X11 extension has been added.


2026-10-19: Live KDE Wayland extension acceptance passed after system consent
was granted: a new virtual monitor appeared in KScreen, a secure desktop viewer
decoded three real H.264 frames, managed-host Stop exited with status zero, and
the virtual monitor disappeared with the original active display set restored.
This test used loopback and view-only sharing. Layout/window movement, virtual
monitor input, other compositors and physical cross-device Wi-Fi remain unverified.


2026-10-23 website redesign: the static site now uses a split opening with
original CSS perspective devices, moving local-connection paths, animated screen
widgets and user-selected Mirror/Control/Extend previews. Three original animated
feature illustrations replace the icon-only feature treatment. Neutral/green
styling, locally bundled fonts, inline SVG, keyboard focus and real GitHub release
installer filtering remain. Download cards now describe the actual EXE/DEB/APK
formats. A global pause control and system reduced-motion rule cover looping
artwork; the page pauses while hidden and starts paused without JavaScript.
Responsive browser checks covered 320/390/768-pixel widths and desktop; installer
filter/offline/rate-limit tests pass. This website milestone is isolated on current
upstream main so it can merge independently of ongoing app/driver work. See
website/DESIGN.md for design decisions and website/README.md for maintenance.


2026-10-23 screen-playground identity and Pages root fix: the first linked-frame
logo was replaced; the refined folded-screen M now has an editable SVG master under
assets/brand and synchronized PNG/Windows ICO exports for the website,
desktop/Linux packaging and Android. The website now renders original
3D device geometry, a shared landscape and purposeful Mirror/Control/Extend
content with a locally vendored MIT Three.js build (integrity/license recorded).
A manual rearrangement control and pointer perspective replace continuous device
bobbing. Rendering is limited to 30fps and stops offscreen, in hidden tabs and
when paused/reduced motion is active; manual changes still render while paused.
The CSS illustration remains the fallback. Alternating feature stories give the
graphics more space. Product alpha limitations and real release filtering stay.

Root index.html and .nojekyll support Pages configured as main / (root), instead
of showing README. The fixed relative redirect preserves repository subpaths,
query strings and anchors. The Actions deployment still uploads website/dist.
A real local browser check of /MightyMonitor/?preview=root#download reached
/MightyMonitor/website/dist/?preview=root#download with all assets. Automated
root-route and installer filtering tests pass; new website checks run on PRs.
Do not claim this is deployed until the PR is merged and Pages finishes.

## 23 October 2026 — generated website artwork revision

Replaced the earlier homemade devices and vague “make room” copy with five
image-generated studio assets. The hero moves a separate transparent window
between screens, with Mirror/Control/Extend paths and a clear Pause/Play control.
Removed the Three.js runtime; root Pages entry and refined app icon are preserved.
Prompt notes live in website/dist/assets/artwork/README.md. Download filtering and
root Pages regression checks pass. Artwork is illustrative; app acceptance status
is unchanged. Physical-device tests remain pending.

## Website refinement — 23 October 2026

Removed the rejected window-transfer animation and its playback controls.
Mode buttons now switch between generated concept scenes. No continuous hero
animation or render loop remains. Generated originals stay in local output.

## 24 October 2026 — animated scene preview

After PR #48 merged, added a separate website-only milestone on current upstream
main. Generated scenes now play as Mirror/Control/Extend chapters with camera
travel, directional wipes and a synchronized timeline. The rejected floating
window stays removed. Pause/Play, hidden/offscreen suspension and reduced-motion
handling are included. Local download and root Pages regressions pass.


## 2026-10-25 — persistent desktop pairing and authenticated Resume

Implemented persistent identity and bounded atomic paired-device storage in the
security crate, TLS-exporter SAS confirmation on both devices, pinned paired
viewing with constant-time secret authentication, local forget/forget-all, and
host/security-helper CLI commands. Added persisted friendly names and display-only
paired-device summaries, injectable-clock code-entry lockout/cooldown hooks and
single-use expiring QR-offer pinning hooks. Code-entry transport, QR rendering,
discovery wiring, GUI and Android pairing are not implemented here.

Desktop TLS Resume now uses random logical IDs bound to authenticated paired or
per-share private-link principals. The bounded registry rejects other peers,
unknown/expired/evicted IDs, revoked credentials and future sequences before
video. Fresh keyframes continue the server sequence without replaying input or
historical video. Explicit plaintext compatibility remains unauthenticated.
Automated loopback tests cover paired fake video and these authorization cases;
unit tests cover store corruption/limits/permissions/restart/forget, names,
code lockout and offer expiry/single use. Physical-device pairing, physical LAN
recovery, Windows runtime and Android pairing are unverified. Not released.

Validation passed on Linux with the shared Cargo target directory, one heavy
build at a time:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --features android-bridge/jni --locked -- -D warnings`
- `MM_REQUIRE_CODECS=1 cargo test --workspace --features android-bridge/jni --locked`
  (147 passed; four existing desktop/portal tests ignored, including input tests
  requiring isolated Xvfb; ignored tests are not acceptance evidence)
- `cargo build --workspace --locked`
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`
- `cargo +1.85.0 check --workspace --all-targets --locked`

The Android bridge explicitly rejects paired endpoints until its pairing
transport is implemented; existing private-link tests pass. No viewer/GUI,
discovery, README or website feature changes are included. No new dependency
packages or lockfile changes were required.

## 25 October 2026 — pairing identity lookup hardening

Paired reconnect now reloads credentials by the full 32-byte certificate
fingerprint. Local short-ID unpair resolves a unique stored entry before removing
its full fingerprint; ambiguous selections fail closed. Discovery names and TXT
IDs remain unauthenticated routing/display hints. Added loopback tests for a
different key behind matching display claims and colliding short IDs, and extended
Resume coverage for matching display claims from a different authenticated key.

Validation: formatting, workspace Clippy with JNI and warnings denied, workspace
tests with JNI and required codecs, workspace build, rustdoc with warnings denied,
and Rust 1.85 all-target checks passed sequentially. Four existing live desktop/
portal tests remain ignored. Physical-device, Windows runtime and discovery
integration acceptance remain unverified; discovery wiring is not implemented.
This change is not released.
## 25 October 2026 — current snapshot

As of 25 October 2026, PRs #35-#49 and #55 are merged on `main`. Merged work covers the protocol and
transport core (M1), Linux and Windows desktop host/viewer (M2-M4), Android capture and
Android viewer (M5-M6) at alpha level, plus Wayland sharing, remote control, secure
pairing links, Linux and Windows installers and the website. M2-M6 evidence is
loopback, Xvfb and emulator testing; physical Wi-Fi, Windows and phone acceptance is
unverified. The browser viewer (M7) is not started (issue #8). Open work: draft PR #51
(P2, protocol v3 and core hygiene) and draft PR #52 (P1 part 1, mDNS discovery crate).
Next-batch order, acceptance tests and assignments are in issue #50; the claimed-task
table is in ROADMAP.md.

Not yet done: persistent device pairing and discovery UI (M8, in progress), Windows
and X11 extended display (M9, spike not started), release signing and a stable
release (M10; pre-release [`v0.1.0-alpha.1`](https://github.com/reduckyLang/MightyMonitor/releases/tag/v0.1.0-alpha.1) is published, unsigned, built from
`357d22d`, tested on emulators, Xvfb and one KDE session only; the Windows installer
version string is still `0.0.1-alpha.1` and the `.deb` description text is outdated),
physical-device acceptance on Windows 11 and phones, and the Simplicity track (S1-S8
in #50). Of the #19 reviewer follow-ups, items (1) and (8) are planned in P2 and item (7)
is partly planned there (the Resume handling already exists in the host session in
`host/src/session.rs`; P2 adds documentation and a test hook). The #19 checklist on
GitHub has no items ticked yet, so treat the earlier STATUS entries (e.g. items 3
and 4) as the record for the rest. Supply-chain checks (`cargo-deny`, advisory for
bincode 1.3) are planned in P3 and P2.

Wireless operation over Wi-Fi/LAN remains a cross-cutting requirement for every
milestone.
