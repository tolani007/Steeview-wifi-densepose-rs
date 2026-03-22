use axum::{extract::State, response::IntoResponse, http::header};
use std::sync::Arc;
use crate::state::AppState;

/// Prometheus-compatible metrics endpoint.
pub async fn prometheus_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let frames   = state.frames.frame_count();
    let poses    = state.frames.pose_count();
    let uptime   = state.uptime_secs();
    let vitals_n = state.vitals.latest().map(|v| v.frame_id).unwrap_or(0);

    let body = format!(
        "# HELP ruview_frames_total Total CSI frames processed\n\
         # TYPE ruview_frames_total counter\n\
         ruview_frames_total {frames}\n\
         # HELP ruview_poses_total Total pose estimates generated\n\
         # TYPE ruview_poses_total counter\n\
         ruview_poses_total {poses}\n\
         # HELP ruview_uptime_seconds Server uptime in seconds\n\
         # TYPE ruview_uptime_seconds gauge\n\
         ruview_uptime_seconds {uptime:.3}\n\
         # HELP ruview_latest_vital_frame_id Frame ID of latest vital sign reading\n\
         # TYPE ruview_latest_vital_frame_id gauge\n\
         ruview_latest_vital_frame_id {vitals_n}\n"
    );

    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body)
}
