# MightyMonitor project handover

Last reviewed: 25 October 2026. This is the starting point for a new contributor
or AI assistant. Verify repository/release state before relying on this snapshot.

## Intended finished app

An original, clean-room Rust application for Windows 11, Linux and Android
(the user's Android targets are 10, 11, 14 and 16). Devices on the same local
network should share, view and optionally control screens in either direction:

- Show a phone screen in a PC window and control it with keyboard/mouse.
- Show and control a Windows/Linux screen from a phone or another PC.
- Create real extra desktop space on a source PC and view it on another device.
  A mirrored window is not an extended display.
- Host and view concurrently: eventually Linux extends to Windows while Windows
  extends to Linux, with an Android session alongside them. Control and disconnect
  must remain scoped to each session, with no recursive capture or input loops.
- Wi-Fi is primary. Reuse IP over Ethernet, Android USB networking and supported
  Thunderbolt/USB4 networking. An arbitrary USB-C cable between two host ports
  does not guarantee a network link. Never promise zero latency.
- Simple View/Share navigation, minimal text, light neutral/green styling and a
  modern flat project icon. No blue/dark default. No proprietary product branding
  in marketing copy. Proprietary code/protocols/assets must not be copied.
- Local accounts/cloud services are unnecessary. The project website must stay
  plain HTML/CSS/JS usable from disk and GitHub Pages, without Sites hosting.

`CROSS_DEVICE.md` is the detailed product/acceptance specification. Android is an
extension *viewer*; a separate Android desktop is outside the initial scope.

## Verified state

PRs #38–45 are merged upstream. #44 added remote control and concurrent desktop
viewers; #45 added experimental Wayland extension. Check newer PRs before editing.

| Capability | Implementation and evidence | Remaining acceptance/limits |
| --- | --- | --- |
| Linux mirror/view | X11 and system-authorized PipeWire/Wayland; shared H.264 | Physical two-device Wi-Fi/recovery/latency matrix |
| Windows mirror/view | GStreamer DXGI capture and H.264 decoding | Physical Windows capture/control and cross-device tests |
| Android mirror/view | MediaProjection capture, MediaCodec decode, Rust JNI networking | PC→phone and phone→desktop real video passed on Android 10/11/14/16 **emulators**; physical phones unverified |
| Secure sessions | Pinned TLS 1.3, per-share private certificate/token link, QR, fresh identity per Share; desktop Resume bound to authenticated principal, bounded registry | Legacy plaintext Resume remains unauthenticated; physical recovery matrix unverified |
| Persistent pairing | Security API and desktop CLI: persistent identity/store, mutual TLS-exporter SAS confirmation, pinned paired view, forget, friendly names, code limiter and single-use QR-offer hooks; automated unit/loopback paired-video and Resume authorization tests | Discovery wiring, GUI and Android pairing absent; code-entry transport/QR rendering absent; physical devices and Windows runtime unverified; not released |
| Desktop control | Windows/X11 native input; Wayland RemoteDesktop portal; focused picture forwards input | Actual X11 injection/held-key cleanup passed on isolated Xvfb; physical Windows and interactive Wayland input unverified; primary/default desktop control only |
| Android control | User-enabled Accessibility service, optional user-selected IME, local per-share and viewer grants | Actual Android button tap, text entry and revocation passed on all four emulator versions; physical phones unverified |
| Concurrent viewers | Desktop host accepts four viewers, one controller at a time | Two encrypted H.264 viewers and controller handoff passed on Xvfb; Android host supports one viewer; opposite-direction multi-device acceptance remains |
| Linux Extend | Portal Virtual source, advertised capability and returned type checked; Mirror/Extend UI | Live KDE Wayland: real extra output appeared, three H.264 frames decoded over secure loopback, normal Stop removed output and restored original active displays. Layout/window movement, virtual-display input, other compositors and physical Wi-Fi unverified |
| Windows/X11 Extend | Explicit unsupported error | Actual virtual-output/driver implementation missing |
| Installers | Linux DEB, Windows per-user setup, Android debug/test APK | Builds pass, including Windows install/runtime/uninstall; production signing, stable release publication and update continuity not established |
| Website | Static `website/dist`; latest stable GitHub installer asset lookup | Keep status current; do not confuse workflow artifacts with published releases |

The full app is **not complete**. There is no defensible measured completion
percentage. Builds, emulator tests and a local live test are distinct evidence.

## Code map

- `core/`: versioned messages/framing, generic transports, reconnect, bitrate,
  normalized input events, platform traits. See PROTOCOL.md and ABR.md.
- `security/`: TLS 1.3, certificate pinning, private session invitations and
  asynchronous/blocking handshakes. `pairing.rs`: persistent identity and mutual
  SAS/paired TLS; `store.rs`: bounded atomic credential store and display models;
  `pairing_hooks.rs`: names, injectable clocks, code-entry limiter and single-use
  offers; `peer.rs`: transport-established principals; `device_cli.rs` and
  `bin/mm-devices.rs`: terminal consent/maintenance. See SECURE_CONNECTIONS.md.
- `host/src/resume.rs`: bounded expiring session-to-principal bindings;
  `host/tests/authenticated_resume.rs` and `paired_view.rs`: loopback evidence.
- `discovery/`: opt-in mDNS library, strict public TXT metadata and bounded browsing;
  not wired into host/viewer. See DISCOVERY.md. Real devices, Wi-Fi, Windows
  firewall and Android discovery are unverified. The backend normalizes duplicate
  wire TXT keys before the strict parser; see DISCOVERY.md for that boundary.
- `codec-gst/`: H.264 encoding/decoding and Windows capture adapter.
- `host-linux/`: X11 capture, Wayland portal/PipeWire, shared latest-frame capture,
  optional RemoteDesktop portal input and Virtual monitor support.
- `host/`: `mm-host`, capture/encoder pipeline, per-viewer sessions, native input,
  exclusive controller ownership and managed Stop lifecycle.
- `viewer/`: eframe/egui `mm-viewer`, View/Share, QR, decoder, focused input,
  child-host lifecycle. `examples/receive_smoke.rs` receives actual video.
- `android-bridge/`: original Rust phone host/client and JNI bridge.
- `android/`: native Java Activity, MediaCodec/MediaProjection foreground service,
  optional Accessibility/IME; debug-only InputTestActivity and instrumentation.
- `packaging/`, `scripts/`, `.github/workflows/`: installers and platform checks.
- `website/dist/`: deployable HTML/CSS/JS, bundled fonts and original icon.
- `docs/STATUS.md`: historical progress log, not a concise current support matrix.

Read REMOTE_CONTROL.md, EXTENDED_DISPLAYS.md, INSTALLERS.md, WINDOWS.md and
ANDROID instructions in `android/README.md` before changing their features.

## Security and lifecycle invariants

- Discovery service names, TXT short IDs and friendly names are unauthenticated
  routing/display hints, never identity. Authentication, revocation and Resume
  use the full 32-byte paired certificate fingerprint/authenticated principal.
  Resolve local short-ID selections to stored pins and reject ambiguity; connect
  using the route plus the store/offer pin. A different key fails terminally.

- A viewing link is a bearer secret. Do not broadcast it through discovery,
  store it in preferences, print it in diagnostics or put it in screenshots/PRs.
- TLS pin/token failures are terminal: no fallback, public trust bypass or downgrade.
  Legacy plaintext viewing is explicit opt-in and cannot enable remote control.
- Persistent pairing requires local 60-second mode and SAS confirmation on both
  devices. Stored pins/secrets never come from discovery or display names. Forget
  persists on each device; host rechecks it on connection and Resume. Store limits
  are 64 peers/16 KiB, with atomic writes, Unix 0600 and fail-closed corruption.
- Desktop TLS Resume binds a random ID to the transport-established principal;
  registry limits are 16 sessions/five minutes, with validated sequence bounds.
  Unknown/other-peer/revoked Resume never gets video. Legacy plaintext remains
  explicitly unauthenticated. No input or historical video is replayed.
- Pairing code/offer APIs have injectable clocks, expiry and single-use/attempt
  limits. Keep limiter state across reconnects; QR offers contain pin/route/nonce,
  never viewing tokens or pairing secrets. Local host confirmation is mandatory.
- Sharing is view-only by default. Control needs local host permission plus a
  separate viewer request. Android also requires real OS Accessibility consent;
  optional keyboard enable/selection stays with the user. Wayland uses OS consent.
- Disconnect, focus loss, revocation and Stop release held input. Reconnect drops
  stale input and needs a fresh control grant. Queues/frame buffers stay bounded.
- Four desktop viewers share exclusive controller ownership. A view-only peer
  must not release another viewer's keys on the shared Wayland input backend.
- Never automate real user input during tests on their active desktop. Android
  secure-setting commands in emulator instrumentation are test-only, not app code.
- Windows protected/elevated screens and Android protected content retain OS limits.

## Next work, in order of useful independent milestones

1. **Physical acceptance and usability:** install on Windows 11 and the supported
   phones; record actual Wi-Fi playback, motion latency median/p95, sustained
   sessions, rotation/scaling, focus, revocation, congestion and reconnection.
   Finish KDE virtual-monitor layout/input tests and another supported compositor.
   OS permission dialogs require the user; do independent work while waiting.
2. **Discovery wiring + GUI + Android:** wire nearby-device routing to the
   persistent-pairing API without broadcasting tokens or treating proximity as
   trust. Add local consent, short-code/QR presentation and lifecycle/network
   tests, including Android. Code-entry transport remains to be wired; preserve
   host-owned attempt limits and mandatory confirmation. Private links still work.
3. **Windows extended display:** implement/integrate a license-compatible virtual
   display driver and monitor/input mapping. Research driver signing/distribution.
   Do not automatically install a third-party root certificate or disable driver
   signature enforcement. Production signing/account/certificate access is an
   external dependency to explain concretely, not silently bypass.
4. **Concurrent product behavior:** explicit source selection, independent session
   identities and opposite-direction streams; prevent recursive capture/input;
   increase phone-host peers only with bounded-resource and lifecycle acceptance.
5. **Wired setup:** discover usable IP interfaces, explain Wi-Fi/Ethernet/USB
   networking, and test supported USB4/Thunderbolt plus Android tethering/tunnels.
   Document actual hardware support instead of assuming a Type-C cable works.
6. **Production delivery:** stable Android signing/update keys, Windows signing,
   dependency/license review, installer upgrade/uninstall tests, versioned
   published installer releases, then latest-release website download validation.
   Never commit signing credentials. Browser streaming is a later roadmap feature;
   the static website itself is not a screen viewer.

Do not leave later milestones as unexplained "pending": implement what can be
implemented, identify physical/hardware/signing dependencies precisely, and keep
this file's evidence matrix current as work lands.

## Build and meaningful checks

Use the Rust toolchain/MSRV and exact dependency versions specified in the repo
(current workspace MSRV is 1.85). Linux needs native GStreamer, PipeWire/SPA,
Clang, X11 and GUI development libraries; follow workflow dependency lists.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features android-bridge/jni --locked -- -D warnings
MM_REQUIRE_CODECS=1 cargo test --workspace --features android-bridge/jni --locked
node website/tests/downloads.cjs
```

`MM_REQUIRE_CODECS=1` makes codec availability meaningful. Ignored live tests are
not automatic passes. Run `host/tests/x11_input.rs` only on an isolated Xvfb with
`MM_ALLOW_INPUT_SMOKE=1` and sequential tests. Current platform workflows also
check Windows, PipeWire and MSRV. Android's workflow builds three ABIs and runs
three real video/control tests on APIs 29/30/34/36. Instrumentation must grant
Accessibility **after** its process restart and wait for the specific target
editor's IME connection; prebinding or accepting any stale IME caused failures.

For Linux Extend, start `mm-host --secure --extend --managed` with a local bind,
keep its stdin open, approve the virtual monitor, receive real H.264 video, send
`STOP`, and compare active displays before/during/after. A closed stdin immediately
stops a managed host. Keep its invitation private and delete test credentials.

Installer workflows are `linux-installer.yml` and `windows-installer.yml`.
Android builds use `android-viewer.yml`. They can be dispatched on a feature ref;
workflow artifacts are temporary development downloads, not stable releases.
Do not rely on another contributor's `/tmp` dependency paths or local binaries.
Recreate dependencies from the checked-in workflows. Avoid concurrent builds from
multiple worktrees into one cargo target directory because stale output can mislead.

## Repository and PR workflow

Upstream: `reduckyLang/MightyMonitor`. Contributors may work from forks.
Check `git remote -v`, clean/dirty state and applicable AGENTS.md before working.
Milestone PRs target upstream and are merged after review and green required
checks. Prefer `codex/` branch names. Do not discard local work.

Upstream uses **squash merge**. This preserves the source but changes ancestry.
PRs #40–45 needed synchronization when based on pre-squash feature history.
Before each new PR:

1. Fetch upstream main and confirm the previous prerequisite really merged.
2. Compare its reviewed branch tree with the squash merge/main tree.
3. Synchronize the new feature branch; resolve conflicts by inspecting code.
   Taking feature versions wholesale is safe only if tree identity was proven
   and no independent upstream changes would be lost. Never assume it.
4. Verify the PR diff contains only the new milestone and is mergeable. Push
   normally; avoid forced rewrites of someone else's work.
5. Run relevant checks, include concrete evidence and limitations, open and attach
   the PR. Avoid overlapping dependent PRs. Document the next merge order clearly.

No new scheduling/hosting service is needed to keep the website fresh: update it
with each milestone. Its release buttons must point only to real published
installers from the configured upstream repository, with honest unavailable states.

## Keeping this handover useful

At each milestone update: the date, support/evidence matrix, remaining work,
new module map entries, tests and known failure causes, relevant PRs, and website
status copy. Link this guide from README and keep AGENTS.md pointing here. Keep
personal conversation history, local machine identifiers, secrets and transient
execution-session details out of the repository. This guide should explain the
project sufficiently for a contributor with no access to previous chats.


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


2026-10-23 screen-playground identity and Pages root fix: the user rejected the
first linked-frame logo; the refined folded-screen M now has an editable SVG
master under assets/brand and synchronized PNG/Windows ICO exports for the
website, desktop/Linux packaging and Android. The website now renders original
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


## 25 October 2026 — persistent desktop pairing and authenticated Resume

Implemented security-crate and host CLI pairing independently of discovery.
Unit/loopback tests cover store restart/permissions/bounds/forget, SAS and
rejected authorization, friendly-name persistence, code lockout/cooldown,
single-use expiring offers, paired view-only fake video, and authenticated
Resume identity, revocation, sequence, expiry and eviction behavior. Core
reconnect and desktop viewer already learn the logical ID from HelloAck and
need no protocol changes. Discovery wiring, GUI and Android pairing remain
unimplemented. These checks do not establish physical-device acceptance,
Windows runtime acceptance or release publication. See STATUS.md for commands.
