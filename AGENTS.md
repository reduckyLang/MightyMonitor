# Working on Trinix

Read `docs/PROJECT_HANDOVER.md` before changing this project. It contains the
product requirements, verified status, missing features, architecture, acceptance
plan and PR workflow. `docs/CROSS_DEVICE.md` defines the intended final behavior.

- Preserve original Rust/protocol implementation and dependency provenance in NOTICE.
- Distinguish implemented, automatically tested, physically tested and released.
  Never infer physical acceptance or a completion percentage from successful builds.
- Share is view-only by default. Preserve explicit control consent, pinned TLS,
  bounded queues and release of held input. Never replay input after reconnect.
- Keep the interface simple, light and neutral/green. No blue/dark default theme,
  unnecessary technical copy or proprietary branding. Website is plain static
  HTML/CSS/JS in `website/dist`, usable from disk and GitHub Pages. No Sites hosting.
- Update the handover guide, current status and website copy at each milestone.
  Keep historical log entries clearly historical. Use inline SVG for website icons.
- Build each milestone on current upstream main. Before opening its PR, verify
  prerequisite PRs merged and account for squash merges; avoid overlapping PR chains.
  Do not merge upstream PRs unless the user explicitly asks you to merge them.
- Run checks appropriate to the actual changes; do not run input-injection tests
  on the user's active desktop. Use an isolated X server. Portal consent stays
  under the user's control. Never enable Android accessibility or IME permissions
  programmatically in the shipped app.
- Keep secrets, private viewing links, device identifiers, screenshots of personal
  content, local test artifacts and credentials out of commits, logs and PRs.
