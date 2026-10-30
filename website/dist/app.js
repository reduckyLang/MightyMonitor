"use strict";
const PROJECT = "reduckyLang/Trinix";
const RELEASES_URL = `https://github.com/${PROJECT}/releases`;
const descriptions = {
  mirror: [
    "Same screen.\nNew perspective.",
    "Mirror a screen across your devices. Alpha software.",
  ],
  control: [
    "Your touch.\nTheir workspace.",
    "Click, type and touch from another device. Control requires permission.",
  ],
  extend: [
    "A little\nmore room.",
    "Real extra desktop space. Linux Wayland alpha; Windows integration in progress.",
  ],
};
for (const button of document.querySelectorAll(".mode-switch button")) {
  button.addEventListener("click", () => {
    const mode = button.dataset.mode;
    if (!descriptions[mode]) return;
    for (const peer of document.querySelectorAll(".mode-switch button"))
      peer.setAttribute("aria-pressed", String(peer === button));
    document.getElementById("device-stage").dataset.mode = mode;
    if (typeof window !== "undefined")
      window.dispatchEvent(
        new CustomEvent("mm-preview-mode", { detail: mode }),
      );
    document.getElementById("concept-detail").textContent =
      descriptions[mode][1];
  });
}
// Accept installable packages only. Portable executables and source archives are excluded.
function installerPlatform(name) {
  if (/\.msi$/i.test(name) || /(?:setup|installer)[\w. -]*\.exe$/i.test(name))
    return "windows";
  if (/\.(deb|rpm)$/i.test(name)) return "linux";
  if (/\.apk$/i.test(name)) return "android";
  return null;
}
function trustedAssetUrl(value) {
  try {
    const url = new URL(value);
    return (
      url.protocol === "https:" &&
      url.hostname === "github.com" &&
      !url.username &&
      !url.password &&
      url.pathname.startsWith(`/${PROJECT}/releases/download/`)
    );
  } catch {
    return false;
  }
}
function resetDownloads(label) {
  for (const card of document.querySelectorAll("[data-platform]")) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "button unavailable";
    button.disabled = true;
    button.textContent = label;
    card.querySelector(".asset-links").replaceChildren(button);
  }
}
async function loadRelease() {
  const status = document.getElementById("release-status");
  const refresh = document.getElementById("refresh-releases");
  refresh.disabled = true;
  document.getElementById("download-availability").textContent =
    "Alpha builds. Stable installers appear below when published.";
  resetDownloads("Checking availability…");
  status.textContent = "Checking for published installers…";
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 8000);
  try {
    const response = await fetch(
      `https://api.github.com/repos/${PROJECT}/releases/latest`,
      {
        headers: { Accept: "application/vnd.github+json" },
        signal: controller.signal,
      },
    );
    if (response.status === 404) {
      resetDownloads("Not released yet");
      status.textContent =
        "No published release yet. Installer downloads will appear here when available.";
      return;
    }
    if (!response.ok) throw new Error("Release lookup failed");
    const release = await response.json();
    if (release.draft || release.prerelease || !Array.isArray(release.assets))
      throw new Error("No stable release");
    resetDownloads("No installer in this release");
    const grouped = { windows: [], linux: [], android: [] };
    for (const asset of release.assets) {
      if (
        !asset ||
        typeof asset.name !== "string" ||
        !trustedAssetUrl(asset.browser_download_url)
      )
        continue;
      const platform = installerPlatform(asset.name);
      if (platform) grouped[platform].push(asset);
    }
    let total = 0;
    for (const [platform, assets] of Object.entries(grouped)) {
      if (!assets.length) continue;
      total += assets.length;
      const links = document.querySelector(
        `[data-platform="${platform}"] .asset-links`,
      );
      links.replaceChildren();
      for (const asset of assets) {
        const link = document.createElement("a");
        link.href = asset.browser_download_url;
        link.className = "button";
        link.textContent = `Download ${asset.name}`;
        link.setAttribute("aria-label", `Download ${asset.name} installer`);
        links.append(link);
      }
    }
    if (total)
      document.getElementById("download-availability").textContent =
        "Release installers are available in the download section.";
    status.textContent = total
      ? `Latest release: ${release.tag_name || "published release"} · ${total} installer package${total === 1 ? "" : "s"} available.`
      : `Latest release: ${release.tag_name || "published release"} · No installer packages attached yet.`;
  } catch {
    resetDownloads("Check GitHub Releases");
    status.replaceChildren(
      document.createTextNode("Could not check downloads. "),
    );
    const link = document.createElement("a");
    link.href = RELEASES_URL;
    link.textContent = "Check GitHub Releases";
    status.append(link);
  } finally {
    clearTimeout(timeout);
    refresh.disabled = false;
  }
}
document
  .getElementById("refresh-releases")
  .addEventListener("click", loadRelease);
loadRelease();
