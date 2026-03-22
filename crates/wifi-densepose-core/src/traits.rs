//! Core traits defining the contracts for signal processing, neural inference, and storage.

use crate::error::CoreResult;
use crate::types::{CsiFrame, PoseEstimate, ProcessedSignal};

/// Synchronous signal processor: raw CSI → processed signal + features.
pub trait SignalProcessor: Send + Sync {
    fn process(&self, frame: &CsiFrame) -> CoreResult<ProcessedSignal>;
    fn name(&self) -> &str;
}

/// Async neural network inference: processed signal → pose estimate.
pub trait NeuralInference: Send + Sync {
    fn infer(&self, signal: &ProcessedSignal) -> CoreResult<PoseEstimate>;
    fn is_ready(&self) -> bool;
    fn model_name(&self) -> &str;
}

/// Persistence layer — store and retrieve sensing data.
pub trait DataStore: Send + Sync {
    fn store_frame(&self, frame: &CsiFrame) -> CoreResult<()>;
    fn store_pose(&self, pose: &PoseEstimate) -> CoreResult<()>;
    fn frame_count(&self) -> CoreResult<u64>;
}
