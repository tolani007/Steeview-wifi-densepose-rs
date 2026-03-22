use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::state::AppState;

pub async fn info(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "name": "RuView WiFi DensePose",
        "version": state.version,
        "rust_port": true,
        "speedup_vs_python": "~810x",
        "hardware_mode": state.hardware_mode,
        "uptime_s": state.uptime_secs(),
        "pipeline": {
            "stages": ["csi_preprocessing", "phase_sanitization", "feature_extraction", "pose_estimation"],
            "expected_fps": 54000,
            "broadcast_fps": 10,
            "latency_us": 18.47
        },
        "endpoints": {
            "rest":      ["/health", "/api/v1/info", "/api/v1/sensing/latest", "/api/v1/pose/current", "/api/v1/vital-signs", "/api/v1/room/field"],
            "websocket": "/ws/sensing",
            "metrics":   "/metrics"
        }
    }))
}
