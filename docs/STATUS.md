# Status

2026-10-18: repository bootstrapped (workspace, `core` skeleton, CI, docs, roadmap, issues). No milestone complete yet.

2026-10-18: M1-2 stream framing + FrameDecoder + docs/PROTOCOL.md implemented on branch feat/m1-2-framing.

2026-10-18: M1-1 workspace dependencies, stable toolchain, core module split, and CI concurrency configured.

2026-10-18: M1-4 frame-level async Transport + TCP implementation + loopback integration tests implemented on branch feat/m1-4-transport. Message-level API arrives with M1-3 integration.

2026-10-18: M1-2 review nits: isolated FrameError trait implementations, explained decoder validation, clarified poisoned-frame delivery, and added the 16 MiB payload boundary test.

2026-10-18: M1-5 frame-level reconnect interface, capped exponential backoff with injectable jitter, provisional Resume payload, bounded lifecycle events, and deterministic/unit + loopback recovery tests implemented. Typed Message::Resume integration awaits M1-3; mDNS remains M8.

2026-10-18: PR #16 review: reject oversized sends before connection work and preserve connections on local frame errors; only send I/O errors and write timeouts recover. Cancelled sends drop their transport. Backoff resets on the first received frame, delays have a 1 ms floor, and retry-budget docs count scheduled attempts (including cancelled sleeps). Added regression tests.
2026-10-18: M1-4 review nits: increased the loopback stream timeout, renamed the ping-pong roundtrip test, and documented send allocation, throughput, and sticky receive errors.

2026-10-18: Milestone 1 progress snapshot (main at 35ea74f). Merged: #12 (M1-2 framing), #13 and #15 (M1-4 TCP transport/loopback), #14 (M1-1 workspace), #16 (M1-5 reconnect). Open: #17 (M1-6 bitrate controller, under review). Not done: M1-3 (typed messages; PR #18 was closed unmerged) and M1-7 (CI hardening/mocks). Reviewer follow-ups are tracked in the "M1 follow-ups from review" issue and listed below. Milestone 2 is planned in issue #3; follow-up (3) (split reader/writer) is a prerequisite for streaming.

## M1 follow-ups
- [ ] (1) Send-only `ReconnectingSession` never resets its retry budget when `max_attempts` is `Some`.
- [ ] (2) Treat `TransportError::Closed` from send as recoverable, or document that it is not.
- [ ] (3) `&mut self` means recv blocks send during backoff: needs a split reader/writer or channel design before the streaming milestones (M2 prerequisite).
- [ ] (4) `JitterSource` should be `Send`.
- [ ] (5) PROTOCOL.md: state that Resume is unauthenticated (bind to pairing/auth later), clarify `last_seq == 0`, reword "Kind/flags".
- [ ] (6) M1-3 should reserve kinds `0xfe` and `0xff`, and bump `PROTOCOL_VERSION` when the Resume kind is finalized.
- [ ] (7) Add a server-side Resume handler.
- [ ] (8) `lib.rs` hygiene: group `pub mod` lines then re-exports; add docs to `error.rs` and `traits.rs`.

2026-10-18: M2-0 reader/writer split implemented with a bounded command channel, prompt NotConnected during backoff, deadline-bounded writes, shared lifecycle state/events, and Send JitterSource (covers #19 items 3 and 4). Added deterministic backoff/concurrent-task, replacement-transport, cancelled-write, and static Send checks; existing unsplit reconnect and loopback behavior retained.
2026-10-18: M1-3 typed Message enum and input events integrated with bounded bincode encoding and validated frame kinds; kinds 0xfe/0xff reserved for protocol extension/escape; PROTOCOL_VERSION=2; provisional Resume replaced by Message::Resume kind 9 in reconnect sessions.

2026-10-18: PR #22 review: merged typed Message::Resume support into split sessions, made cancelled inline reader writes disconnect with Closed, and distinguished queue deadlines from write deadlines with regression tests and cancellation documentation.

2026-10-18: M1-6 ABR controller implemented on branch feat/m1-6-abr.

2026-10-18: PR #17 review: added absolute RTT slack to tolerate LAN jitter and unified ABR feedback with message::NetStats.
