use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::state::AppState;

pub async fn current_pose(State(state): State<Arc<AppState>>) -> Json<Value> {
    match state.latest_pose.read().await.clone() {
        Some(pose) => Json(json!({
            "frame_id":     pose.frame_id,
            "timestamp":    pose.timestamp,
            "n_persons":    pose.n_persons(),
            "room_width_m":  pose.room_width_m,
            "room_height_m": pose.room_height_m,
            "persons": pose.persons.iter().map(|p| json!({
                "person_id":   p.person_id,
                "confidence":  p.overall_confidence.value(),
                "breathing_bpm": p.breathing_bpm,
                "heart_rate_bpm": p.heart_rate_bpm,
                "bounding_box": {
                    "x": p.bounding_box.x,
                    "y": p.bounding_box.y,
                    "width": p.bounding_box.width,
                    "height": p.bounding_box.height,
                },
                "keypoints": p.keypoints.iter().map(|k| json!({
                    "type": format!("{:?}", k.kp_type),
                    "x": k.x,
                    "y": k.y,
                    "confidence": k.confidence.value(),
                    "visible": k.is_visible(),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })),
        None => Json(json!({ "n_persons": 0, "persons": [] })),
    }
}
