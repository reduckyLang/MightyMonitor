//! Deterministic additive-increase, multiplicative-decrease bitrate adaptation.

use crate::Encoder;

/// Receiver feedback; will be superseded/re-exported from the typed Message module
/// in M1-3. Field names and types are chosen to match that module.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetStats {
    pub rtt_ms: u32,
    pub loss_pct: f32,
    pub recv_kbps: u32,
}

/// Updates a target bitrate from receiver feedback.
pub trait BitrateController {
    /// Returns `Some(new_kbps)` only when the target changes.
    fn on_stats(&mut self, stats: &NetStats) -> Option<u32>;
}

/// Parameters for an AIMD controller, validated by [`AimdController::new`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AimdConfig {
    pub min_kbps: u32,
    pub max_kbps: u32,
    pub start_kbps: u32,
    /// Additive step after each full run of stable samples.
    pub increase_kbps: u32,
    /// Multiplicative decrease, strictly between zero and one.
    pub decrease_factor: f32,
    /// Congestion occurs when loss strictly exceeds this percentage.
    pub loss_threshold_pct: f32,
    /// Congestion occurs when RTT strictly exceeds minimum observed RTT times this factor.
    pub rtt_rise_factor: f32,
    /// Consecutive stable samples required for each increase.
    pub stable_samples_required: u32,
}

impl Default for AimdConfig {
    fn default() -> Self {
        Self {
            min_kbps: 500,
            max_kbps: 20_000,
            start_kbps: 4_000,
            increase_kbps: 250,
            decrease_factor: 0.7,
            loss_threshold_pct: 2.0,
            rtt_rise_factor: 1.5,
            stable_samples_required: 3,
        }
    }
}

/// AIMD state with a minimum-observed RTT baseline and sample-count hysteresis.
#[derive(Debug, Clone)]
pub struct AimdController {
    config: AimdConfig,
    target_kbps: u32,
    baseline_rtt_ms: Option<u32>,
    stable_samples: u32,
}

impl AimdController {
    /// Rejects zero/inverted bounds, zero steps/sample counts, non-finite floats,
    /// decrease factors outside `(0, 1)`, loss thresholds outside `[0, 100]`, and
    /// RTT factors at or below one. Clamps the starting bitrate to the bounds.
    pub fn new(config: AimdConfig) -> Result<Self, &'static str> {
        if config.min_kbps == 0 || config.min_kbps > config.max_kbps {
            return Err("bitrate bounds must be positive and ordered");
        }
        if config.increase_kbps == 0 || config.stable_samples_required == 0 {
            return Err("increase step and stable sample count must be positive");
        }
        if !config.decrease_factor.is_finite()
            || config.decrease_factor <= 0.0
            || config.decrease_factor >= 1.0
        {
            return Err("decrease factor must be finite and strictly between zero and one");
        }
        if !config.loss_threshold_pct.is_finite()
            || !(0.0..=100.0).contains(&config.loss_threshold_pct)
        {
            return Err("loss threshold must be finite and between zero and 100");
        }
        if !config.rtt_rise_factor.is_finite() || config.rtt_rise_factor <= 1.0 {
            return Err("RTT rise factor must be finite and greater than one");
        }
        Ok(Self {
            target_kbps: config.start_kbps.clamp(config.min_kbps, config.max_kbps),
            config,
            baseline_rtt_ms: None,
            stable_samples: 0,
        })
    }

    /// Current target, including the initial clamped target before feedback.
    pub fn target_kbps(&self) -> u32 {
        self.target_kbps
    }
}

impl Default for AimdController {
    fn default() -> Self {
        Self::new(AimdConfig::default()).expect("default AIMD configuration is valid")
    }
}

impl BitrateController for AimdController {
    fn on_stats(&mut self, stats: &NetStats) -> Option<u32> {
        // Invalid loss feedback cannot establish stability or alter the RTT baseline.
        if !stats.loss_pct.is_finite() || !(0.0..=100.0).contains(&stats.loss_pct) {
            self.stable_samples = 0;
            return None;
        }
        let baseline = self
            .baseline_rtt_ms
            .map_or(stats.rtt_ms, |rtt| rtt.min(stats.rtt_ms));
        self.baseline_rtt_ms = Some(baseline);
        let congested = stats.loss_pct > self.config.loss_threshold_pct
            || f64::from(stats.rtt_ms)
                > f64::from(baseline) * f64::from(self.config.rtt_rise_factor);
        let next = if congested {
            self.stable_samples = 0;
            (f64::from(self.target_kbps) * f64::from(self.config.decrease_factor)).round() as u32
        } else {
            self.stable_samples += 1;
            if self.stable_samples < self.config.stable_samples_required {
                return None;
            }
            self.stable_samples = 0;
            self.target_kbps.saturating_add(self.config.increase_kbps)
        };
        let next = next.clamp(self.config.min_kbps, self.config.max_kbps);
        if next == self.target_kbps {
            None
        } else {
            self.target_kbps = next;
            Some(next)
        }
    }
}

