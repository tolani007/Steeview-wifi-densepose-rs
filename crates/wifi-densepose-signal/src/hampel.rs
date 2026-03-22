//! Hampel identifier for robust outlier removal from CSI time series.
//!
//! The Hampel filter replaces values that deviate more than `k` MADs
//! from the local median with the median value.

/// Hampel filter configuration.
#[derive(Debug, Clone)]
pub struct HampelFilter {
    /// Half-window size (total window = 2*half_window + 1)
    pub half_window: usize,
    /// Threshold in units of MAD (typically 3.0)
    pub threshold: f32,
}

impl Default for HampelFilter {
    fn default() -> Self { Self { half_window: 3, threshold: 3.0 } }
}

impl HampelFilter {
    pub fn new(half_window: usize, threshold: f32) -> Self {
        Self { half_window, threshold }
    }

    /// Apply the Hampel filter in-place. Returns number of outliers replaced.
    pub fn apply(&self, data: &mut [f32]) -> usize {
        let n = data.len();
        if n < 3 { return 0; }

        let original = data.to_vec();
        let mut n_replaced = 0usize;
        let k_mad = 1.4826f32; // consistency factor for normal dist

        for i in 0..n {
            let lo = i.saturating_sub(self.half_window);
            let hi = (i + self.half_window + 1).min(n);
            let window = &original[lo..hi];

            let median = median_of(window);
            let mut deviations: Vec<f32> = window.iter().map(|&x| (x - median).abs()).collect();
            deviations.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mad = deviations[deviations.len() / 2];
            let sigma = k_mad * mad;

            let threshold = if sigma > 1e-6 { self.threshold * sigma } else { 1e-6 };
            if (original[i] - median).abs() > threshold {
                data[i] = median;
                n_replaced += 1;
            }
        }
        n_replaced
    }

    /// Apply filter across each row of a 2-D matrix (links × subcarriers).
    pub fn apply_2d(&self, matrix: &mut Vec<Vec<f32>>) -> usize {
        matrix.iter_mut().map(|row| self.apply(row)).sum()
    }
}

fn median_of(data: &[f32]) -> f32 {
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = sorted.len();
    if n % 2 == 0 {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hampel_removes_spike() {
        let mut data = vec![1.0f32; 21];
        data[10] = 100.0; // spike
        let filter = HampelFilter::new(5, 3.0);
        let n = filter.apply(&mut data);
        assert!(n >= 1, "expected at least one outlier replaced");
        assert!((data[10] - 1.0).abs() < 0.5, "spike should be replaced with median ~1.0");
    }

    #[test]
    fn test_hampel_clean_signal() {
        let mut data: Vec<f32> = (0..50).map(|i| i as f32 * 0.1).collect();
        let original = data.clone();
        let filter = HampelFilter::default();
        let n = filter.apply(&mut data);
        let max_err = data.iter().zip(original.iter()).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(max_err < 0.5, "clean signal should not be modified much: {max_err}");
        let _ = n; // may replace edges
    }
}
