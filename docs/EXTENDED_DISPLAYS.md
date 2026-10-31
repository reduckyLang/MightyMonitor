# Extended displays (experimental)

A virtual monitor adds desktop space: move a window onto it using your normal
system display layout, then view that monitor on another device. Mirroring only
shows an existing screen.

On supported Linux Wayland desktops, choose **Extend** in Share, then **Share extra
display**. Approve creating a virtual monitor in the system dialog. Open the
private viewing link on Windows, Linux or Android. The normal encrypted video
and optional remote-control paths are reused. Configure the monitor's arrangement
in Linux Display Settings. Stop sharing closes the portal session and its monitor.

The CLI equivalent is `trinix-host --secure --extend --bind YOUR_LAN_IP:5000`;
add `--allow-control` to request keyboard and pointer consent too.

This uses the standard ScreenCast portal's VIRTUAL source type. The app checks
that the compositor advertises virtual-monitor support and verifies that its
response identifies a virtual stream. It fails clearly instead of silently
mirroring when unsupported. Resolution and display creation are controlled by the
compositor.

## Verified locally

On 2026-10-19, a real KDE Wayland session passed system-approved virtual-monitor
creation, private TLS/H.264 streaming to the desktop decoder, and cleanup. KScreen
reported a new enabled virtual output; the decoder received three real video
frames. Normal managed-host Stop exited successfully, removed the virtual output,
and restored the original active display set. The test used loopback networking
and did not save screen images or publish the viewing credential.

Moving application windows between displays, other compositor implementations,
resolution/layout changes, virtual-monitor remote input and physical cross-device
Wi-Fi acceptance still need testing.

Windows virtual-monitor creation and Linux X11 extension are not implemented.
They cannot be enabled by changing the video transport. A Windows display driver
and its distribution/signing process are separate work. Android can view a Linux
virtual monitor; Android hosting still shares its existing phone screen.

Reference: [XDG ScreenCast portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html),
AvailableSourceTypes, SelectSources and stream source_type.
