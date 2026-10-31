# Trinix development guide

Last reviewed: 25 October 2026. This is the starting point for a new contributor. Verify repository/release state before relying on this snapshot.

## Intended finished app

An original, clean-room Rust application for Windows 11, Linux and Android
(supported Android versions: 10, 11, 14 and 16). Devices on the same local
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

PRs #35–49 are merged upstream. #44 added remote control and concurrent desktop
viewers; #45 added experimental Wayland extension; #46–49 cover the website and
branding. Open draft PRs #51 (protocol v3) and #52 (mDNS discovery crate) are in
progress. Check newer PRs before editing.

| Capability | Implementation and evidence | Remaining acceptance/limits |
| --- | --- | --- |
| Linux mirror/view | X11 and system-authorized PipeWire/Wayland; shared H.264 | Physical two-device Wi-Fi/recovery/latency matrix |
| Windows mirror/view | GStreamer DXGI capture and H.264 decoding | Physical Windows capture/control and cross-device tests |
| Android mirror/view | MediaProjection capture, MediaCodec decode, Rust JNI networking | PC→phone and phone→desktop real video passed on Android 10/11/14/16 **emulators**; physical phones unverified |
| Secure sessions | Pinned TLS 1.3, per-share private certificate/token link, QR, fresh identity per Share; desktop Resume bound to authenticated principal, bounded registry | Legacy plaintext Resume remains unauthenticated; physical recovery matrix unverified |
| Persistent pairing | Security API and desktop CLI: persistent identity/store, mutual commit-reveal SAS confirmation (TLS exporter plus both nonces), pinned paired view, forget, friendly names, code limiter and single-use QR-offer hooks; automated unit/loopback paired-video and Resume authorization tests | Discovery wiring, GUI and Android pairing absent; code-entry transport/QR rendering absent; physical devices and Windows runtime unverified; not released |
| Discovery and pairing UI | Per-share opt-in advertisement, bounded background Nearby list, mutual code confirmation, paired view, QR offer/paste, countdown/Renew and confirmed Forget; pure model/parser and routed loopback tests | No real multi-device/Wi-Fi/guest-network or physical tests; Windows runtime and egui rendering unverified; Android and in-app QR scanning absent |
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
  `bin/trinix-devices.rs`: terminal consent/maintenance. See SECURE_CONNECTIONS.md.
- `host/src/pairing.rs`: per-share window, local decisions, routed device endpoint
  and advertisement metadata. `security/src/managed.rs`: strict bounded local
  command/status protocol. `viewer/src/devices.rs`: background browse/pairing
  workers. `viewer/src/ui/mod.rs`: pure Nearby/PairFlow/window models.
- `host/src/resume.rs`: bounded expiring session-to-principal bindings;
  `host/tests/authenticated_resume.rs` and `paired_view.rs`: loopback evidence.
- `discovery/`: opt-in mDNS library, strict public TXT metadata and bounded browsing;
  wired into desktop Share/Nearby. See DISCOVERY.md. Real devices, Wi-Fi, Windows
  firewall and Android discovery are unverified. The backend normalizes duplicate
  wire TXT keys before the strict parser; see DISCOVERY.md for that boundary.
- `codec-gst/`: H.264 encoding/decoding and Windows capture adapter.
- `host-linux/`: X11 capture, Wayland portal/PipeWire, shared latest-frame capture,
  optional RemoteDesktop portal input and Virtual monitor support.
- `host/`: `trinix-host`, capture/encoder pipeline, per-viewer sessions, native input,
  exclusive controller ownership and managed Stop lifecycle.
- `viewer/`: eframe/egui `trinix-viewer`, View/Share, QR, decoder, focused input,
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

- Discovery never broadcasts viewing links, tokens, offers, pairing secrets or
  full fingerprints. Advertisement only lives during secure sharing.
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
  limits. Live failures lock after three tries; local Renew replaces the window
  and invalidates old offers. Keep limiter state across reconnects; QR offers contain pin/route/nonce,
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
2. **Remaining discovery and UI work:** Android discovery/pairing, in-app QR
   scanning, Home redesign and text-size control; physical multi-device/Wi-Fi,
   guest-network and Windows runtime tests. Check egui layout/keyboard/accessibility
   on real desktops. Desktop discovery, consent, QR offer/paste and pinned reconnects
   are wired; private-link fallback remains. No typed code-entry mode is required.
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

## Where to find current tasks

- Tracking issue [#50](https://github.com/reduckyLang/Trinix/issues/50):
  ordered next tasks P1-P6, the Simplicity track (S1-S8), acceptance tests and
  follow-up checklists. Milestone issues are #2-#11; reviewer follow-ups are #19.
- `docs/ROADMAP.md`, section "Current work": task,
  branch, issue/PR link and status. Update it by docs PR after every claim or merge.
- `docs/STATUS.md`: dated progress log; its last entry is the current snapshot.
- GitHub labels: `claimed`, `blocked`, `follow-up`, `wireless`,
  `simplicity`, `area:*`, `milestone-N`; GitHub milestones M1-M10 group the milestone issues.

## Quick start for a new contributor

1. Read this file, `CONTRIBUTING.md` and `docs/CROSS_DEVICE.md`.
2. Run the checks in "Build and meaningful checks" below on a clean branch from `main`.
3. Pick an unclaimed task from #50 (or ask in the issue), add a `claimed` comment with
   your branch name, open a draft PR early, and keep ROADMAP/STATUS current.

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

For Linux Extend, start `trinix-host --secure --extend --managed` with a local bind,
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

Upstream: `reduckyLang/Trinix`. Contributors may work from forks.
Check `git remote -v` and clean/dirty state before working.
Work happens on a branch with a PR to upstream. Never push to `main`; nobody
pushes to `main` directly. PRs target upstream and are merged after review and
green required checks. Use a descriptive branch prefix such as `feat/`, `docs/`,
`ci/` or `fix/`. Do not discard local work.

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
