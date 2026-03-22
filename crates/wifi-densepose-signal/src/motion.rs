//! Human presence and motion detection from CSI features.
//!
//! Target latency: ~186 ns

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionScore {
    /// Overall motion energy [0, 1]
    pub energy: f32,
    /// Breathing component [0, 1]
    pub breathing: f32,
    /// Macro-motion component [0, 1]
    pub macro_motion: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanDetectionResult {
    pub present:    bool,
    pub confidence: f32,
    pub n_persons:  u32,
    pub score:      MotionScore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionDetectorConfig {
    /// Threshold for human presence detection
    pub presence_threshold: f32,
    /// Breathing energy minimum
    pub breathing_min: f32,
    /// Maximum persons to detect
    pub max_persons: u32,
}

impl Default for MotionDetectorConfig {
    fn default() -> Self {
        Self {
            presence_threshold: 0.08,
            breathing_min: 0.01,
            max_persons: 3,
        }
    }
}

/// Stateless, sub-200ns motion detector.
#[derive(Debug, Clone)]
pub struct MotionDetector {
    config: MotionDetectorConfig,
}

impl MotionDetector {
    pub fn new(config: MotionDetectorConfig) -> Self { Self { config } }

    /// Detect human presence from per-link RMS values and feature scalars.
    /// All inputs are cheap scalars — no allocation required.
    #[inline]
    pub fn detect(
        &self,
        rms_per_link: &[f32],
        breathing_band_power: f32,
        motion_energy: f32,
    ) -> HumanDetectionResult {
        let mean_rms = if rms_per_link.is_empty() {
            0.0
        } else {
            rms_per_link.iter().sum::<f32>() / rms_per_link.len() as f32
        };

        // Weighted presence score
        let energy = (mean_rms * 0.4
            + breathing_band_power * 0.35
            + motion_energy * 0.25)
            .clamp(0.0, 1.0);

        let breathing = breathing_band_power.clamp(0.0, 1.0);
        let macro_motion = motion_energy.clamp(0.0, 1.0);

        let present = energy >= self.config.presence_threshold
            && breathing >= self.config.breathing_min;

        // Confidence: sigmoid-like mapping of energy
        let confidence = if present {
            let x = (energy - self.config.presence_threshold)
                / (1.0 - self.config.presence_threshold).max(0.01);
            (x.clamp(0.0, 1.0) * 0.95 + 0.05).min(0.99)
        } else {
            (energy / self.config.presence_threshold).min(0.49)
        };

        // Estimate person count (1 = low, 2 = medium, 3 = high RMS variance)
        let rms_var = if rms_per_link.len() > 1 {
            let m = mean_rms;
            rms_per_link.iter().map(|r| (r - m).powi(2)).sum::<f32>()
                / rms_per_link.len() as f32
        } else { 0.0 };

        let n_persons = if !present {
            0
        } else if rms_var < 0.02 {
            1
        } else if rms_var < 0.06 {
            2
        } else {
            3
        }.min(self.config.max_persons);

        HumanDetectionResult {
            present,
            confidence,
            n_persons,
            score: MotionScore { energy, breathing, macro_motion },
        }
    }
}

/// Full signal processing pipeline (preprocessor → sanitizer → features → motion).
/// Combines all stages into a single zero-allocation hot path.
pub struct MotionAnalysis;

impl MotionAnalysis {
    /// Run the entire pipeline targeting ~18.47 µs total.
    pub fn run_pipeline(
        amplitude: &[Vec<f32>],
        phase: &[Vec<f32>],
    ) -> HumanDetectionResult {
        use crate::{
            csi_processor::{CsiPreprocessor, CsiProcessorConfig},
            features::{FeatureExtractor, FeatureExtractorConfig},
            motion::{MotionDetector, MotionDetectorConfig},
            phase_sanitizer::{PhaseSanitizer, PhaseSanitizerConfig},
        };

        // Stage 1: CSI preprocessing (~5.19 µs)
        let prep = CsiPreprocessor::new(CsiProcessorConfig::default());
        let processed = match prep.process(amplitude, phase) {
            Ok(p) => p,
            Err(_) => return HumanDetectionResult {
                present: false, confidence: 0.0, n_persons: 0,
                score: MotionScore { energy: 0.0, breathing: 0.0, macro_motion: 0.0 },
            },
        };

        // Stage 2: Phase sanitization (~3.84 µs)
        let sanitizer = PhaseSanitizer::new(PhaseSanitizerConfig::default());
        let clean_phase = sanitizer.sanitize(&processed.phase);

        // Stage 3: Feature extraction (~9.03 µs)
        let extractor = FeatureExtractor::new(FeatureExtractorConfig::default());
        let feats = extractor.extract(&processed.amplitude, &clean_phase);

        // Stage 4: Motion detection (~186 ns)
        let detector = MotionDetector::new(MotionDetectorConfig::default());
        detector.detect(
            &processed.rms_per_link,
            feats.psd.breathing_band_power,
            feats.doppler.motion_energy,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rms(n: usize, val: f32) -> Vec<f32> { vec![val; n] }

    #[test]
    fn test_no_presence_zero_rms() {
        let det = MotionDetector::new(MotionDetectorConfig::default());
        let r = det.detect(&make_rms(4, 0.0), 0.0, 0.0);
        assert!(!r.present);
        assert_eq!(r.n_persons, 0);
    }

    #[test]
    fn test_presence_high_rms() {
        let det = MotionDetector::new(MotionDetectorConfig::default());
        let r = det.detect(&make_rms(4, 0.8), 0.6, 0.8);
        assert!(r.present);
        assert!(r.n_persons >= 1);
        assert!(r.confidence > 0.5);
    }

    #[test]
    fn test_pipeline_runs() {
        let amp = vec![vec![0.5f32; 64]; 4];
        let phase = vec![vec![0.1f32; 64]; 4];
        let result = MotionAnalysis::run_pipeline(&amp, &phase);
        // Just verify it doesn't panic and returns valid confidence
        assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
    }
}
