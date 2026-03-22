//! Core error types for WiFi DensePose.

use thiserror::Error;

pub type CoreResult<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Signal processing error: {0}")]
    Signal(#[from] SignalError),

    #[error("Inference error: {0}")]
    Inference(#[from] InferenceError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Invalid configuration: {0}")]
    Config(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Hardware error: {0}")]
    Hardware(String),

    #[error("IO error: {0}")]
    Io(String),
}

#[derive(Debug, Error)]
pub enum SignalError {
    #[error("CSI preprocessing failed: {0}")]
    CsiPreprocessing(String),

    #[error("Phase sanitization failed: {0}")]
    PhaseSanitization(String),

    #[error("Feature extraction failed: {0}")]
    FeatureExtraction(String),

    #[error("Insufficient data: expected {expected}, got {got}")]
    InsufficientData { expected: usize, got: usize },

    #[error("Array shape mismatch: {0}")]
    ShapeMismatch(String),
}

#[derive(Debug, Error)]
pub enum InferenceError {
    #[error("Model not loaded: {0}")]
    ModelNotLoaded(String),

    #[error("Inference failed: {0}")]
    Failed(String),

    #[error("Invalid output shape: {0}")]
    InvalidOutput(String),
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Record not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}
