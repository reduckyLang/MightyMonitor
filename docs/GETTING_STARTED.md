# Getting started

Trinix lets one device share its screen with another device over your
own home or office Wi-Fi. It is free and needs no account. Your screen goes
straight to the other device. It does not go through any cloud service.
Trinix is meant for your own home or office network.

You need two devices on the **same Wi-Fi**. One device **shares** its screen.
The other device **sees** it. Pick your device below. Each list takes you from
install to your first shared screen in five steps or fewer.

This is an early test version. If something does not work, see
[Can't connect?](HELP.md). Unfamiliar word? See the [glossary](GLOSSARY.md).

> **Older test version:** v0.1.0-alpha.1 still uses the earlier name
> MightyMonitor. Its files are `MightyMonitor-0.0.1-alpha.1-windows-x64-setup.exe`,
> `mightymonitor_0.1.0-alpha.1_amd64.deb` and
> `MightyMonitor-0.1.0-alpha.1-android-debug.apk`, and it appears as
> MightyMonitor in the Start menu and app list. The Trinix names (`trinix_*.deb`,
> `trinix-host`, the Trinix Start menu entry) apply from the next release.

## Windows 10 and 11

1. Download the Windows installer from the project's
   [Releases page](https://github.com/reduckyLang/Trinix/releases) and
   open it. Windows may say it protected your PC, because this early version is
   not signed yet. If you got the file from the project, choose **More info**,
   then **Run anyway**. The first install also downloads video support, so you
   need internet once.
2. Follow Setup, then open **Trinix** from the Start menu.
3. To share your screen, choose **Share this screen** in the left menu. On that
   page, press the large button with the same name. If Windows asks to let
   Trinix use your network, keep **Private networks** ticked and choose
   **Allow access**.
4. Press **Copy connection link**. Send it to the other device in a chat or <!-- jargon-ok -->
   email. A link is a long private text that one device makes when it starts <!-- jargon-ok -->
   sharing. For a phone, you can also scan the square picture on this screen, but not
   every phone camera opens it.
5. On the other device, open Trinix. Stay on **View a screen**, paste
   the link, and press **Connect**. The shared screen appears. <!-- jargon-ok -->

To see another device's screen on this PC instead, do step 5 on this PC, using
a link from the other device. <!-- jargon-ok -->

## Linux (Debian and Ubuntu)

1. Install the file. Double-click the `.deb` file and press **Install**, or
   open a terminal in the folder with the file and run
   `sudo apt install ./trinix_*.deb`.
2. Open **Trinix** from your applications menu.
3. To share your screen, choose **Share this screen** in the left menu. On that
   page, press the large button with the same name. On some desktops, your
   system then asks which screen to share. Choose one and confirm.
4. Press **Copy connection link**. Send it to the other device in a chat or
   email. For a phone, you can also scan the square picture on this screen, but not
   every phone camera opens it.
5. On the other device, open Trinix. Stay on **View a screen**, paste
   the link, and press **Connect**. The shared screen appears. <!-- jargon-ok -->

To see another device's screen on this computer instead, do step 5 on this
computer, using a link from the other device. <!-- jargon-ok --> To stop at any
time, press **Stop sharing**.

## Android

1. Download the Android app file from the project's
   [Releases page](https://github.com/reduckyLang/Trinix/releases) and
   open it. This early version is not on Google Play, so Android asks you to
   allow installs from the app you used (for example your browser or Files).
   Allow it once, then press **Install**. If Android warns that the app is
   unknown, choose **Install anyway**.
2. Open **Trinix**. To see a computer's screen, first start sharing on
   that computer (steps 3 and 4 in the Windows or Linux list) and send the
   link to your phone. <!-- jargon-ok -->
3. On the phone, stay on **View**. Paste the link, or tap the link if it opens <!-- jargon-ok -->
   in Trinix.
4. Press **Connect**. The computer's screen appears on your phone.
5. To stop, press **Disconnect**.

To share your phone's screen instead, open the **Share** tab and press
**Share this screen**. When Android asks for permission, choose the button that
starts sharing (usually **Start now**). Then press **Copy connection link**,
send the link to the computer, and paste it there under **View a screen**. <!-- jargon-ok -->

Remote control, which lets the other device tap and type on your phone, is off
unless you turn on **Allow remote control**. Android then asks you to switch on
a setting in Accessibility.

## Good to know

- A link works only while sharing is running. If you press **Stop sharing**
  or close the app, start sharing again to get a new link.
- Anyone who has the link can see your screen while sharing is on. Send it only
  to the person you want to share with.
- The connection is encrypted by default. Use it only on a network you trust.
- Both Wi-Fi and a cable to the same router should work.
- Finding nearby devices automatically is planned. For now, you send a private
  link from one device to the other. On a guest or visitor Wi-Fi, devices may
  not be able to reach each other, so use your main Wi-Fi.
- Using a device as a second screen is still being developed. Today it is
  experimental, on some Linux Wayland desktops (KDE tested).
- The Android app file is only test-signed, so Android may warn about it. To
  install a newer version, you may need to uninstall the old version first.

<!-- technical-below -->
## More detail for technical readers

Note: the viewer has a setting named "older alpha (unencrypted)". It turns
encryption off. Leave it off unless you are connecting to an old test version
on a network you trust.

- [Private links and security](SECURE_CONNECTIONS.md)
- [Installers](INSTALLERS.md), [Windows](WINDOWS.md), [Linux](LINUX.md)
- [Desktop viewer](VIEWER.md), [Android app](../android/README.md)
- [Remote control](REMOTE_CONTROL.md), [extended displays](EXTENDED_DISPLAYS.md)
- [Roadmap](ROADMAP.md)
