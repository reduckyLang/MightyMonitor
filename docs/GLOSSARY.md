# Glossary

Short, plain explanations of words you may meet in MightyMonitor.

**Share**: let another device see what is on this device's screen.

**See another screen** (shown in the app as **View a screen** today): look at
another device's screen on this one.

**Connection link**: a long private text, a bit like a web address, that one <!-- jargon-ok -->
device makes when it starts sharing. Paste it on the other device to connect.
It works only while sharing is on. Keep it private: anyone who has it can see
your screen.

**QR code**: a square picture that holds the same link. Point a phone camera at <!-- jargon-ok -->
it instead of typing or pasting. Not every phone camera can open it.

**Connect / Disconnect**: start or stop seeing a shared screen.

**Stop sharing**: end sharing and make the old link stop working. <!-- jargon-ok -->

**Same Wi-Fi (same network)**: both devices are connected to the same router, so
they can reach each other. A cable to that router should also work.

**Guest network**: a separate Wi-Fi name that many routers offer for
visitors. It keeps devices apart from each other. Use your main Wi-Fi instead.

**Firewall**: a built-in guard that controls which programs other devices
can connect to. It may ask you to **Allow** MightyMonitor the first time you
share. Keep **Private networks** ticked.

**VPN**: a program that sends your internet traffic through another
location. It can keep two devices in the same room from reaching each other.
Turn it off while you use MightyMonitor.

**SmartScreen / "Windows protected your PC"**: a Windows warning about programs
from makers it does not know yet. This early version is not signed. If you
downloaded it from the project, choose **More info**, then **Run anyway**. Do
not do this for files from anywhere else.

**Remote control**: letting the other device use this device's mouse, keyboard
or touch screen. It is off until you turn it on.

**Second screen** (also called an extended display): use another device as a
second monitor, instead of copying your screen. Still being developed.

**Mirror**: show exactly the same screen on the other device.

**Wayland / X11**: two ways Linux desktops draw the screen. See
[Can't connect?](HELP.md) if sharing does not start on Wayland. If you see a
choice between them on the sign-in screen, X11 or Xorg is the one where sharing
works without extra setup.

**Reconnect**: the app tries again by itself after a short network drop.

<!-- technical-below -->
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