/// Apply a changed controller target to the encoder and return that target.
pub fn adapt(
    encoder: &mut dyn Encoder,
    ctl: &mut dyn BitrateController,
    stats: &NetStats,
) -> Option<u32> {
    let target = ctl.on_stats(stats)?;
    encoder.set_bitrate_kbps(target);
    Some(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(rtt_ms: u32, loss_pct: f32) -> NetStats {
        NetStats {
            rtt_ms,
            loss_pct,
            recv_kbps: 4_000,
        }
    }

    #[test]
    fn decreases_on_loss() {
        let mut ctl = AimdController::default();
        assert_eq!(ctl.on_stats(&stats(20, 5.0)), Some(2_800));
    }

    #[test]
    fn increases_when_stable() {
        let mut ctl = AimdController::default();
        assert_eq!(ctl.on_stats(&stats(20, 0.0)), None);
        assert_eq!(ctl.on_stats(&stats(20, 0.0)), None);
        assert_eq!(ctl.on_stats(&stats(20, 0.0)), Some(4_250));
    }

    #[test]
    fn never_exceeds_min_max() {
        let mut ctl = AimdController::new(AimdConfig {
            min_kbps: 500,
            max_kbps: 1_000,
            start_kbps: u32::MAX,
            increase_kbps: u32::MAX,
            stable_samples_required: 1,
            ..AimdConfig::default()
        })
        .unwrap();
        assert_eq!(ctl.target_kbps(), 1_000);
        assert_eq!(ctl.on_stats(&stats(20, 0.0)), None);
        for _ in 0..20 {
            ctl.on_stats(&stats(20, 10.0));
        }
        assert_eq!(ctl.target_kbps(), 500);
        assert_eq!(ctl.on_stats(&stats(20, 10.0)), None);
        assert_eq!(ctl.on_stats(&stats(20, 0.0)), Some(1_000));
    }

    #[test]
    fn recovers_after_congestion_clears() {
        let mut ctl = AimdController::default();
        ctl.on_stats(&stats(20, 0.0));
        assert!(ctl.on_stats(&stats(40, 0.0)).is_some());
        let reduced = ctl.target_kbps();
        assert_eq!(ctl.on_stats(&stats(20, 0.0)), None);
        assert_eq!(ctl.on_stats(&stats(20, 0.0)), None);
        assert_eq!(ctl.on_stats(&stats(20, 0.0)), Some(reduced + 250));
    }

    #[test]
    fn hysteresis_resets_on_congestion_and_after_increase() {
        let mut ctl = AimdController::default();
        for _ in 0..2 {
            assert_eq!(ctl.on_stats(&stats(20, 0.0)), None);
        }
        ctl.on_stats(&stats(20, 10.0));
        for _ in 0..2 {
            assert_eq!(ctl.on_stats(&stats(20, 0.0)), None);
        }
        assert!(ctl.on_stats(&stats(20, 0.0)).is_some());
        assert_eq!(ctl.on_stats(&stats(20, 0.0)), None);
    }

    #[test]
    fn mock_encoder_receives_new_bitrate() {
        #[derive(Default)]
        struct MockEncoder {
            targets: Vec<u32>,
        }
        impl Encoder for MockEncoder {
            fn encode(&mut self, raw: &[u8]) -> crate::Bytes {
                raw.to_vec()
            }
            fn set_bitrate_kbps(&mut self, kbps: u32) {
                self.targets.push(kbps);
            }
        }
        let mut encoder = MockEncoder::default();
        let mut ctl = AimdController::default();
        assert_eq!(adapt(&mut encoder, &mut ctl, &stats(20, 0.0)), None);
        let target = adapt(&mut encoder, &mut ctl, &stats(20, 10.0)).unwrap();
        assert_eq!(encoder.targets, vec![target]);
    }

    #[test]
    fn rejects_invalid_configuration() {
        for config in [
            AimdConfig {
                min_kbps: 0,
                ..AimdConfig::default()
            },
            AimdConfig {
                max_kbps: 1,
                ..AimdConfig::default()
            },
            AimdConfig {
                increase_kbps: 0,
                ..AimdConfig::default()
            },
            AimdConfig {
                stable_samples_required: 0,
                ..AimdConfig::default()
            },
            AimdConfig {
                decrease_factor: f32::NAN,
                ..AimdConfig::default()
            },
            AimdConfig {
                decrease_factor: 1.0,
                ..AimdConfig::default()
            },
            AimdConfig {
                loss_threshold_pct: 101.0,
                ..AimdConfig::default()
            },
            AimdConfig {
                rtt_rise_factor: 1.0,
                ..AimdConfig::default()
            },
        ] {
            assert!(AimdController::new(config).is_err());
        }
    }
}
