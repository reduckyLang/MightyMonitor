//! Time-gated host policy around the core AIMD controller.
use std::time::Duration;
use trinix_core::{AimdConfig, AimdController, Encoder, NetStats};

/// One decision per second, two healthy reports before recovery, and a bounded
/// 5 Mbit/s recovery step. Time is supplied by the caller for deterministic tests.
#[derive(Clone, Debug)]
pub struct Abr {
    core: AimdController,
    last: Option<Duration>,
    pending_write_pressure: bool,
    min: u32,
    max: u32,
}
impl Abr {
    pub fn new(start: u32, min: u32, max: u32) -> Result<Self, &'static str> {
        Ok(Self {
            core: AimdController::new(AimdConfig {
                min_kbps: min,
                max_kbps: max,
                start_kbps: start,
                increase_kbps: 5_000,
                stable_samples_required: 2,
                ..Default::default()
            })?,
            last: None,
            pending_write_pressure: false,
            min,
            max,
        })
    }
    pub fn target_kbps(&self) -> u32 {
        self.core.target_kbps()
    }
    /// Failed encoder updates leave the policy unchanged, allowing a retry.
    /// `blocked` means a video write exceeded the 100 ms local queue budget.
    pub fn apply<E: Encoder>(
        &mut self,
        encoder: &mut E,
        stats: &NetStats,
        now: Duration,
        blocked: bool,
    ) -> Result<Option<u32>, trinix_core::CodecError> {
        // Latch before cloning so skipped reports and encoder failures retain
        // the observation until a valid decision is successfully committed.
        self.pending_write_pressure |= blocked;
        let mut next = self.clone();
        let target = next.decide(stats, now);
        if let Some(target) = target {
            encoder.set_bitrate_kbps(target)?;
        }
        *self = next;
        Ok(target)
    }
    fn decide(&mut self, stats: &NetStats, now: Duration) -> Option<u32> {
        if !stats.loss_pct.is_finite()
            || !(0.0..=100.0).contains(&stats.loss_pct)
            || self
                .last
                .is_some_and(|last| now.saturating_sub(last) < Duration::from_secs(1))
        {
            return None;
        }
        self.last = Some(now);
        let congested = self.pending_write_pressure || self.core.is_congested_at(stats, now);
        self.pending_write_pressure = false;
        let previous = self.target_kbps();
        let mut feedback = stats.clone();
        if congested {
            feedback.loss_pct = feedback.loss_pct.max(3.0);
        }
        self.core.on_stats_at(&feedback, now);
        // Throughput alone is application limited, not a capacity estimate.
        // Use it only with independent congestion evidence, reserving 10% headroom.
        if congested && stats.recv_kbps > 0 {
            let cap = (u64::from(stats.recv_kbps) * 9 / 10) as u32;
            let target = self.target_kbps().min(cap).clamp(self.min, self.max);
            self.core = AimdController::new(AimdConfig {
                min_kbps: self.min,
                max_kbps: self.max,
                start_kbps: target,
                increase_kbps: 5_000,
                stable_samples_required: 2,
                ..Default::default()
            })
            .ok()?;
        }
        (previous != self.target_kbps()).then_some(self.target_kbps())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct EncoderStub {
        target: u32,
        reject: bool,
    }
    impl Encoder for EncoderStub {
        fn encode(&mut self, _: &[u8]) -> Result<Vec<u8>, trinix_core::CodecError> {
            Ok(vec![])
        }
        fn set_bitrate_kbps(&mut self, target: u32) -> Result<(), trinix_core::CodecError> {
            if self.reject {
                return Err(std::io::Error::other("rejected").into());
            }
            self.target = target;
            Ok(())
        }
    }
    fn stats(rtt_ms: u32, recv_kbps: u32) -> NetStats {
        NetStats {
            rtt_ms,
            recv_kbps,
            loss_pct: 0.0,
        }
    }
    #[test]
    fn noise_rate_limit_clamps_and_rejection() {
        let mut abr = Abr::new(20_000, 500, 20_000).unwrap();
        let mut encoder = EncoderStub::default();
        for second in 0..20 {
            abr.apply(
                &mut encoder,
                &stats(20 + second % 8, 20_000),
                Duration::from_secs(second.into()),
                false,
            )
            .unwrap();
            assert_eq!(abr.target_kbps(), 20_000);
        }
        encoder.reject = true;
        assert!(abr
            .apply(
                &mut encoder,
                &stats(100, 4_000),
                Duration::from_secs(20),
                false
            )
            .is_err());
        assert_eq!(abr.target_kbps(), 20_000);
        encoder.reject = false;
        abr.apply(
            &mut encoder,
            &stats(100, 4_000),
            Duration::from_secs(20),
            false,
        )
        .unwrap();
        assert_eq!(encoder.target, 3_600);
        for ms in 20_001..21_000 {
            assert!(abr
                .apply(
                    &mut encoder,
                    &stats(100, 1),
                    Duration::from_millis(ms),
                    true
                )
                .unwrap()
                .is_none());
        }
        abr.apply(&mut encoder, &stats(100, 1), Duration::from_secs(21), true)
            .unwrap();
        assert_eq!(abr.target_kbps(), 500);
    }

    #[test]
    fn write_pressure_survives_skipped_reports_and_encoder_rejection() {
        for invalid in [false, true] {
            let mut abr = Abr::new(10_000, 500, 20_000).unwrap();
            let mut encoder = EncoderStub::default();
            abr.apply(&mut encoder, &stats(20, 10_000), Duration::ZERO, false)
                .unwrap();
            let mut skipped = stats(20, 10_000);
            if invalid {
                skipped.loss_pct = f32::NAN;
            }
            assert_eq!(
                abr.apply(
                    &mut encoder,
                    &skipped,
                    Duration::from_millis(if invalid { 1_000 } else { 500 }),
                    true
                )
                .unwrap(),
                None
            );
            encoder.reject = true;
            assert!(abr
                .apply(
                    &mut encoder,
                    &stats(20, 10_000),
                    Duration::from_secs(1),
                    false
                )
                .is_err());
            assert_eq!(abr.target_kbps(), 10_000);
            encoder.reject = false;
            assert_eq!(
                abr.apply(
                    &mut encoder,
                    &stats(20, 10_000),
                    Duration::from_secs(1),
                    false
                )
                .unwrap(),
                Some(7_000)
            );
            // Consumed pressure must not cause another decrease.
            assert_eq!(
                abr.apply(
                    &mut encoder,
                    &stats(20, 10_000),
                    Duration::from_secs(2),
                    false
                )
                .unwrap(),
                None
            );
            assert_eq!(
                abr.apply(
                    &mut encoder,
                    &stats(20, 10_000),
                    Duration::from_secs(3),
                    false
                )
                .unwrap(),
                Some(12_000)
            );
        }
    }

    #[test]
    fn recovery_requires_two_healthy_samples() {
        let mut abr = Abr::new(10_000, 500, 20_000).unwrap();
        let mut encoder = EncoderStub::default();
        abr.apply(&mut encoder, &stats(20, 10_000), Duration::ZERO, false)
            .unwrap();
        abr.apply(
            &mut encoder,
            &stats(100, 4_000),
            Duration::from_secs(1),
            false,
        )
        .unwrap();
        let reduced = abr.target_kbps();
        assert_eq!(
            abr.apply(
                &mut encoder,
                &stats(22, 4_000),
                Duration::from_secs(2),
                false
            )
            .unwrap(),
            None
        );
        assert_eq!(
            abr.apply(
                &mut encoder,
                &stats(24, 4_000),
                Duration::from_secs(3),
                false
            )
            .unwrap(),
            Some(reduced + 5_000)
        );
    }

    // Virtual 1 ms ticks, 30 fps CBR encoder, single non-preemptible network
    // write plus one latest raw frame. No wall-clock sleeps or socket buffers.
    fn simulate(throttle: bool) -> (Vec<f64>, u32, u32, usize) {
        let mut abr = Abr::new(20_000, 500, 20_000).unwrap();
        let mut encoder = EncoderStub {
            target: 20_000,
            reject: false,
        };
        let mut remaining = 0.0_f64;
        let mut started = 0;
        let mut pending = None;
        let mut delivered = 0.0;
        let mut latencies = vec![];
        let mut blocked = false;
        let mut at_drop = 0;
        let mut recovered = 0;
        let mut max_queue = 0;
        for ms in 0_u32..30_000 {
            let capacity = if throttle && (3_000..9_000).contains(&ms) {
                4_000.0
            } else {
                20_000.0
            };
            if ms % 33 == 0 {
                pending = Some(ms);
            }
            max_queue = max_queue.max(usize::from(pending.is_some()));
            if remaining <= 0.0 {
                if let Some(captured) = pending.take() {
                    started = captured;
                    remaining = f64::from(encoder.target) / 30.0;
                }
            }
            if remaining > 0.0 {
                let sent = remaining.min(capacity / 1000.0);
                remaining -= sent;
                delivered += sent;
                if remaining <= 0.0 {
                    let age = ms - started + 1;
                    blocked |= age > 100;
                    latencies.push(f64::from(age) + 20.0);
                }
            }
            if ms % 1000 == 999 {
                let queue_ms = (remaining / capacity * 1000.0) as u32;
                abr.apply(
                    &mut encoder,
                    &stats(20 + queue_ms, delivered as u32),
                    Duration::from_millis(ms.into()),
                    blocked,
                )
                .unwrap();
                blocked = false;
                delivered = 0.0;
            }
            if ms == 4_999 {
                at_drop = encoder.target;
            }
            if ms == 18_999 {
                recovered = encoder.target;
            }
        }
        (latencies, at_drop, recovered, max_queue)
    }
    #[test]
    fn bandwidth_drop_recovery_and_latency_virtual_transport() {
        fn p95(mut values: Vec<f64>) -> f64 {
            values.sort_by(f64::total_cmp);
            values[values.len() * 95 / 100]
        }
        let baseline = p95(simulate(false).0);
        let (latency, reduced, recovered, queue) = simulate(true);
        let throttled = p95(latency);
        eprintln!("virtual latency p95: baseline={baseline}ms throttled={throttled}ms; reduced={reduced}, recovered={recovered}, raw_queue={queue}");
        assert!(reduced <= 4_000);
        assert!(recovered >= 16_000);
        assert_eq!(queue, 1);
        assert!(throttled < 2.0 * baseline);
    }
}
