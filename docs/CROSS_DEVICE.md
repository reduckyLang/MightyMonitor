# Cross-device product requirements

This document defines intended behavior, not currently supported features.
Trinix is an original Rust implementation with an original protocol.
Do not copy proprietary spacedesk code, protocols, drivers, or assets. Record
third-party dependencies and source provenance in NOTICE with license review.
Small platform API adapters may be necessary around the shared Rust core.

## Device roles and modes

Windows and Linux PCs, and Android phones, should each be able to host a screen
and view a remote screen. A PC should show an Android screen in a window and
send keyboard/mouse actions without requiring phone interaction for each action.
Android should display and control a PC. OS capture consent, enabled services,
and security restrictions still apply; setup must explain these requirements.

Mirror mode streams an existing display. Extend mode creates an independent
virtual display on the source PC, which the OS treats as additional desktop
space, and streams that display. Opening a mirrored window is not extension.
Android is an extension viewer; creating an Android extended desktop is outside
the initial scope.

A Windows PC and Linux PC must eventually host and view at the same time.
For example, Windows display 2 is shown on Linux while Linux display 2 is shown
on Windows. Each stream has an independent session ID, display ID, connection
state, permissions, and bitrate policy. Never capture the receiving viewer as
part of its own source display; make source selection explicit. Multiple peers
must not silently replace each other's sessions.

Input belongs to an explicitly focused remote session. Scope coordinates to its
selected display; map scaling, rotation, and touch deliberately. Release held
keys/buttons on disconnect, focus loss, or revoked permission. Reconnect must
not replay stale input or silently grant control. Prevent input feedback loops
when both devices host and view concurrently.

## Transport

Wi-Fi/LAN is the primary transport. Reuse the IP protocol over Ethernet and
supported Thunderbolt/USB4 host-to-host network links. An arbitrary USB-C cable
and two USB-C ports do not guarantee networking: controller capabilities,
cable support, drivers, and OS configuration must be detected and documented.
Android USB networking or an explicitly authorized adb tunnel can be a later
fallback, with a separate setup/test path. Do not assume wireless adb is an
always-available prerequisite for the normal Android app.

Wired connections still incur capture/codec/presentation latency. Measure
capture-to-display latency and report median/p95 under motion and congestion;
texture submission or transport-only timing is insufficient. Avoid zero-lag
claims and distinguish simulation from device measurements.

## Delivery sequence and acceptance

Submit independently reviewable PRs against current upstream main. A later PR
must not assume an earlier open PR has merged. Keep each milestone incomplete
until its acceptance evidence exists; CI alone cannot verify physical devices.

1. **Linux baseline:** run real X11 host/viewer on two devices over Wi-Fi;
   record dependencies, resolution/fps, hardware, latency, sustained playback,
   bandwidth throttling, and recovery after network interruption. Separately
   validate Wayland; the CLI selects Wayland capture automatically and requires system portal consent.
2. **Secure sessions:** authenticated pairing, encrypted transport, explicit
   view/control permissions and revocation. Test rejected peers, reconnect
   authorization, and independent session identity before enabling remote input.
3. **Windows streaming:** native capture, H.264 encode/decode, source selection,
   and hardware/software fallback. Verify Linux-to-Windows and Windows-to-Linux
   with real playback, congestion, and reconnect; a cross-compile is insufficient.
4. **Desktop input:** Windows and Linux mouse/keyboard forwarding with scoped
   coordinates and held-input cleanup. Validate focus loss, disconnect,
   permissions, keyboard layouts, scaling, and relevant Wayland limitations.
5. **Android host/control:** MediaProjection and MediaCodec integration with
   consent and lifecycle handling, plus user-enabled Accessibility or an
   explicitly configured adb control backend. Test phone-to-both-PC-platforms,
   rotation, text entry, touch gestures, disconnect, and permission revocation.
6. **Android viewer:** PC video to phone, touch-to-pointer mapping, keyboard,
   rotation/scaling, and reconnect. Validate both Linux and Windows hosts.
7. **Extended displays:** Windows indirect display driver and a supported Linux
   virtual-output implementation. Verify independent desktop space, display
   arrangement, resolution changes, cleanup, and reconnect. Document supported
   compositors; do not claim universal Wayland compatibility. Account for Windows
   driver signing, installation, and distribution before release.
8. **Concurrent devices:** separate host/viewer sessions and independently scoped
   control; validate two opposite-direction extended streams plus an Android
   session, session-specific disconnect/reconnect, and bounded CPU/memory/queues.
9. **Discovery, wired setup, packaging:** discover and pair devices without
   granting trust merely because they are nearby. Test Ethernet and supported
   USB4/Thunderbolt links separately from Wi-Fi, and Android USB fallback if
   implemented. Publish installers and a reproducible platform test matrix.

## Platform references

- Windows virtual displays: https://learn.microsoft.com/en-us/windows-hardware/drivers/display/indirect-display-driver-model-overview
- Android capture consent: https://developer.android.com/media/grow/media-projection
- Android gesture APIs: https://developer.android.com/reference/android/accessibilityservice/AccessibilityService
- USB4/Thunderbolt networking: https://docs.kernel.org/6.16/admin-guide/thunderbolt.html
- USB host/device architecture: https://www.kernel.org/doc/html/latest/driver-api/usb/gadget.html
