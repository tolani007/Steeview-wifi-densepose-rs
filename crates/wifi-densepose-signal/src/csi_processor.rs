//! CSI signal preprocessor: Hampel filtering, windowing, normalization, phase cleaning.
//!
//! Target latency: ~5.19 µs for a 4×64 CSI matrix.

use crate::hampel::HampelFilter;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CsiProcessorError {
    #[error("No subcarrier data in frame")]
    EmptyFrame,
    #[error("Invalid dimensions: links={links}, subcarriers={subcarriers}")]
    InvalidDimensions { links: usize, subcarriers: usize },
}

/// Processed CSI matrix after noise removal and normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedCsi {
    /// Shape: [n_links, n_subcarriers] — cleaned amplitude
    pub amplitude: Vec<Vec<f32>>,
    /// Shape: [n_links, n_subcarriers] — sanitized phase
    pub phase: Vec<Vec<f32>>,
    pub n_links: usize,
    pub n_subcarriers: usize,
    /// RMS per link
    pub rms_per_link: Vec<f32>,
}

impl ProcessedCsi {
    pub fn mean_amplitude(&self) -> f32 {
        let total: f32 = self.amplitude.iter()
            .flat_map(|row| row.iter())
            .sum();
        let count = (self.n_links * self.n_subcarriers) as f32;
        if count > 0.0 { total / count } else { 0.0 }
    }
}

/// Configuration for the CSI preprocessor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsiProcessorConfig {
    /// Noise floor in dB — subcarriers below this are zeroed
    pub noise_floor_db: f32,
    /// Hampel filter half-window
    pub hampel_half_window: usize,
    /// Hampel outlier threshold (MADs)
    pub hampel_threshold: f32,
    /// Normalize amplitude to [0, 1]
    pub normalize: bool,
    /// Apply Hanning window across subcarriers
    pub apply_window: bool,
}

impl Default for CsiProcessorConfig {
    fn default() -> Self {
        Self {
            noise_floor_db: -30.0,
            hampel_half_window: 3,
            hampel_threshold: 3.0,
            normalize: true,
            apply_window: true,
        }
    }
}

/// Stateless CSI preprocessor. Clone freely across threads.
#[derive(Debug, Clone)]
pub struct CsiPreprocessor {
    config: CsiProcessorConfig,
    hampel: HampelFilter,
}

impl CsiPreprocessor {
    pub fn new(config: CsiProcessorConfig) -> Self {
        let hampel = HampelFilter::new(config.hampel_half_window, config.hampel_threshold);
        Self { config, hampel }
    }

    /// Preprocess a raw CSI amplitude+phase matrix.
    ///
    /// Steps:
    /// 1. Noise gating (zero subcarriers below noise floor)
    /// 2. Hampel filter per link (outlier removal)
    /// 3. Hanning window (spectral leakage reduction)
    /// 4. L∞ normalization per link
    pub fn process(
        &self,
        amplitude: &[Vec<f32>],
        phase: &[Vec<f32>],
    ) -> Result<ProcessedCsi, CsiProcessorError> {
        let n_links = amplitude.len();
        if n_links == 0 {
            return Err(CsiProcessorError::EmptyFrame);
        }
        let n_subcarriers = amplitude[0].len();
        if n_subcarriers == 0 {
            return Err(CsiProcessorError::InvalidDimensions { links: n_links, subcarriers: 0 });
        }

        // Generate Hanning window once
        let hanning: Vec<f32> = if self.config.apply_window {
            (0..n_subcarriers)
                .map(|i| {
                    0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (n_subcarriers - 1) as f32).cos())
                })
                .collect()
        } else {
            vec![1.0f32; n_subcarriers]
        };

        let mut out_amp: Vec<Vec<f32>> = Vec::with_capacity(n_links);
        let mut out_phase: Vec<Vec<f32>> = Vec::with_capacity(n_links);
        let mut rms_per_link: Vec<f32> = Vec::with_capacity(n_links);

        for (link_amp, link_phase) in amplitude.iter().zip(phase.iter()) {
            // 1. Noise gate
            let mut amp: Vec<f32> = link_amp
                .iter()
                .map(|&a| if a < self.config.noise_floor_db { 0.0 } else { a - self.config.noise_floor_db })
                .collect();

            // 2. Hampel filter
            self.hampel.apply(&mut amp);

            // 3. Hanning window
            for (a, &w) in amp.iter_mut().zip(hanning.iter()) {
                *a *= w;
            }

            // 4. L∞ normalize
            if self.config.normalize {
                let max = amp.iter().cloned().fold(0.0f32, f32::max);
                if max > 0.0 {
                    amp.iter_mut().for_each(|a| *a /= max);
                }
            }

            // RMS
            let rms = {
                let sq_sum: f32 = amp.iter().map(|&v| v * v).sum();
                (sq_sum / amp.len() as f32).sqrt()
            };
            rms_per_link.push(rms);

            // Phase — just copy for now (sanitizer handles unwrapping)
            let ph: Vec<f32> = link_phase.to_vec();

            out_amp.push(amp);
            out_phase.push(ph);
        }

        Ok(ProcessedCsi {
            amplitude: out_amp,
            phase: out_phase,
            n_links,
            n_subcarriers,
            rms_per_link,
        })
    }
}

// Legacy alias for API compatibility
pub type CsiProcessor = CsiPreprocessor;
pub type CsiProcessorConfigBuilder = CsiProcessorConfig;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_csi(n_links: usize, n_sc: usize, val: f32) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        let amp = vec![vec![val; n_sc]; n_links];
        let phase = vec![vec![0.0f32; n_sc]; n_links];
        (amp, phase)
    }

    #[test]
    fn test_preprocess_basic() {
        let proc = CsiPreprocessor::new(CsiProcessorConfig::default());
        let (amp, phase) = make_test_csi(4, 64, -15.0);
        let out = proc.process(&amp, &phase).unwrap();
        assert_eq!(out.n_links, 4);
        assert_eq!(out.n_subcarriers, 64);
        assert_eq!(out.rms_per_link.len(), 4);
    }

    #[test]
    fn test_preprocess_empty_fails() {
        let proc = CsiPreprocessor::new(CsiProcessorConfig::default());
        let result = proc.process(&[], &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_noise_gating() {
        let proc = CsiPreprocessor::new(CsiProcessorConfig { noise_floor_db: -20.0, normalize: false, apply_window: false, ..Default::default() });
        // All values below noise floor → should be zeroed
        let (amp, phase) = make_test_csi(1, 8, -30.0);
        let out = proc.process(&amp, &phase).unwrap();
        let all_zero = out.amplitude[0].iter().all(|&v| v == 0.0);
        assert!(all_zero, "below-noise subcarriers should be zero");
    }

    #[test]
    fn test_normalize_max_one() {
        let proc = CsiPreprocessor::new(CsiProcessorConfig { apply_window: false, ..Default::default() });
        let (amp, phase) = make_test_csi(2, 16, -10.0);
        let out = proc.process(&amp, &phase).unwrap();
        for row in &out.amplitude {
            let max = row.iter().cloned().fold(0.0f32, f32::max);
            assert!((max - 1.0).abs() < 1e-5 || max == 0.0, "normalized max should be 1.0");
        }
    }
}
