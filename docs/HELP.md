# Can't connect?

Try these in order. Test the connection after each one.

<a id="same-wifi"></a>
## 1. Check that both devices use the same Wi-Fi

Both devices must be on the same Wi-Fi or network. Check the Wi-Fi name on each
one. Things that often get in the way:

- A **guest network**, or a router setting that keeps devices apart from each
  other. Move both devices to your main network.
- A **VPN** on either device. Turn it off for now and try again.
- A phone using mobile data instead of Wi-Fi.
- Public or hotel Wi-Fi, which often blocks devices from seeing each other.

<a id="network-permission"></a>
## 2. Let MightyMonitor use the network

The first time you share a screen, your computer may ask if MightyMonitor can
use the network. On Windows, the choices are **Allow access** and **Cancel**.
Choose **Allow access**. If you chose **Cancel** by mistake, open your
computer's firewall settings and allow MightyMonitor on private networks. Then
press **Stop sharing** and press **Share this screen** again.

<a id="new-link"></a> <!-- jargon-ok -->
## 3. Make a new link <!-- jargon-ok -->

A link works only while sharing is on. Press **Stop <!-- jargon-ok -->
sharing**, then **Share this screen** to make a new link, copy it and paste it <!-- jargon-ok -->
into the other device again. Paste the whole link, with no spaces before or <!-- jargon-ok -->
after it.

<a id="restart"></a>
## 4. Restart the app

Close MightyMonitor on both devices and open it again. Open it after you are
connected to Wi-Fi, because the app looks up your network when it starts.
If it still does not work, restart your router, or both devices.

<a id="steady-connection"></a>
## 5. Get a steadier connection

- Move the devices closer to the router, or switch to the faster 5 GHz Wi-Fi
  name if your router has one.
- If a device can use a cable to the router, you can try that. Both devices
  still need to be on the same network.
- Close other programs that use a lot of the network, such as large downloads.

<a id="slow-picture"></a>
## Picture is slow or jumpy

MightyMonitor is designed to lower the picture quality on its own when the
network is busy, and to try again after a short drop. For a smoother picture,
stay close to the router and pause big downloads.

<a id="other-messages"></a>
## Other messages you may see

- **You see "mm-host" in your firewall list**: this is the screen-sharing part
  of MightyMonitor. Allow it together with MightyMonitor.
- **Sharing component missing**: reinstall MightyMonitor. The sharing part is
  installed together with the app.
- **The video runtime is missing** (Windows): run the MightyMonitor installer
  again and stay online until it finishes.
- **The connection link is invalid or has expired**: make a new link as in step 3.
- **Windows says it protected your PC**: this early version is not signed yet.
  If you got the file from the project, choose **More info**, then **Run anyway**.

<a id="linux-sharing"></a>
<!-- technical-below -->
## Linux: sharing does not start

On many Linux desktops (those that use Wayland), the system must give
permission before any app can capture the screen. This needs two parts:
PipeWire and a screen-sharing add-on (called a portal) for your desktop. If no
window asks which screen to share, or sharing stops straight away:

1. Install PipeWire and the screen-sharing portal for your desktop. On Debian
   or Ubuntu, GNOME needs `xdg-desktop-portal-gnome` and KDE needs
   `xdg-desktop-portal-kde`. Then log out and log back in.
2. If it still fails, log out. On the sign-in screen, choose the **Xorg** or
   **X11** session (look for a gear icon), then sign in. Sharing should work
   there, and it does not need a permission window.
3. If a window asks which screen to share and you close it, sharing is cancelled
   on purpose. Press **Share this screen** again and choose a screen.

## Still stuck?

Search or open a report on the project's
[issues page](https://github.com/reduckyLang/MightyMonitor/issues). Say which
devices you use and what the app showed. Do not post your connection link in
public: anyone who has it can watch while sharing is on.

See also the [glossary](GLOSSARY.md) and [getting started](GETTING_STARTED.md).
