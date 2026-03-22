use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::state::AppState;

pub async fn vital_signs(State(state): State<Arc<AppState>>) -> Json<Value> {
    match state.vitals.latest() {
        Some(v) => Json(json!({
            "timestamp":       v.timestamp,
            "frame_id":        v.frame_id,
            "person_id":       v.person_id,
            "breathing_bpm":   v.breathing_bpm,
            "heart_rate_bpm":  v.heart_rate_bpm,
            "confidence":      v.confidence,
            "presence_score":  v.presence_score,
        })),
        None => Json(json!({
            "breathing_bpm":   0.0,
            "heart_rate_bpm":  0.0,
            "confidence":      0.0,
            "presence_score":  0.0,
        })),
    }
}
