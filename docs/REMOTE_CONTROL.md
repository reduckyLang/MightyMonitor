# Remote control (alpha)

Sharing is view-only by default. Enable **Allow remote control** before starting
Share, send the private link to your other device, then enable **Control** in its
viewer. Only a secure connection can request control. A viewing link alone does
not activate input. Treat it as a secret: it lets its holder view the screen and,
when you have enabled control, request that permission.

On desktop, click the picture to focus it. Keyboard, mouse and scrolling then go
to the shared screen. Clicking elsewhere or switching away releases held keys
and buttons. Uncheck Control to return control; Stop sharing ends the session.
Desktop hosts support four viewers, with one controlling viewer at a time.
If another viewer controls the host, its control request is denied until the
current controller relinquishes control or disconnects.

## Platforms

- Windows: primary desktop control through the native input API. Windows security
  screens and applications running with higher privileges may reject input.
- Linux X11: desktop keyboard/mouse input. Actual key/button delivery and two-viewer
  handoff have passed tests on an isolated X server.
- Linux Wayland: screen and input access use the system RemoteDesktop portal.
  The compositor must support it and you must grant keyboard and pointer access.
  Capture can remain view-only if input access is denied. Interactive compositor
  input acceptance is still required.
- Android viewer: enable Control, touch the picture, and use Keyboard to type or
  send Enter. Hardware keyboard forwarding is also implemented.
- Android host: explicitly enable Trinix control in Android Accessibility
  settings, return to the app, and enable Allow remote control for this share.
  The optional Trinix keyboard improves text entry; enable/select it through
  Android keyboard settings. No permission or keyboard is enabled automatically.
  You can revoke control while sharing without stopping the video.

Android control APK builds and automated real touch/typing/revocation tests passed
on Android 10, 11, 14 and 16 emulators. Physical-phone acceptance is still required.
Android hosts currently accept
one viewer. Protected screen content and Android security dialogs remain subject
to Android's restrictions.

## Implementation

The `input-v1` capability negotiates HelloAckV2, ControlRequest and ControlStatus.
Input events validate normalized coordinates, HID keyboard codes and bounded
Unicode text. Local host consent, secure authentication and a separate active
viewer grant are required. Legacy plaintext viewers cannot activate control.

Input queues are bounded. Reconnects discard queued input and require a fresh
control request. Focus loss, revocation, disconnect and normal managed-host stop
release held remote input. Desktop controller ownership is exclusive across all
viewers, including the shared Wayland portal input backend.

The CLI requires both `--secure --allow-control`; desktop monitor selection is
currently limited to the primary screen for control. This feature controls an
existing screen. It does not create a virtual extended monitor.
