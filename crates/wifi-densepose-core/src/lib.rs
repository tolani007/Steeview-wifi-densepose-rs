//! # WiFi-DensePose Core
//!
//! Core types, traits, and error definitions for the WiFi DensePose system.
//! Every other crate in the workspace depends on this one.
//!
//! # Example
//! ```rust
//! use wifi_densepose_core::{CsiFrame, Keypoint, KeypointType, Confidence};
//!
//! let kp = Keypoint::new(KeypointType::Nose, 0.5, 0.3, Confidence::new(0.95).unwrap());
//! assert!(kp.is_visible());
//! ```

#![forbid(unsafe_code)]

pub mod error;
pub mod traits;
pub mod types;
pub mod utils;

pub use error::{CoreError, CoreResult, InferenceError, SignalError, StorageError};
pub use traits::{DataStore, NeuralInference, SignalProcessor};
pub use types::{
    AntennaConfig, BoundingBox, Confidence, CsiFrame, CsiMetadata, DeviceId, FrameId,
    FrequencyBand, Keypoint, KeypointType, PersonPose, PoseEstimate, ProcessedSignal,
    SignalFeatures, Timestamp,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_KEYPOINTS: usize = 17;
pub const MAX_SUBCARRIERS: usize = 256;
pub const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.5;

pub mod prelude {
    pub use crate::error::{CoreError, CoreResult};
    pub use crate::traits::{DataStore, NeuralInference, SignalProcessor};
    pub use crate::types::*;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_valid() { assert!(!VERSION.is_empty()); }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_KEYPOINTS, 17);
        assert!(MAX_SUBCARRIERS > 0);
        assert!(DEFAULT_CONFIDENCE_THRESHOLD > 0.0 && DEFAULT_CONFIDENCE_THRESHOLD < 1.0);
    }
}
