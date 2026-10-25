# Roadmap

For the current implementation/evidence matrix and remaining work, start with
[PROJECT_HANDOVER.md](PROJECT_HANDOVER.md). The numbered milestones below describe
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
8. **mDNS discovery.** Library exists with unit tests; not yet wired into host/viewer, with no pairing yet. Acceptance: hosts advertise and viewers list devices automatically on LAN; wireless adb Android devices are discoverable and pairable without cable.
9. **Virtual extended monitors (optional, per OS).** Acceptance: EVDI/compositor output on Linux, indirect display driver on Windows; streams wirelessly.
10. **Polish, docs, installers, first release.** Acceptance: packaged installers, user docs, end-to-end wireless test matrix passes, tagged release.
