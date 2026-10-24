# Getting started

MightyMonitor shows the screen of one device on another device, over your own
home or office Wi-Fi. It is free, needs no account, and nothing leaves your
network.

You need two devices on the **same Wi-Fi or network**. One device **shows its
screen**. The other device **sees it**. Pick your device below. Each list takes
you from install to your first shared screen in five steps or fewer.

This is an early test version. If something does not work, see
[Can't connect?](HELP.md). Unfamiliar word? See the [glossary](GLOSSARY.md).

Automatic discovery of nearby devices is planned (see the
[roadmap](ROADMAP.md)). Until it arrives, you send a private link from one
device to the other, as described below.

## Windows 10 and 11

1. Download the Windows installer and open it. Windows may say it protected
   your PC because this early version is not signed yet. Choose **More info**,
   then **Run anyway**. The first install also downloads some video support, so
   you need internet once.
2. Follow Setup, then open **MightyMonitor** from the Start menu.
3. To show your screen, choose **Share this screen** in the left menu, then
   press the big **Share this screen** button. If Windows asks about network
   access, choose **Allow**.
4. Press **Copy connection link**, or scan the picture (QR code) with your
   phone. Send the link to the other device in a chat or email.
5. On the other device, open MightyMonitor, stay on **View a screen**, paste
   the link and press **Connect**. Your shared screen appears.

To see another screen on this PC instead, do step 5 here with a link from the
other device.

## Linux (Debian and Ubuntu)

1. Install the package. Double-click the `.deb` file and press **Install**, or
   open a terminal in the folder with the file and run
   `sudo apt install ./mightymonitor_*.deb`.
2. Open **MightyMonitor** from your applications menu.
3. To show your screen, choose **Share this screen** in the left menu, then
   press the big **Share this screen** button. On some desktops (Wayland) your
   system asks which screen to share. Choose one and confirm.
4. Press **Copy connection link**, or scan the picture (QR code) with your
   phone. Send the link to the other device in a chat or email.
5. On the other device, open MightyMonitor, stay on **View a screen**, paste
   the link and press **Connect**. Your shared screen appears.

To see another screen on this computer instead, do step 5 here with a link
from the other device. To stop at any time, press **Stop sharing**.

## Android

1. Install the Android app file. This early version is not on Google Play, so
   Android asks you to allow installing from the app you used to open the file
   (for example your browser or Files). Allow it once, then press **Install**.
   Installer files are listed on the project's
   [Releases page](https://github.com/reduckyLang/MightyMonitor/releases) once
   they are published.
2. Open **MightyMonitor**. To see a computer's screen, first start sharing on
   that computer (steps 3 and 4 above for Windows or Linux) and send the link
   to your phone.
3. On the phone, stay on **View**. Paste the link, or tap the link if it opens
   in MightyMonitor.
4. Press **Connect**. The computer's screen appears on your phone.
5. To stop, press **Disconnect**.

To show your phone's screen instead, open the **Share** tab and press
**Share this screen**. Android asks you to confirm. Then press **Copy
connection link**, send it to the computer and paste it there under **View a
screen**. Sharing control (using the other device's mouse, keyboard or touch)
is off unless you switch on **Allow remote control**.

## Good to know

- A link works only while sharing is running. If you press **Stop sharing**
  or close the app, start sharing again to get a new link.
- Anyone who has the link can watch while sharing is on. Send it only to the
  person you want to share with.
- The connection is private and stays inside your network.
- Wi-Fi works. A cable also works, as long as both devices are on the same
  network.
- Using a device as an extra screen is still being developed and only works on
  some Linux desktops today.

## More detail for technical readers

- [Private links and security](SECURE_CONNECTIONS.md)
- [Installers](INSTALLERS.md), [Windows](WINDOWS.md), [Linux](LINUX.md)
- [Desktop viewer](VIEWER.md), [Android app](../android/README.md)
- [Remote control](REMOTE_CONTROL.md), [extended displays](EXTENDED_DISPLAYS.md)
