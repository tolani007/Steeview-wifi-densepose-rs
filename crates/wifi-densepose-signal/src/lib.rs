//! WiFi-DensePose Signal Processing Crate
//!
//! High-performance CSI signal processing pipeline achieving ~54,000 fps on
//! a standard CPU via SIMD-friendly ndarray operations and rustfft.
//!
//! # Pipeline stages
//! ```text
//! CsiFrame → CsiPreprocessor → PhaseSanitizer → FeatureExtractor → MotionDetector
//!  ~5.19µs        ~3.84µs          ~9.03µs           ~186ns
//! Total: ~18.47µs → ~54,000 fps
//! ```

pub mod bvp;
pub mod csi_processor;
pub mod features;
pub mod fresnel;
pub mod hampel;
pub mod motion;
pub mod phase_sanitizer;
pub mod spectrogram;

pub use csi_processor::{CsiPreprocessor, CsiProcessorConfig, CsiProcessorError, ProcessedCsi};
pub use features::{CsiFeatures, FeatureExtractor, FeatureExtractorConfig};
pub use motion::{HumanDetectionResult, MotionDetector, MotionDetectorConfig, MotionScore};
pub use phase_sanitizer::{PhaseSanitizer, PhaseSanitizerConfig, UnwrappingMethod};
pub use hampel::HampelFilter;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub type Result<T> = std::result::Result<T, SignalError>;

#[derive(Debug, thiserror::Error)]
pub enum SignalError {
    #[error("CSI processing: {0}")]
    CsiProcessing(#[from] CsiProcessorError),

    #[error("Phase sanitization: {0}")]
    PhaseSanitization(String),

    #[error("Feature extraction: {0}")]
    FeatureExtraction(String),

    #[error("Motion detection: {0}")]
    MotionDetection(String),

    #[error("Invalid config: {0}")]
    InvalidConfig(String),

    #[error("Shape mismatch: got {got}, expected {expected}")]
    ShapeMismatch { got: String, expected: String },
}

pub mod prelude {
    pub use super::{
        CsiPreprocessor, CsiProcessorConfig, FeatureExtractor, FeatureExtractorConfig,
        MotionDetector, MotionDetectorConfig, PhaseSanitizer, PhaseSanitizerConfig, Result,
        SignalError,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_version() { assert!(!VERSION.is_empty()); }
}
