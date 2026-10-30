# Installer packages

## Debian / Ubuntu alpha package

Build native optimized binaries, then generate an installable .deb:

```sh
cargo build --release --locked -p trinix-host -p trinix-viewer
python3 scripts/package-linux.py --version '0.0.1~alpha.1'
```

The script requires Python 3, dpkg, dpkg-deb, and dpkg-shlibdeps (dpkg-dev).
It validates the ELF architecture, derives linked-library ABI requirements using
dpkg-shlibdeps, and declares dynamically loaded GUI/codec plugins explicitly.
Packaging never installs software or executes application binaries.

The package contains the host and viewer together in /usr/lib/trinix,
a trinix command, an application-menu entry, the project icon, and the
project license/NOTICE. All package ownership is normalized to root:root without
requiring root during creation. It contains no service, boot autostart, privileged
input device setup, or network listener enabled by installation.

Install a package built for your distribution/architecture using its graphical
package installer, or:

```sh
sudo apt install ./trinix_0.0.1~alpha.1_amd64.deb
```

Then open Trinix from the application menu. apt resolves the declared
GStreamer/GUI runtime dependencies. Remove it using your package manager or
`sudo apt remove trinix`.

The Build Linux installer workflow produces an artifact built on Ubuntu 24.04.
A local build records ABI requirements of the machine where it was built; it is
not automatically compatible with older distributions. Other package formats
and clean-machine installation acceptance remain pending.

## What this package is

An alpha development installer for the Linux X11/Wayland sharing and viewing app.
It is not a declaration that the complete cross-platform product is finished.
The GUI uses private viewing links over pinned TLS; see
[connection security](SECURE_CONNECTIONS.md). Remote input, independent extended
displays, and complete multi-device acceptance remain unfinished.

## Releases and downloads

The packaging workflow uploads build artifacts, not a public stable release.
After device testing, maintainers can attach the .deb to a GitHub release. The
static project website discovers installable assets from the latest stable
release; an unpublished/prerelease CI artifact does not appear as a stable download.
No source archive is substituted for an installer.

## Windows installer development milestone

The `Build Windows installer` workflow creates a per-user x64 Setup executable
using the runner's Inno Setup 6 compiler. It installs the host, viewer, launcher,
Start menu entry, optional desktop shortcut, icon and an uninstaller. The first
installation downloads the pinned official GStreamer 1.28.8 package over HTTPS,
verifies its SHA-256, and installs it under the current user's local app folder.
An internet connection and additional disk space are required. No runtime DLLs
are redistributed inside our Setup executable.

Use the Start menu shortcut: its Rust launcher configures the dedicated runtime
for the viewer and its owned host without editing the system PATH. Setup enables
no service, firewall exception, driver or network listener. The runtime is a
separately installed prerequisite and remains after app removal; its own Windows
uninstall entry can remove it when no longer needed.

The workflow exercises silent install, installed decoder initialization with the
build SDK removed from PATH, and app uninstall. An installer artifact is not a
signed stable release or proof of physical Windows 11 screen capture. Clean-device
installation, firewall approval and real Wi-Fi use still require acceptance.

## Pre-release pipeline

The `Release pre-release` workflow (`.github/workflows/release-prerelease.yml`)
calls the Linux, Windows, and Android installer workflows in parallel, collects
all three artifacts, adds a `SHA256SUMS` file, and uploads the bundle as a
workflow artifact. With the `publish` input set to `true`, it also creates a
GitHub pre-release tagged `v<version>`.

All pre-release artifacts are alpha quality:
- Linux DEB and Windows EXE are **unsigned**.
- Android APK is **debug-signed** (sideloading only).
- No auto-update is present.

See [docs/RELEASING.md](RELEASING.md) for the full publish procedure, artifact
naming, verification steps, and signing status.


## Upgrading from MightyMonitor builds

The Windows installer keeps the same application id, installs into
`%LOCALAPPDATA%\Programs\Trinix`, and removes the previous
`%LOCALAPPDATA%\Programs\MightyMonitor` folder, its Start menu and desktop
shortcuts, and the old `%LOCALAPPDATA%\MightyMonitor` video runtime, which is
downloaded again under `%LOCALAPPDATA%\Trinix`. The `trinix` Debian package
declares `Conflicts`, `Replaces` and `Provides: mightymonitor`, so installing it
replaces an existing `mightymonitor` package.
