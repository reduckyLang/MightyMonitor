# Contributing

- `main` is protected: never push to it directly. All work goes through branches and pull requests.
- Keep PRs small and focused; CI (fmt, clippy -D warnings, test --all-features, doc, build on Linux and Windows) must pass before merge.
- Use conventional commits (`feat:`, `fix:`, `docs:`, `chore:`, `test:`, `ci:`).
- Clean room: do not copy code, assets, branding or protocol from spacedesk or other proprietary apps. Open-source code may be studied only if its license allows; record attribution in NOTICE.
- Never commit secrets or tokens.
- Wireless first: every feature must work over Wi-Fi/LAN with no cable.

Branch from an up-to-date `main` for each task (for example,
`feat/m1-7-ci-wrapup`). Implement the focused change, update `docs/STATUS.md`,
and run the checks below. Push the task branch and open a pull request targeting
`main` with a description of the change and validation results. Address review
feedback and wait for Linux and Windows CI to pass before merging through the PR.

Run these checks from the workspace root before opening or updating a PR:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo build --workspace
```

In PowerShell, set `$env:RUSTDOCFLAGS = "-D warnings"` before running the doc
command. The optional core `mock` feature provides synthetic capture, a reversible
codec, and a recording input injector; `--all-features` includes its TCP loopback
pipeline test. The default build does not enable these mocks.
