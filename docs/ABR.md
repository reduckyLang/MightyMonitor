# Host adaptive bitrate

Viewer NetStats -> Pipeline::on_stats -> host::abr::Abr -> core AIMD ->
Encoder::set_bitrate_kbps. The policy accepts a caller-supplied monotonic time,
with at most one decision per second. It preserves the minimum RTT baseline over
a rolling 30-second window (configured by `AimdConfig::baseline_window`), so an
isolated low sample eventually ages out. It uses core's 20 ms / 1.5x RTT slack
and 2% loss threshold, and requires two healthy
reports per additive recovery step (at most 5,000 kbit/s per step). Bounds default
to 500–20,000 kbit/s; CLI starting values outside that range extend the bounds.
Congestion reduces by core's 0.7 factor, additionally capped at 90% of measured
payload throughput when nonzero. Throughput alone never causes a reduction:
it can be application limited. A completed video write taking over 100 ms also
signals congestion. Failed encoder updates retain controller state for retry.
Write pressure is latched across rate-limited or invalid reports and encoder
rejections, then consumed by the next successfully applied valid decision.
Invalid loss reports are ignored. A frame write can delay processing feedback;
write timeout remains the upper bound, not a two-second real-network guarantee.

The capture slot holds one latest raw frame. Encoding is demand driven, so no
encoded application backlog accumulates. Socket/kernel queues are outside this
bound. Host write duration and viewer RTT expose queue pressure indirectly;
there is no TCP queue-depth or packet-retransmission telemetry. Viewer payload
throughput uses actual bytes and elapsed time; RTT uses matched Ping/Pong.
TCP packet loss is unavailable, so loss_pct remains zero, not a measured loss
estimate. Sequence gaps also include deliberate host frame drops and must not
be interpreted as packet loss.

Run `cargo test -p mightymonitor-host abr:: -- --nocapture` for deterministic
coverage. The virtual transport uses 1 ms ticks, a 30 fps constant-rate encoder,
one non-preemptible write and one replaceable raw slot, with 20 Mbps capacity,
a drop to 4 Mbps at 3 seconds and recovery at 9 seconds. It checks reduction
within two seconds, recovery to at least 16 Mbps within ten seconds of recovery,
and a one-frame raw queue bound. It compares whole-run p95 frame delivery latency
against a separate 20 Mbps baseline over 30 virtual seconds, including a fixed
20 ms propagation delay. The <2x assertion is for this model and window, not
physical capture/display latency or a bound on every frame or outage percentile.
It omits TCP retransmission, kernel buffering, variable-size IDRs and real codec
rate-control delay. Actual Wi-Fi and desktop measurements remain pending.

FPS/resolution renegotiation is not implemented: core Encoder exposes bitrate
only, and changing dimensions requires VideoConfig and codec/capture coordination.
The latest-frame policy naturally reduces delivered fps under backpressure.

Local deterministic result (2026-10-18): whole-run p95 79 ms baseline and
79 ms with throttling; target 2,514 kbit/s at drop +2 s and 19,000 kbit/s at
recovery +10 s. These are model outputs, not measured LAN performance.
