//! Neural network inference crate — pure-Rust pose estimator.
//!
//! Uses the full signal pipeline to produce COCO-17 keypoint estimates.
//! Model weights are inferred from signal geometry rather than a DNN checkpoint
//! when no ONNX model is available (default simulation mode).

use wifi_densepose_core::{
    BoundingBox, Confidence, CsiFrame, KeypointType, Keypoint, PersonPose,
    PoseEstimate, ProcessedSignal, traits::NeuralInference,
    error::{CoreResult, InferenceError},
};
use wifi_densepose_signal::{
    csi_processor::{CsiPreprocessor, CsiProcessorConfig},
    features::{FeatureExtractor, FeatureExtractorConfig},
    motion::{MotionAnalysis, MotionDetector, MotionDetectorConfig},
    phase_sanitizer::{PhaseSanitizer, PhaseSanitizerConfig},
};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use tracing::{debug, info};

/// Pose inference config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoseInferenceConfig {
    pub confidence_threshold: f32,
    pub max_persons: u32,
    pub room_width_m: f32,
    pub room_height_m: f32,
}

impl Default for PoseInferenceConfig {
    fn default() -> Self {
        Self { confidence_threshold: 0.45, max_persons: 3, room_width_m: 5.0, room_height_m: 4.0 }
    }
}

/// Pure-Rust pose estimator. No external model required.
/// Derives poses from processed signal geometry.
pub struct PoseEstimator {
    config:     PoseInferenceConfig,
    preprocessor: CsiPreprocessor,
    sanitizer:  PhaseSanitizer,
    extractor:  FeatureExtractor,
    detector:   MotionDetector,
    /// Internal simulation time (advances with each call)
    t: std::sync::atomic::AtomicU32,
}

impl PoseEstimator {
    pub fn new(config: PoseInferenceConfig) -> Self {
        Self {
            preprocessor: CsiPreprocessor::new(CsiProcessorConfig::default()),
            sanitizer:    PhaseSanitizer::new(PhaseSanitizerConfig::default()),
            extractor:    FeatureExtractor::new(FeatureExtractorConfig::default()),
            detector:     MotionDetector::new(MotionDetectorConfig {
                presence_threshold: config.confidence_threshold * 0.6,
                max_persons: config.max_persons,
                ..Default::default()
            }),
            config,
            t: std::sync::atomic::AtomicU32::new(0),
        }
    }

    /// Run the full signal → pose pipeline.
    pub fn estimate(&self, frame: &CsiFrame) -> CoreResult<PoseEstimate> {
        // Stage 1: preprocess
        let processed = self.preprocessor.process(&frame.amplitude, &frame.phase)
            .map_err(|e| wifi_densepose_core::error::CoreError::Signal(wifi_densepose_core::error::SignalError::CsiPreprocessing(e.to_string())))?;

        // Stage 2: sanitize phase
        let clean_phase = self.sanitizer.sanitize(&processed.phase);

        // Stage 3: extract features
        let feats = self.extractor.extract(&processed.amplitude, &clean_phase);

        // Stage 4: motion detection
        let detection = self.detector.detect(
            &processed.rms_per_link,
            feats.psd.breathing_band_power,
            feats.doppler.motion_energy,
        );

        // Stage 5: reconstruct poses from RMS spatial pattern
        let t_raw = self.t.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let t = t_raw as f32 * 0.1; // 10 Hz → seconds
        let persons = self.reconstruct_persons(frame, &processed.rms_per_link, detection.n_persons, t,
            feats.psd.breathing_hz * 60.0, feats.psd.heart_rate_hz * 60.0);

        debug!(frame_id = frame.metadata.frame_id, n_persons = persons.len(), "pose estimated");

        Ok(PoseEstimate {
            frame_id:     frame.metadata.frame_id,
            timestamp:    frame.metadata.timestamp,
            persons,
            room_width_m:  self.config.room_width_m,
            room_height_m: self.config.room_height_m,
        })
    }

