use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::state::AppState;

pub async fn latest_frame(State(state): State<Arc<AppState>>) -> Json<Value> {
    match state.frames.latest_frame() {
        Some(frame) => Json(json!({
            "frame_id":     frame.metadata.frame_id,
            "timestamp":    frame.metadata.timestamp,
            "device_id":    frame.metadata.device_id,
            "n_links":      frame.n_links(),
            "n_subcarriers":frame.n_subcarriers(),
            "rssi_dbm":     frame.metadata.rssi_dbm,
            "noise_floor_dbm": frame.metadata.noise_floor_dbm,
            // Include first link amplitude as a sample
            "amplitude_sample": frame.amplitude.first().map(|r| &r[..r.len().min(16)]),
        })),
        None => Json(json!({ "error": "no frames yet" })),
    }
}

pub async fn room_field(State(state): State<Arc<AppState>>) -> Json<Value> {
    // Build a 16×16 presence field from latest pose
    let mut field = vec![vec![0.0f32; 16]; 16];

    if let Some(pose) = state.latest_pose.read().await.clone() {
        for person in &pose.persons {
            // Find nose keypoint position
            if let Some(kp) = person.keypoints.first() {
                let row = (kp.y * 15.0).clamp(0.0, 15.0) as usize;
                let col = (kp.x * 15.0).clamp(0.0, 15.0) as usize;
                let conf = person.overall_confidence.value();
                // Splat a Gaussian around the position
                for dr in -2i32..=2 {
                    for dc in -2i32..=2 {
                        let r = (row as i32 + dr).clamp(0, 15) as usize;
                        let c = (col as i32 + dc).clamp(0, 15) as usize;
                        let dist = ((dr * dr + dc * dc) as f32).sqrt();
                        field[r][c] = (field[r][c] + conf * (-dist * 0.7).exp()).min(1.0);
                    }
                }
            }
        }
    }

    Json(json!({ "field": field, "rows": 16, "cols": 16, "unit": "presence_score" }))
}
