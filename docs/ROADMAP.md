# Roadmap

Cross-cutting requirement for every milestone: it must work **wirelessly over Wi-Fi/LAN with no cable**, with adaptive bitrate and automatic reconnects. Android uses wireless adb (adb pair / adb connect over Wi-Fi) and Wi-Fi streaming.

1. **Workspace, CI, core protocol.** *(In progress: M1-1, M1-2, M1-4, M1-5 merged; M1-6 in review; M1-3 and M1-7 open; see M1 follow-ups in docs/STATUS.md.)* Acceptance: workspace builds; CI green on Linux and Windows; versioned framing; platform traits compile; TCP loopback test passes; reconnect and bitrate-adaptation interfaces defined.
2. **Linux host to desktop viewer.** *(Planned; prerequisite: M1 follow-up (3), split reader/writer.)* Acceptance: H.264 stream over Wi-Fi/LAN; latency measured and documented first; adaptive bitrate reacts to congestion; viewer reconnects after a network drop.
3. **Windows host to desktop viewer.** Acceptance: WGC/DXGI capture, H.264, over LAN; adaptive bitrate and reconnect work.
4. **Input back.** Acceptance: mouse/keyboard over the control channel; uinput on Linux, SendInput on Windows; input survives reconnect.
5. **Android capture to desktop with control.** Acceptance: MediaProjection stream over Wi-Fi to desktop viewer; control via Accessibility or wireless adb (no cable); adaptive bitrate; reconnect.
6. **Android as client (PC to phone).** Acceptance: phone displays a PC stream over Wi-Fi and sends touch input back; reconnect.
7. **Browser viewer.** Acceptance: static page served by host works in a modern browser over LAN with mouse/keyboard/touch; adaptive bitrate; reconnect.
8. **mDNS discovery.** Acceptance: hosts advertise and viewers list devices automatically on LAN; wireless adb Android devices are discoverable and pairable without cable.
9. **Virtual extended monitors (optional, per OS).** Acceptance: EVDI/compositor output on Linux, indirect display driver on Windows; streams wirelessly.
10. **Polish, docs, installers, first release.** Acceptance: packaged installers, user docs, end-to-end wireless test matrix passes, tagged release.
