//! Monotonic timestamps and caller-supplied NTP clock-offset samples.
use std::{collections::VecDeque, num::NonZeroUsize, time::Instant};

/// A source of monotonic microseconds. Inject a fake source in consumers' tests.
pub trait TimeSource {
    fn now_us(&self) -> u64;
}

impl<F: Fn() -> u64> TimeSource for F {
    fn now_us(&self) -> u64 {
        self()
    }
}

/// Microseconds since a process-local origin. Clones preserve the same origin.
#[derive(Clone, Debug)]
pub struct MonotonicClock {
    origin: Instant,
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MonotonicClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }

    pub fn now_us(&self) -> u64 {
        self.origin.elapsed().as_micros().min(u64::MAX as u128) as u64
    }
}

impl TimeSource for MonotonicClock {
    fn now_us(&self) -> u64 {
        self.now_us()
    }
}

/// Offset is peer clock minus local clock; RTT excludes peer processing time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OffsetEstimate {
    pub offset_us: i64,
    pub rtt_us: u64,
}

/// Retains the newest samples and selects the minimum RTT (oldest wins ties).
/// Persistent path asymmetry biases the estimate by half the delay difference.
pub struct OffsetEstimator {
    capacity: NonZeroUsize,
    samples: VecDeque<OffsetEstimate>,
}

impl Default for OffsetEstimator {
    fn default() -> Self {
        Self::new(NonZeroUsize::new(32).unwrap())
    }
}

impl OffsetEstimator {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            samples: VecDeque::new(),
        }
    }

    /// Add t0 local send, t1 peer receive, t2 peer send, t3 local receive.
    /// Reject negative RTT, reversed per-host times, or unrepresentable offsets
    /// without modifying the window. Arithmetic uses i128 to avoid overflow.
    pub fn add_sample(&mut self, t0: u64, t1: u64, t2: u64, t3: u64) -> Option<OffsetEstimate> {
        let rtt = (i128::from(t3) - i128::from(t0)) - (i128::from(t2) - i128::from(t1));
        if t3 < t0 || t2 < t1 || rtt < 0 {
            return None;
        }
        let offset = ((i128::from(t1) - i128::from(t0)) + (i128::from(t2) - i128::from(t3))) / 2;
        let sample = OffsetEstimate {
            offset_us: i64::try_from(offset).ok()?,
            rtt_us: u64::try_from(rtt).ok()?,
        };
        if self.samples.len() == self.capacity.get() {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
        self.estimate()
    }

    pub fn estimate(&self) -> Option<OffsetEstimate> {
        self.samples
            .iter()
            .min_by_key(|sample| sample.rtt_us)
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonicity_and_injected_source() {
        let clock = MonotonicClock::new();
        let clone = clock.clone();
        let mut previous = 0;
        for _ in 0..1000 {
            let now = clock.now_us();
            assert!(now >= previous);
            assert!(clone.now_us() >= now);
            previous = now;
        }
        let fake = || 123;
        assert_eq!(TimeSource::now_us(&fake), 123);
    }

    #[test]
    fn clock_offset_estimator_converges() {
        let mut estimator = OffsetEstimator::default();
        let mut seed = 42_u64;
        let mut random = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            seed >> 32
        };
        for n in 0..1000 {
            let t0 = 10_000_000 + n * 100_000;
            // Independent asymmetric queue delays, plus a symmetric 100us floor.
            let outbound = 100 + random() % 5000;
            let inbound = 100 + random() % 5000;
            let t1 = t0 + 1_234_567 + outbound;
            estimator
                .add_sample(t0, t1, t1 + 50, t0 + outbound + 50 + inbound)
                .unwrap();
        }
        assert!((estimator.estimate().unwrap().offset_us - 1_234_567).abs() < 1000);
    }

    #[test]
    fn rejects_negative_rtt_and_invalid_samples() {
        let mut e = OffsetEstimator::default();
        assert_eq!(e.estimate(), None);
        assert_eq!(e.add_sample(10, 20, 40, 15), None);
        assert_eq!(e.add_sample(10, 40, 20, 50), None);
        assert_eq!(e.add_sample(50, 20, 20, 10), None);
        assert_eq!(e.add_sample(0, u64::MAX, u64::MAX, 0), None);
        assert_eq!(e.estimate(), None);
        assert_eq!(e.add_sample(100, 50, 50, 120).unwrap().offset_us, -60);
        let before = e.estimate();
        assert_eq!(e.add_sample(10, 20, 40, 15), None);
        assert_eq!(e.estimate(), before);
    }

    #[test]
    fn window_eviction() {
        let mut e = OffsetEstimator::new(NonZeroUsize::new(2).unwrap());
        e.add_sample(0, 10, 10, 2);
        e.add_sample(0, 20, 20, 10);
        assert_eq!(e.estimate().unwrap().rtt_us, 2);
        e.add_sample(0, 30, 30, 20);
        assert_eq!(
            e.estimate().unwrap(),
            OffsetEstimate {
                offset_us: 15,
                rtt_us: 10
            }
        );
    }
}
