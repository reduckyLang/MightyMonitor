# Status

2026-10-18: repository bootstrapped (workspace, `core` skeleton, CI, docs, roadmap, issues). No milestone complete yet.

2026-10-18: M1-2 stream framing + FrameDecoder + docs/PROTOCOL.md implemented on branch feat/m1-2-framing.

2026-10-18: M1-1 workspace dependencies, stable toolchain, core module split, and CI concurrency configured.

2026-10-18: M1-4 frame-level async Transport + TCP implementation + loopback integration tests implemented on branch feat/m1-4-transport. Message-level API arrives with M1-3 integration.

2026-10-18: M1-2 review nits: isolated FrameError trait implementations, explained decoder validation, clarified poisoned-frame delivery, and added the 16 MiB payload boundary test.

2026-10-18: M1-5 frame-level reconnect interface, capped exponential backoff with injectable jitter, provisional Resume payload, bounded lifecycle events, and deterministic/unit + loopback recovery tests implemented. Typed Message::Resume integration awaits M1-3; mDNS remains M8.

2026-10-18: PR #16 review: reject oversized sends before connection work and preserve connections on local frame errors; only send I/O errors and write timeouts recover. Cancelled sends drop their transport. Backoff resets on the first received frame, delays have a 1 ms floor, and retry-budget docs count scheduled attempts (including cancelled sleeps). Added regression tests.
2026-10-18: M1-4 review nits: increased the loopback stream timeout, renamed the ping-pong roundtrip test, and documented send allocation, throughput, and sticky receive errors.

2026-10-18: M2-2 step A implemented: Linux-only host-linux crate, pure-Rust X11 GetImage capture with packed BGRA, monotonic frame metadata, optional pacing, fake-source unit tests, and Xvfb smoke coverage in CI. PipeWire portal capture remains the next PR.
