//! Phase sanitizer: unwrapping, linear trend removal, and outlier clipping.
//!
//! Target latency: ~3.84 µs for a 4×64 phase matrix.

use serde::{Deserialize, Serialize};

/// Phase unwrapping algorithm to use.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum UnwrappingMethod {
    #[default]
    /// Simple consecutive-difference unwrapping (fastest)
    Consecutive,
    /// Least-squares linear fit subtraction
    LinearFit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseSanitizerConfig {
    pub method: UnwrappingMethod,
    /// Clip outliers beyond ±threshold radians after unwrapping
    pub outlier_threshold_rad: f32,
    /// Remove linear trend (STO compensation)
    pub remove_linear_trend: bool,
}

impl Default for PhaseSanitizerConfig {
    fn default() -> Self {
        Self {
            method: UnwrappingMethod::Consecutive,
            outlier_threshold_rad: 10.0,
            remove_linear_trend: true,
        }
    }
}

/// Stateless phase sanitizer.
#[derive(Debug, Clone)]
pub struct PhaseSanitizer {
    config: PhaseSanitizerConfig,
}

impl PhaseSanitizer {
    pub fn new(config: PhaseSanitizerConfig) -> Self { Self { config } }

    /// Sanitize a 2-D phase matrix [n_links × n_subcarriers] in-place.
    /// Returns the sanitized matrix.
    pub fn sanitize(&self, phase: &[Vec<f32>]) -> Vec<Vec<f32>> {
        phase.iter().map(|row| self.sanitize_row(row)).collect()
    }

    fn sanitize_row(&self, phase: &[f32]) -> Vec<f32> {
        if phase.is_empty() { return vec![]; }

        // Step 1: unwrap phase
        let mut unwrapped = self.unwrap(phase);

        // Step 2: remove linear trend (removes STO/CFO offsets)
        if self.config.remove_linear_trend {
            self.subtract_linear_trend(&mut unwrapped);
        }

        // Step 3: clip outliers
        let thresh = self.config.outlier_threshold_rad;
        for v in unwrapped.iter_mut() {
            *v = v.max(-thresh).min(thresh);
        }

        unwrapped
    }

    fn unwrap(&self, phase: &[f32]) -> Vec<f32> {
        let mut out = vec![0.0f32; phase.len()];
        if phase.is_empty() { return out; }

        out[0] = phase[0];
        let two_pi = 2.0 * std::f32::consts::PI;

        match self.config.method {
            UnwrappingMethod::Consecutive => {
                for i in 1..phase.len() {
                    let mut d = phase[i] - phase[i - 1];
                    // Wrap d to [-π, π]
                    while d > std::f32::consts::PI  { d -= two_pi; }
                    while d < -std::f32::consts::PI { d += two_pi; }
                    out[i] = out[i - 1] + d;
                }
            }
            UnwrappingMethod::LinearFit => {
                // First unwrap consecutively, then subtract best-fit line
                for i in 1..phase.len() {
                    let mut d = phase[i] - phase[i - 1];
                    while d > std::f32::consts::PI  { d -= two_pi; }
                    while d < -std::f32::consts::PI { d += two_pi; }
                    out[i] = out[i - 1] + d;
                }
                self.subtract_linear_trend(&mut out);
            }
        }
        out
    }

    fn subtract_linear_trend(&self, data: &mut Vec<f32>) {
        let n = data.len();
        if n < 2 { return; }

        // Ordinary least squares: y = a*x + b
        let n_f = n as f32;
        let xs: Vec<f32> = (0..n).map(|i| i as f32).collect();
        let sum_x:  f32 = xs.iter().sum();
        let sum_y:  f32 = data.iter().sum();
        let sum_xx: f32 = xs.iter().map(|x| x * x).sum();
        let sum_xy: f32 = xs.iter().zip(data.iter()).map(|(x, y)| x * y).sum();

        let denom = n_f * sum_xx - sum_x * sum_x;
        if denom.abs() < 1e-10 { return; }
        let a = (n_f * sum_xy - sum_x * sum_y) / denom;
        let b = (sum_y - a * sum_x) / n_f;

        for (i, v) in data.iter_mut().enumerate() {
            *v -= a * i as f32 + b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn test_unwrap_monotone() {
        let sanitizer = PhaseSanitizer::new(PhaseSanitizerConfig {
            remove_linear_trend: false,
            ..Default::default()
        });
        // Wrap a linearly increasing phase
        let phase: Vec<f32> = (0..64)
            .map(|i| ((i as f32 * 0.3) % (2.0 * PI)) - PI)
            .collect();
        let out = sanitizer.sanitize_row(&phase);
        assert_eq!(out.len(), 64);
        // Unwrapped should be monotone-ish (no big jumps)
        for i in 1..out.len() {
            let jump = (out[i] - out[i - 1]).abs();
            assert!(jump < PI + 0.1, "unexpected phase jump {jump} at index {i}");
        }
    }

    #[test]
    fn test_linear_trend_removal() {
        let sanitizer = PhaseSanitizer::new(PhaseSanitizerConfig::default());
        let n = 64usize;
        // Pure linear phase ramp
        let phase: Vec<f32> = (0..n).map(|i| i as f32 * 0.1).collect();
        let out = sanitizer.sanitize_row(&phase);
        let std: f32 = {
            let m = out.iter().sum::<f32>() / out.len() as f32;
            let var = out.iter().map(|v| (v - m).powi(2)).sum::<f32>() / out.len() as f32;
            var.sqrt()
        };
        assert!(std < 1.0, "after linear trend removal, std should be small, got {std}");
    }
}
