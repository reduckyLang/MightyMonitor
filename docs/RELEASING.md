# Releasing Trinix

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
   e.g. `0.1.0-alpha.2`. Use a version whose tag `v<version>` does not exist
   yet; `0.1.0-alpha.1` is already released.
4. Leave **publish** unchecked (default: false).
5. Click **Run workflow**.

The workflow calls `linux-installer.yml`, `windows-installer.yml`, and
`android-viewer.yml` in parallel, then the `collect` job downloads all
produced artifacts, renames the Android APK to include the version string
(e.g. `trinix-0.1.0-alpha.2-debug-signed.apk`), and computes
`SHA256SUMS`. The resulting bundle is uploaded as a workflow artifact named
`trinix-<version>-prerelease`.

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

Releases can only be built and published from `main`: dispatch the workflow
with **Use workflow from: main**. When dispatched from any other branch or tag,
the `preflight` and `publish` jobs are skipped by an `if: github.ref ==
'refs/heads/main'` condition, so the dependent build jobs are skipped too and
nothing is built or published.

A version whose tag already exists cannot be republished. The workflow's first
job fails with a clear message if the tag `v<version>` or a release for it
(including a draft) exists, for both verification and publish runs. Only an
HTTP 404 from the tag lookup is treated as "unused"; any other API error fails
the job. The `publish` job repeats the check immediately before creating the
release. To ship a fix, choose the next version (for example
`0.1.0-alpha.3`). Existing tags and releases are never moved or overwritten.

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
- The builds have not yet been tested on physical devices or across real Wi-Fi
  networks.
- Windows Setup downloads the official GStreamer runtime (including the x264 and
  libav plugins) at install time; the runtime is not part of the installer.

## Version caveats

- **Debian version sorting.** In a `.deb`, the first `-` splits the upstream
  version from the Debian revision, so `0.1.0-alpha.2` is upstream `0.1.0` with
  revision `alpha.2`. That version sorts **after** `0.1.0` (and after
  `0.1.0-alpha.1`), whereas `0.1.0~alpha.2` sorts **before** `0.1.0`. A package
  built from the workflow version `0.1.0-alpha.N` therefore sorts above a later
  `0.1.0` final package; a final `0.1.0` would not upgrade it. A tilde form
  (`0.1.0~alpha.N`) sorts below the final release. The Linux workflow's own
  default version uses the tilde form. Choose the DEB version scheme before
  publishing a final release. `dpkg --compare-versions` shows the ordering.
- **Android versionName.** The APK's `versionName` and `versionCode` come from
  the Gradle build (`android/app/build.gradle`), not from the workflow `version`
  input. The input only names the APK file and the release; it does not change
  the version reported by Android or its upgrade ordering.

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
| Linux DEB | `trinix_<version>_amd64.deb` |
| Windows Setup | `Trinix-<version>-windows-x64-setup.exe` |
| Android APK | `trinix-<version>-debug-signed.apk` |
| Checksums | `SHA256SUMS` |
