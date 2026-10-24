# Releasing MightyMonitor

This document describes the pre-release pipeline: how to build and verify
installer artifacts and how to publish a GitHub pre-release.

## Overview

The `.github/workflows/release-prerelease.yml` workflow dispatches three
installer builds (Linux DEB, Windows Setup, Android APK), collects them into
a single artifact bundle with a `SHA256SUMS` file, and optionally creates a
GitHub pre-release. All artifacts are pre-release / alpha quality.

## Running the pipeline (publish=false)

This is the default and does not publish anything publicly. Use it to verify
that the build succeeds and artifacts are correct before deciding to publish.

1. Go to **Actions → Release pre-release** in the repository.
2. Click **Run workflow**.
3. Enter a `version` string following the pattern `MAJOR.MINOR.PATCH-alpha.N`,
   e.g. `0.1.0-alpha.1`.
4. Leave **publish** unchecked (default: false).
5. Click **Run workflow**.

The workflow calls `linux-installer.yml`, `windows-installer.yml`, and
`android-viewer.yml` in parallel, then the `collect` job downloads all
produced artifacts, renames the Android APK to include the version string
(e.g. `mightymonitor-0.1.0-alpha.1-debug-signed.apk`), and computes
`SHA256SUMS`. The resulting bundle is uploaded as a workflow artifact named
`mightymonitor-<version>-prerelease`.

### Verifying artifacts

Download the workflow artifact bundle and verify checksums:

```sh
# Unzip the downloaded bundle, then:
sha256sum -c SHA256SUMS
```

All entries must read `OK`. Inspect the DEB, EXE, and APK before proceeding
to a publish run.

## Publishing a pre-release (publish=true)

Only proceed after verifying the artifact bundle (see above).

1. Run the workflow again with the same `version`.
2. Check the **publish** checkbox (set to true).
3. Click **Run workflow**.

The `publish` job creates a GitHub pre-release tagged `v<version>` pointing
at the dispatched commit. It uses only `GITHUB_TOKEN` (`contents: write` is
scoped to that job; no other permissions are elevated and no repository
secrets are used).

The resulting release is marked as a **pre-release**. The release notes state:

- The Linux DEB is **unsigned**.
- The Windows EXE is **unsigned**.
- The Android APK is **debug-signed** (sideload only; not from the Play Store).
- There is **no auto-update** mechanism.

## Signing status

| Platform | Signing |
|----------|---------|
| Linux DEB | Unsigned; no GPG/APT repository signing |
| Windows EXE | Unsigned; Windows may show SmartScreen/UAC warnings |
| Android APK | Debug-signed; requires "Unknown sources" / sideloading |

Production signing (Windows Authenticode, Google Play keys, GPG-signed APT
repository) is not part of the current alpha pipeline. Do not claim the
artifacts are production-ready.

## What publish=true does NOT do

- It does not push code or commits.
- It does not trigger any auto-update delivery.
- It does not modify the website. Update `website/dist` separately following
  `website/README.md` if a stable download link is needed on the project page.

## Artifact naming

| Artifact | Pattern |
|----------|---------|
| Linux DEB | `mightymonitor_<version>_amd64.deb` |
| Windows Setup | `MightyMonitor-<version>-windows-x64-setup.exe` |
| Android APK | `mightymonitor-<version>-debug-signed.apk` |
| Checksums | `SHA256SUMS` |
