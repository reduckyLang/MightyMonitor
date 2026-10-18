//! Statistics used by the latency harness.

/// Nearest-rank percentile of sorted microsecond samples (0..=100).
/// Returns None for empty input or an invalid percentile; caller sorts once.
pub fn percentile(sorted: &[u64], percent: u8) -> Option<u64> {
    if sorted.is_empty() || percent > 100 {
        return None;
    }
    let rank = (sorted.len() as u128 * u128::from(percent)).div_ceil(100);
    Some(sorted[rank.saturating_sub(1) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nearest_rank_boundaries() {
        assert_eq!(percentile(&[], 99), None);
        assert_eq!(percentile(&[5], 101), None);
        for p in [0, 50, 95, 99, 100] {
            assert_eq!(percentile(&[5], p), Some(5));
        }
        let values: Vec<_> = (1..=100).collect();
        for p in [1, 50, 95, 99, 100] {
            assert_eq!(percentile(&values, p), Some(u64::from(p)));
        }
        assert_eq!(percentile(&[1, 2, 3], 50), Some(2));
    }
}
