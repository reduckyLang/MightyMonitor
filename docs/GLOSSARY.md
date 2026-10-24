# Glossary

Short, plain explanations of words you may meet in MightyMonitor.

**Share (show my screen)**: let another device see what is on this device's
screen.

**View (see another screen)**: look at another device's screen on this one.

**Connection link**: a long private web-style address that one device creates
when sharing starts. Paste it on the other device to connect. It works only
while that sharing is running.

**QR code**: a square picture that holds the same link. Point a phone camera at
it instead of typing or pasting.

**Connect / Disconnect**: start or end watching a shared screen.

**Stop sharing**: end sharing and make the old link stop working.

**Same Wi-Fi (same network)**: both devices are joined to the same router, so
they can find each other. A cable to that router counts too.

**Guest network**: a separate Wi-Fi name on many routers where devices cannot
see each other. Avoid it for MightyMonitor.

**Firewall**: a built-in guard that blocks unknown programs from using the
network. It may ask you to **Allow** MightyMonitor the first time you share.

**VPN**: a program that sends your internet traffic through another place. It
can stop devices in the same room from finding each other.

**SmartScreen / "Windows protected your PC"**: a Windows warning about programs
from publishers it does not know. This early version is not signed yet.
Choose **More info**, then **Run anyway** if you downloaded it from the project.

**Remote control**: letting the viewing device use the shared device's mouse,
keyboard or touch. It is off unless the sharing device allows it.

**Extra screen (extended display)**: using another device as an additional
monitor instead of copying your screen. Still being developed.

**Mirror**: show exactly the same screen on the other device.

**Wayland / X11**: two ways Linux desktops draw the screen. See
[Can't connect?](HELP.md) if sharing does not start on Wayland.

**Reconnect**: the app tries again by itself after a short network drop.

## Technical terms

These appear in project documentation.

**IP address**: the number that identifies a device on a network, for example
`192.168.1.10`.

**Port**: a numbered door on a device. An address plus a port, such as
`192.168.1.10:5000`, says exactly where to connect.

**LAN**: local area network, meaning your home or office network.

**TCP**: the network method MightyMonitor uses to carry data today.

**TLS**: encryption that keeps the connection private. MightyMonitor uses TLS
1.3.

**Certificate fingerprint / SHA-256**: a short code that identifies a sharing
device, so the viewer can be sure it reached the right one. It is part of the
connection link.

**Codec / H.264**: a way to shrink video so it fits over the network.

**Bitrate (kbit/s)**: how much data per second the video uses. Higher usually
looks better and needs a stronger connection. The app adjusts it automatically.

**Frames per second (fps)**: how many pictures per second make up the video.

**Latency**: the delay between something happening on the shared screen and
you seeing it.

**GStreamer**: an open-source video library used for capture and playback.
On Windows, Setup downloads it from the GStreamer project.

**PipeWire / portal**: the Linux services that let programs capture the screen
with the user's permission on Wayland.

**mDNS**: a way for devices to announce themselves on the network so they can be
found without typing addresses. Planned; see the [roadmap](ROADMAP.md).

**Listener**: a program part waiting for another device to connect.

**Session**: one run of sharing, from **Share this screen** to **Stop sharing**.
