#!/usr/bin/env python3
"""Build a real Debian installer from native release binaries; never installs it."""
import argparse
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent

def run(*args, cwd=None):
    return subprocess.run(args, cwd=cwd, check=True, text=True, capture_output=True).stdout.strip()

def architecture(binary):
    with binary.open('rb') as handle:
        header = handle.read(20)
    if len(header) < 20 or header[:4] != b'\x7fELF' or header[5] != 1:
        raise ValueError(f'Not a supported little-endian ELF executable: {binary}')
    machine = struct.unpack_from('<H', header, 18)[0]
    names = {62: 'amd64', 183: 'arm64', 3: 'i386', 40: 'armhf'}
    if machine not in names:
        raise ValueError(f'Unsupported ELF architecture: {machine}')
    return names[machine]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--version', default='0.0.1~alpha.1')
    parser.add_argument('--binaries-dir', type=Path, default=ROOT / 'target/release')
    parser.add_argument('--output-dir', type=Path, default=ROOT / 'output/installers')
    args = parser.parse_args()
    if not re.fullmatch(r'[0-9][A-Za-z0-9.+:~\-]*', args.version):
        parser.error('Use a valid Debian package version starting with a digit')
    for tool in ('dpkg', 'dpkg-deb', 'dpkg-shlibdeps'):
        if not shutil.which(tool):
            parser.error(f'{tool} is required (install dpkg-dev)')
    binaries = [(args.binaries_dir / name).resolve() for name in ('trinix-viewer', 'trinix-host')]
    for binary in binaries:
        if not binary.is_file() or not os.access(binary, os.X_OK):
            parser.error(f'Missing executable {binary}; first run cargo build --release --workspace')
    arch = architecture(binaries[0])
    if architecture(binaries[1]) != arch or run('dpkg', '--print-architecture') != arch:
        parser.error('Build/package natively; binary architectures must match this system')
    args.output_dir.mkdir(parents=True, exist_ok=True)
    output = (args.output_dir / f'trinix_{args.version}_{arch}.deb').resolve()
    with tempfile.TemporaryDirectory(prefix='trinix-package-') as temp:
        work = Path(temp)
        package = work / 'package'
        app = package / 'usr/lib/trinix'
        app.mkdir(parents=True)
        for binary in binaries:
            destination = app / binary.name
            shutil.copyfile(binary, destination)
            destination.chmod(0o755)
        launcher = package / 'usr/bin/trinix'
        launcher.parent.mkdir(parents=True)
        launcher.symlink_to('../lib/trinix/trinix-viewer')
        for source, destination in [
            (ROOT / 'packaging/linux/trinix.desktop', 'usr/share/applications/trinix.desktop'),
            (ROOT / 'assets/trinix.png', 'usr/share/pixmaps/trinix.png'),
            (ROOT / 'LICENSE', 'usr/share/doc/trinix/LICENSE'),
            (ROOT / 'NOTICE', 'usr/share/doc/trinix/NOTICE'),
        ]:
            target = package / destination
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            target.chmod(0o644)
        # Resolve actual ELF ABI requirements rather than guessing libc versions.
        debian = work / 'debian'
        debian.mkdir()
        (debian / 'control').write_text('Source: trinix\nSection: net\nPriority: optional\nMaintainer: Trinix contributors <85924848+akasumitlamba@users.noreply.github.com>\n\nPackage: trinix\nArchitecture: any\nDescription: Local screen-sharing prototype\n')
        resolved = run('dpkg-shlibdeps', '-O', *(f'-e{path}' for path in binaries), cwd=work)
        dependency_line = next((line for line in resolved.splitlines() if line.startswith('shlibs:Depends=')), None)
        if not dependency_line:
            raise RuntimeError('Could not determine binary dependencies')
        dependencies = dependency_line.partition('=')[2]
        # GUI/codec plugins are loaded dynamically and do not appear in DT_NEEDED.
        extras = [
            'libgstreamer1.0-0 (>= 1.20)', 'gstreamer1.0-plugins-base',
            'gstreamer1.0-plugins-good', 'gstreamer1.0-plugins-ugly', 'gstreamer1.0-libav',
            'libxkbcommon0', 'libwayland-client0', 'libx11-6', 'libegl1', 'libgl1',
            'xdg-desktop-portal',
        ]
        control = package / 'DEBIAN'
        control.mkdir()
        installed_size = sum(p.stat().st_size for p in package.rglob('*') if p.is_file() and not p.is_symlink()) // 1024
        (control / 'control').write_text(
            f'Package: trinix\nVersion: {args.version}\nArchitecture: {arch}\n'
            'Section: net\nPriority: optional\n'
            'Maintainer: Trinix contributors <85924848+akasumitlamba@users.noreply.github.com>\n'
            f'Installed-Size: {installed_size}\nDepends: {dependencies}, '+', '.join(extras)+'\n'
            'Homepage: https://github.com/reduckyLang/Trinix\n'
            'Description: Local screen-sharing development prototype\n'
            ' Share and view a Linux desktop over a local network, with private\n'
            ' viewing links, pinned TLS, consent-based remote control, an Android\n'
            ' viewer and experimental extended-display sharing. This alpha package\n'
            ' is unsigned and intended for testing.\n'
        )
        (control / 'control').chmod(0o644)
        for directory in [package, *(p for p in package.rglob('*') if p.is_dir())]:
            directory.chmod(0o755)
        run('dpkg-deb', '--root-owner-group', '--build', str(package), str(output))
    print(output)

if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        if isinstance(error, subprocess.CalledProcessError):
            print(error.stderr, end='')
        raise SystemExit(str(error)) from error
