# Can't connect?

Work through these in order. Most problems are fixed by the first three.

## 1. Are both devices on the same Wi-Fi?

Both devices must be on the same Wi-Fi or network. Check the Wi-Fi name on each
one. Things that often get in the way:

- A **guest network**, or a router setting that keeps devices apart from each
  other. Move both devices to your main network.
- A **VPN** on either device. Turn it off for now and try again.
- A phone using mobile data instead of Wi-Fi.
- Public or hotel Wi-Fi, which often blocks devices from seeing each other.

## 2. Allow MightyMonitor through the firewall

The first time you share a screen, your computer may ask whether to allow
MightyMonitor to use the network. Choose **Allow**. If you chose **Cancel** or
**Block** by mistake, open your computer's firewall settings and allow
MightyMonitor on private networks (see "Other messages" below).
Then press **Stop sharing** and **Share this screen** again.

## 3. Get a fresh link

A link works only while that sharing is running. Press **Stop
sharing**, then **Share this screen** to make a new link, copy it and paste it
into the other device again. Paste the whole link, with no spaces before or
after it.

## 4. Restart

Close MightyMonitor on both devices and open it again. Open it after you are
connected to Wi-Fi, because the app looks up your network when it starts.
If it still does not work, restart your router, or both devices.

## 5. Try a different connection

- Move the devices closer to the router, or switch to the faster 5 GHz Wi-Fi
  name if your router has one.
- If a device can use a cable to the router, try that. It is more steady than
  Wi-Fi. Both devices still need to be on the same network.
- Close other programs that use a lot of the network, such as large downloads.

## Picture is slow or jumpy

The app lowers picture quality by itself when the network is busy and
reconnects after a short drop. For the smoothest picture, use a strong Wi-Fi
signal and keep both devices close to the router.

## Other messages

- **Firewall list shows `mm-host`**: that is the screen-sharing part of
  MightyMonitor. Allow it together with MightyMonitor.
- **Sharing component missing**: reinstall MightyMonitor. The sharing part is
  installed together with the app.
- **The video runtime is missing** (Windows): run the MightyMonitor installer
  again and keep your internet connected while it finishes.
- **The connection link is invalid or has expired**: get a new link as in step 3.
- **Screen sharing currently requires Linux or Windows**: on other systems you
  can still watch a screen, but not share one.
- **Windows says it protected your PC**: this early version is not signed yet.
  Choose **More info**, then **Run anyway**.

## Linux: sharing does not start on Wayland

Many Linux desktops use Wayland. There, the system itself must let
MightyMonitor capture the screen, using a service called PipeWire together with
your desktop's screen-sharing (portal) component. If no window asks which
screen to share, or sharing stops straight away:

1. Make sure PipeWire and the screen-sharing portal for your desktop are
   installed. For example on Debian or Ubuntu with GNOME, install
   `xdg-desktop-portal-gnome`; with KDE, `xdg-desktop-portal-kde`. Then log out
   and back in.
2. If it still fails, log out and, on the login screen, choose the **Xorg** or
   **X11** session for your account. Sharing works there without any extra
   permission window.
3. If a window asks which screen to share and you close it, sharing is cancelled
   on purpose. Press **Share this screen** again and choose a screen.

## Still stuck?

Search or open a report on the project's
[issues page](https://github.com/reduckyLang/MightyMonitor/issues). Say which
devices you use and what the app showed. Do not post your connection link in
public: anyone who has it can watch while sharing is on.

See also the [glossary](GLOSSARY.md) and [getting started](GETTING_STARTED.md).
