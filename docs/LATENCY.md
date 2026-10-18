# Latency measurement

## Budgets and targets

The milestone target for 1080p30 over Wi-Fi 5/6 LAN is glass-to-glass
p50 <=100 ms and p95 <=150 ms. The loopback capture/encode/network/decode
pipeline (no display) targets p50 <=50 ms. Network plus framing targets
p99 <=2 ms on loopback for 64 KiB frames. CI only enforces a generous 50 ms
p99 smoke bound; these performance targets are not CI assertions.

An aspirational low-latency LAN budget is 60 ms, allocated below. These are
engineering targets, not measurements of implemented capture or codec stages.

| Stage | Budget (ms) |
| --- | ---: |
| Capture / frame acquisition | 10 |
| Encode | 10 |
| Send / serialization / queueing | 2 |
| LAN network | 8 |
| Decode | 10 |
| Present / display wait | 20 |
| Total | 60 |

## Timestamps and clock offsets

`capture_ts` means `VideoFrame.timestamp_us`: timestamp the capture instant,
before encoding or waiting in a send queue. Use `MonotonicClock::now_us()`,
unsigned microseconds since an origin `Instant`. Clones share an origin;
independently constructed clocks and different hosts have independent origins.
Values are neither wall time nor comparable between hosts without an offset.
`TimeSource` lets consumers inject a deterministic clock or closure for tests.

For four timestamps (t0 local send, t1 peer receive, t2 peer send, t3 local
receive), `OffsetEstimator` computes:

```text
rtt_us = (t3 - t0) - (t2 - t1)
offset_us = ((t1 - t0) + (t2 - t3)) / 2   # peer minus local
```

Signed intermediate arithmetic avoids unsigned underflow. Invalid per-host time
ordering, negative RTT, and unrepresentable results are rejected without changing
the window. The default window retains 32 valid samples and selects the minimum
RTT, evicting oldest samples as new ones arrive. This reduces queueing jitter;
it cannot remove persistent path asymmetry or clock drift. The synthetic test
uses independent deterministic asymmetric delays and checks <1 ms error after
1000 samples. That tolerance is a property of the test distribution, not a
synchronization guarantee on arbitrary networks.

A future receiver in peer-clock coordinates would compute frame age as
`receive_ts - (capture_ts + offset_us)` with signed arithmetic. Keep estimates
fresh and handle uncertainty and negative ages explicitly.

**Current Pong limitation:** protocol version 2 `Message::Pong` only echoes
`t_send_us` and the nonce. It has no peer receive/send timestamps. A follow-up
protocol change must add `t_peer_recv_us` and `t_peer_send_us` to Pong and bump
`PROTOCOL_VERSION` before four-timestamp offset estimation can work on the wire.
This task changes neither the wire format nor protocol version. For now the
estimator accepts samples supplied by callers; existing Ping/Pong supports
RTT-only measurement (`rtt = t3 - t0`, including peer processing).

## Harness and measurement method

```sh
cargo run -p bench --release -- --frames 1000 --size 65536
cargo run -p bench --release -- --frames 1000 --size 1048576
cargo run -p bench --release -- --frames 1000 --size 65536 --fps 30
cargo run -p bench --release -- --frames 1000 --rtt
cargo run -p bench --release -- --frames 100 --size 65536 --max-p99-ms 50
```

The binary binds an ephemeral IPv4 TCP loopback port and uses `TcpTransport`
with TCP_NODELAY and typed messages. Sender and receiver run concurrently on a
single Tokio thread with one shared clock origin. Video data is synthetic;
the capture timestamp is taken after allocation, immediately before typed
serialization. Receive time is recorded after framing and typed decoding.
Samples include serialization, copies, sender/socket queueing, scheduling,
TCP delivery, framing, and deserialization. No capture, codec, or display is
exercised. All frames are measured, with no warm-up exclusion. Sequence and size
are checked. Default sending is unpaced and can build a socket backlog;
`--fps` adds a wait between completed sends (actual rate may be lower than the
requested rate). RTT mode sends sequential Ping/Pong exchanges and ignores size.

One stdout line contains stable whitespace-separated `key=value` fields:
`mode`, `frames`, `size_bytes`, `elapsed_s`, `throughput_mib_s`, `p50_ms`,
`p95_ms`, `p99_ms`, `max_ms`. Throughput counts video data bytes only (MiB/s),
excludes metadata, and includes pacing time. RTT mode reports size and data
throughput as zero. Percentiles use nearest rank on sorted integer microseconds;
printed milliseconds have three decimals. `--max-p99-ms` exits nonzero when the
unrounded p99 exceeds the limit; invalid arguments and transport errors also
exit nonzero. `--help` lists options.

## Initial results (2026-10-18)

Machine: **dev box, loopback, release build**, Linux; shared development
container, no CPU isolation. These are single local runs, loopback only, not
Wi-Fi or glass-to-glass results. Timing excludes connection establishment.

| Mode | Frames | Data bytes/frame | MiB/s | p50 ms | p95 ms | p99 ms | Max ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| One-way, unpaced | 1000 | 65536 | 515.886 | 3.910 | 5.921 | 7.493 | 10.051 |
| One-way, unpaced | 1000 | 1048576 | 334.838 | 9.204 | 16.098 | 18.760 | 25.941 |
| CI smoke, unpaced | 100 | 65536 | 380.702 | 7.003 | 7.443 | 7.460 | 8.068 |
| RTT | 100 | 0 | 0.000 | 0.006 | 0.007 | 0.009 | 0.024 |
| One-way, requested 60 fps | 10 | 65536 | 3.886 | 0.188 | 0.264 | 0.264 | 0.264 |

The unpaced 64 KiB p99 misses the 2 ms target. Saturation queueing and the
single-thread scheduler are included here; this does not isolate network cost.
Next measurements should compare sustained paced traffic, queue depth, and
serialization/copy costs before claiming the target met. The short paced run
is a functionality check, not sufficient evidence of sustained performance.
Capture/codec/display and real Wi-Fi measurements remain for later M2 tasks.

## Linux Wi-Fi impairment helper

```sh
sudo scripts/wifi-sim.sh setup wlan0
# Run traffic over that interface between actual host/viewer machines.
sudo scripts/wifi-sim.sh teardown wlan0
```

Requires Linux, root, and iproute2 `tc`. Setup adds an egress root netem qdisc
with uniform delay from 5 to 30 ms (17.5 ms +/-12.5 ms), and 1% random loss.
It refuses to replace an existing root qdisc. Teardown deletes the root qdisc;
use a dedicated test interface and always tear down after experiments. It affects
all egress traffic on that interface, including administration connections.
For both directions, configure each endpoint's outgoing interface. TCP loss
causes retransmission delays rather than application frame loss. Applying it
to wlan0 does not affect this harness's 127.0.0.1 traffic; the current harness
is loopback only. The helper was not run for the results above.
