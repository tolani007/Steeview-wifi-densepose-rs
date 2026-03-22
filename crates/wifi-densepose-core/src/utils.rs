//! Shared utility functions.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current Unix timestamp as f64 seconds.
pub fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Compute RMS of a slice.
pub fn rms(data: &[f32]) -> f32 {
    if data.is_empty() { return 0.0; }
    let sum_sq: f32 = data.iter().map(|x| x * x).sum();
    (sum_sq / data.len() as f32).sqrt()
}

/// Compute mean of a slice.
pub fn mean(data: &[f32]) -> f32 {
    if data.is_empty() { return 0.0; }
    data.iter().sum::<f32>() / data.len() as f32
}

/// Compute std of a slice.
pub fn std_dev(data: &[f32]) -> f32 {
    if data.len() < 2 { return 0.0; }
    let m = mean(data);
    let var: f32 = data.iter().map(|x| (x - m).powi(2)).sum::<f32>() / (data.len() - 1) as f32;
    var.sqrt()
}

/// Clamp a value to [lo, hi].
pub fn clamp(v: f32, lo: f32, hi: f32) -> f32 { v.max(lo).min(hi) }
