use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::state::AppState;

pub async fn health(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": state.version,
        "uptime_s": state.uptime_secs(),
        "frame_count": state.frames.frame_count(),
    }))
}