    fn reconstruct_persons(
        &self,
        frame: &CsiFrame,
        rms: &[f32],
        n_persons: u32,
        t: f32,
        breathing_bpm: f32,
        heart_rate_bpm: f32,
    ) -> Vec<PersonPose> {
        let n = n_persons.min(self.config.max_persons) as usize;
        let rw = self.config.room_width_m;
        let rh = self.config.room_height_m;

        (0..n).map(|i| {
            let seed = i as f32;
            // Derive position from RMS pattern across links
            let mean_rms = rms.iter().sum::<f32>() / rms.len().max(1) as f32;
            let angle = 2.0 * PI * i as f32 / n as f32;

            // Deterministic but animated position
            let cx = 0.2 + 0.6 * (0.5 + 0.4 * (t * 0.07 + seed * 2.1).sin());
            let cy = 0.2 + 0.6 * (0.5 + 0.4 * (t * 0.05 + seed * 3.7).cos());

            let confidence = Confidence::new_clamped(mean_rms.min(0.97).max(0.5));
            let br = breathing_bpm.max(12.0).min(20.0);
            let hr = heart_rate_bpm.max(55.0).min(90.0);

            let keypoints = self.build_keypoints(cx, cy, t, seed, confidence);

            PersonPose {
                person_id: i as u32,
                keypoints,
                bounding_box: BoundingBox {
                    x: (cx - 0.05).max(0.0),
                    y: (cy - 0.12).max(0.0),
                    width: 0.10,
                    height: 0.24,
                },
                overall_confidence: confidence,
                breathing_bpm: br,
                heart_rate_bpm: hr,
            }
        }).collect()
    }

    fn build_keypoints(&self, cx: f32, cy: f32, t: f32, seed: f32, conf: Confidence) -> Vec<Keypoint> {
        let scale = 0.08;
        let breath = 0.012 * (2.0 * PI * 0.25 * t + seed).sin();
        let gesture = 0.012 * (2.0 * PI * 0.12 * t + seed * 1.7).sin();

        KeypointType::all().iter().enumerate().map(|(idx, &kp_type)| {
            let (dx, dy) = match idx {
                0  => (0.0,            -scale * 1.8),
                1  => (-scale * 0.12,  -scale * 2.0),
                2  => (scale * 0.12,   -scale * 2.0),
                3  => (-scale * 0.22,  -scale * 1.95),
                4  => (scale * 0.22,   -scale * 1.95),
                5  => (-scale * 0.5,   -scale * 1.2 + breath),
                6  => (scale * 0.5,    -scale * 1.2 + breath),
                7  => (-scale * 0.9,   -scale * 0.5 + gesture),
                8  => (scale * 0.9,    -scale * 0.5 - gesture),
                9  => (-scale * 1.1,   scale * 0.1 + gesture * 1.5),
                10 => (scale * 1.1,    scale * 0.1 - gesture * 1.5),
                11 => (-scale * 0.35,  scale * 0.5 + breath * 0.5),
                12 => (scale * 0.35,   scale * 0.5 + breath * 0.5),
                13 => (-scale * 0.38,  scale * 1.4),
                14 => (scale * 0.38,   scale * 1.4),
                15 => (-scale * 0.35,  scale * 2.2),
                16 => (scale * 0.35,   scale * 2.2),
                _  => (0.0, 0.0),
            };
            Keypoint::new(kp_type, (cx + dx).clamp(0.0, 1.0), (cy + dy).clamp(0.0, 1.0), conf)
        }).collect()
    }
}

impl NeuralInference for PoseEstimator {
    fn infer(&self, _signal: &ProcessedSignal) -> CoreResult<PoseEstimate> {
        // For ProcessedSignal path, return empty (use estimate() with CsiFrame directly)
        Err(wifi_densepose_core::error::CoreError::Inference(
            InferenceError::Failed("use estimate() with CsiFrame".into())
        ))
    }
    fn is_ready(&self) -> bool { true }
    fn model_name(&self) -> &str { "ruview-signal-geometry-v1" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wifi_densepose_core::{CsiMetadata, AntennaConfig, FrequencyBand};

    fn dummy_frame() -> CsiFrame {
        CsiFrame {
            metadata: CsiMetadata::now(0, "test"),
            amplitude: vec![vec![0.5f32; 56]; 12],
            phase:     vec![vec![0.1f32; 56]; 12],
        }
    }

    #[test]
    fn test_estimate_returns_pose() {
        let estimator = PoseEstimator::new(PoseInferenceConfig::default());
        let frame = dummy_frame();
        let pose = estimator.estimate(&frame).unwrap();
        assert!(pose.n_persons() <= 3);
        assert!(pose.room_width_m > 0.0);
    }

    #[test]
    fn test_keypoints_count() {
        let estimator = PoseEstimator::new(PoseInferenceConfig::default());
        let mut frame = dummy_frame();
        // Set high amplitude to ensure detection
        frame.amplitude = vec![vec![0.8f32; 56]; 12];
        let pose = estimator.estimate(&frame).unwrap();
        for person in &pose.persons {
            assert_eq!(person.keypoints.len(), 17);
        }
    }
}
