//! WebAssembly bindings for browser-side CSI processing.

use wasm_bindgen::prelude::*;
use wifi_densepose_signal::motion::MotionAnalysis;
use serde_json;

/// Process a CSI matrix in the browser and return a JSON result.
///
/// amplitude_flat: row-major f32 array [n_links * n_subcarriers]
/// n_links: number of antenna links
/// n_subcarriers: number of subcarriers per link
#[wasm_bindgen]
pub fn process_csi_js(
    amplitude_flat: &[f32],
    n_links: usize,
    n_subcarriers: usize,
) -> String {
    if n_links == 0 || n_subcarriers == 0 || amplitude_flat.len() < n_links * n_subcarriers {
        return r#"{"error":"invalid dimensions"}"#.into();
    }

    // Reshape flat array → 2D
    let amplitude: Vec<Vec<f32>> = (0..n_links)
        .map(|l| amplitude_flat[l * n_subcarriers..(l + 1) * n_subcarriers].to_vec())
        .collect();
    let phase = vec![vec![0.0f32; n_subcarriers]; n_links];

    let result = MotionAnalysis::run_pipeline(&amplitude, &phase);

    serde_json::json!({
        "present":        result.present,
        "confidence":     result.confidence,
        "n_persons":      result.n_persons,
        "motion_energy":  result.score.energy,
        "breathing":      result.score.breathing,
        "macro_motion":   result.score.macro_motion,
    }).to_string()
}

/// Library version string.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").into()
}
