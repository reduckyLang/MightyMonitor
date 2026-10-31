# Roadmap

For the current implementation/evidence matrix and remaining work, start with
[DEVELOPMENT.md](DEVELOPMENT.md). The numbered milestones below describe
the original plan; they are not a current completion checklist.

See [CROSS_DEVICE.md](CROSS_DEVICE.md) for cross-device roles, secure control,
simultaneous sessions, wired transport constraints, and device acceptance gates.
The numbered list below is the original implementation roadmap; it does not
mean those features have passed physical-device acceptance.

Cross-cutting requirement for every milestone: it must work **wirelessly over Wi-Fi/LAN with no cable**, with adaptive bitrate and automatic reconnects. The normal Android application uses IP streaming without requiring adb. Wireless adb is a development or explicit fallback path.

1. [x] **Workspace, CI, core protocol (M1 done, 2026-10-18).** Acceptance: workspace builds; CI green on Linux and Windows; versioned framing; platform traits compile; TCP loopback test passes; reconnect and bitrate-adaptation interfaces defined.
2. **Linux host to desktop viewer.** Acceptance: H.264 stream over Wi-Fi/LAN; latency measured and documented first; adaptive bitrate reacts to congestion; viewer reconnects after a network drop.
3. **Windows host to desktop viewer.** Acceptance: WGC/DXGI capture, H.264, over LAN; adaptive bitrate and reconnect work.
4. **Input back.** Acceptance: mouse/keyboard over the control channel; uinput on Linux, SendInput on Windows; held input is released on disconnect; stale input is never replayed after reconnect.
5. **Android capture to desktop with control.** Acceptance: MediaProjection stream over Wi-Fi to desktop viewer; control via Accessibility or wireless adb (no cable); adaptive bitrate; reconnect.
6. **Android as client (PC to phone).** Acceptance: phone displays a PC stream over Wi-Fi and sends touch input back; reconnect.
7. **Browser viewer.** Acceptance: static page served by host works in a modern browser over LAN with mouse/keyboard/touch; adaptive bitrate; reconnect.
8. **Discovery and pairing.** Desktop Share advertisement, Nearby list, mutual
   six-digit confirmation, stored pinned reconnects and pairing-offer QR/paste
   are wired with automated library/model/loopback coverage. Guest/shared Wi-Fi
   may block Nearby; private-link paste remains the fallback. Android discovery
   and pairing, in-app QR scanning, physical Wi-Fi/guest-network acceptance and
   Windows runtime verification remain. Home redesign and text-size control are
   separate UI work. No release or physical-device acceptance is implied.
9. **Virtual extended monitors (optional, per OS).** Acceptance: EVDI/compositor output on Linux, indirect display driver on Windows; streams wirelessly.
10. **Polish, docs, installers, first release.** Acceptance: packaged installers, user docs, end-to-end wireless test matrix passes, tagged release.

## Current work

Snapshot dated 2026-10-25 (check `git log origin/main` for the current head). Task definitions, acceptance tests and
ordering live in tracking issue [#50](https://github.com/reduckyLang/Trinix/issues/50);
milestone issues are #2-#11 and reviewer follow-ups are in #19. Update this table
through a small docs PR whenever a task starts, opens a PR or merges.

| Task | Branch | Issue / PR | Depends on | Status |
|---|---|---|---|---|
| P1 M8 mDNS discovery + persistent pairing (3 PRs) | `feat/m8a-discovery` (part 1) | #9, draft PR #52 | none | in progress |
| P2 Protocol v3 + core hygiene | `feat/p2-protocol-v3` | #50, #19, draft PR #51 | none | in progress |
| S1 User-message catalog (`ux/` crate) | planned: `feat/s1-user-messages` | #50 | P2 (queue order) | queued, not started |
| P3 CI/supply-chain + pre-release pipeline | `ci/p3-supply-chain-prerelease` | #11 / draft PR #56 | P2, S1 (queue order) | draft PR open; adds a repeatable pre-release pipeline |
| S3 Android plain-language pass (incl. Android legacy toggle debug-only) | not started | #50 | P3 (queue order) | queued, not started |
| S4 Windows installer 7 -> 3 clicks | not started | #50 | S3 (queue order) | queued, not started |
| S2 Getting-started docs and copy | `docs/s2-getting-started` | #50, draft PR #54 | none | in progress |
| S5 Wire message catalog into viewer/host | not started | #50 | P1, S1 | after P1 |
| S6 One-screen Home, zero-settings Share | not started | #50 | P1, S5 | after S5 |
| S7 Pair by code / QR + Nearby list | not started | #50 | P1, S6 | after S6 |
| S8 Android nearby + QR | not started | #50 | S7, P1b | after S7 |
| P1b Android discovery (NsdManager / wireless adb listing) | not started | #9 | P1 | after P1 |
| P4 Host session hardening + capture robustness | not started | #50 | P2 | after P2 |
| P5 Extended display spike (X11 PoC, Windows design) | not started | #10 | none | waiting / not started |
| P6 Viewer batch | not started | #50 | P1, P2 | after P1 + P2 |

Download the latest build from the
[Releases page](https://github.com/reduckyLang/Trinix/releases). Test builds are
unsigned and include a Windows installer, a `.deb`, a debug APK and `SHA256SUMS`.
They were tested on emulators, Xvfb and one KDE session only; physical-device
and Wi-Fi acceptance are unverified. There is no
stable release, and production signing and a stable release are still open. Known
follow-ups: the Windows installer version string is fixed in the build script
(draft PR #56 adds a version input), and the `.deb` description text in
`scripts/package-linux.py` is outdated (it describes an X11-only development prototype).
Labels `claimed`, `blocked` and `follow-up` mark issue state on
GitHub. Legacy unencrypted viewing is an explicit opt-in (CLI plaintext mode and a
viewer setting) and is not limited to debug builds. S3 plans to make only the Android
viewer's legacy toggle debug-only; no planned task changes the CLI plaintext mode or the
desktop viewer setting. Use it only on a trusted LAN. The browser viewer (M7) is not started and keeps
its roadmap position.
