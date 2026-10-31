# Trinix static website

Plain HTML, CSS, JavaScript, images and bundled fonts. No build step, framework, application server, account or hosting-service dependency.

## Open on your computer

Double-click `dist/index.html`, or open it from your browser's File menu. All styles, scripts, images and fonts use relative paths, so this works directly from disk. Internet is only needed to look up releases or follow project links. Without connectivity, the page remains readable and shows a link to check GitHub Releases later.

For an optional local server, run from the repository root:

```sh
python3 -m http.server 8765 --directory website/dist
```

Then open http://localhost:8765.

## GitHub Pages

Both GitHub Pages publishing methods are supported:

### Deploy from a branch: main, / (root)

This works with the root-directory setting. The repository's root `index.html`
opens `website/dist/` automatically. `.nojekyll` keeps Pages from treating the
repository as a Jekyll documentation site. A plain link also works without
JavaScript; the redirect preserves query strings and section anchors when JS is
available. Keep the website files in `website/dist`; do not copy a second site
into the root or remove the root entry point.

### GitHub Actions

The existing `.github/workflows/website.yml` uploads `website/dist` directly.
Choose **Settings → Pages → Source → GitHub Actions** to use this method instead.
Later changes to `website/dist` on main deploy automatically, or run **Publish
project website** manually. There is no package installation or build command.

All site assets use relative paths and support GitHub Pages repository subpaths.
The workflow uses the repository where it runs; download discovery intentionally
points to upstream `reduckyLang/Trinix` releases. Change `PROJECT` in
`dist/app.js` only when intentionally distributing installers from another repo.

## Real installer links

The page requests the latest stable release from the public GitHub API. It only links to installable `.msi`, setup/installer `.exe`, `.deb`, `.rpm`, or `.apk` assets belonging to the configured repository's release-download path. Bare executables, portable archives and source archives are excluded. Offline/rate-limited lookups provide a manual Releases link. Missing releases/packages leave controls disabled.

The website does not create installers. Maintainers must build, validate and attach packages to a published stable release before download links appear. No credentials are embedded.

## Checks and content

```sh
node website/tests/downloads.cjs
```

The automated tests cover missing releases, real installer filtering, untrusted URLs, source-only releases and offline/rate-limit fallback. The development status is an editorial snapshot dated 23 October 2026. Update it at every merged milestone, keeping implementation, emulator/local validation and physical-device acceptance distinct. The interactive device illustration is explicitly a concept, not a screenshot or a claim that all platform/device combinations work. Icons are inline SVG and do not depend on font glyph coverage.

See DESIGN.md for the frontend-design skill's design plan and critique. The icon is original project artwork. Bundled Sora and IBM Plex Sans fonts include their open-font licenses in `dist/assets/fonts/`.


## Motion and artwork

`dist/assets/artwork/` contains generated studio illustrations and prompt notes.
`dist/artwork.js` plays a three-chapter animated scene sequence using local
artwork. Each chapter lasts 6.5 seconds, with a camera move and directional wipe.
Mirror, Control and Extend select chapters; Pause/Play controls the camera and
clock. Offscreen/hidden scenes pause, and reduced motion disables autoplay.
The initial workspace artwork remains visible without JavaScript. These are
concept images, not app screenshots. No WebGL or external asset CDN is required.

The app icon master is `../assets/brand/trinix-master.png`; see that
folder's README for the design, provenance and synchronized export locations.

Additional root Pages regression checks:

```sh
node website/tests/pages-root.cjs
```
