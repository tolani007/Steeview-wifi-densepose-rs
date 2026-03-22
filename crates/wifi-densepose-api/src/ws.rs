//! WebSocket handler for real-time 10 Hz sensing stream.

use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
    extract::ws::{Message, WebSocket},
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use wifi_densepose_hardware::SimulatedAdapter;
use crate::state::AppState;

/// WebSocket upgrade handler.
pub async fn sensing_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Per-client WebSocket handler — streams data at 10 Hz.
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut ticker = interval(Duration::from_millis(100)); // 10 Hz
    let mut sim = SimulatedAdapter::new(2, 4, 56, 5.0, 4.0);
    let mut t = 0.0f32;

    info!("WebSocket client connected");

    // Spawn a receiver task to handle client messages (ping, close)
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if matches!(msg, Message::Close(_)) { break; }
        }
    });

    loop {
        ticker.tick().await;

        // Generate CSI frame
        let frame = sim.generate_frame(t, 0.1);
        t += 0.1;

        // Run pose estimation
        let pose_opt = state.estimator.estimate(&frame).ok();
        if let Some(ref pose) = pose_opt {
            *state.latest_pose.write().await = Some(pose.clone());

            // Store vitals from first person
            if let Some(p) = pose.persons.first() {
                use wifi_densepose_db::VitalSignRecord;
                state.vitals.push(VitalSignRecord {
                    timestamp:      frame.metadata.timestamp,
                    frame_id:       frame.metadata.frame_id,
                    person_id:      p.person_id,
                    breathing_bpm:  p.breathing_bpm,
                    heart_rate_bpm: p.heart_rate_bpm,
                    confidence:     p.overall_confidence.value(),
                    presence_score: p.overall_confidence.value(),
                });
            }
        }

        // Store frame
        let _ = state.frames.store_frame(frame.clone());

        // Serialize full payload
        let payload = serde_json::json!({
            "type": "sensing",
            "timestamp": frame.metadata.timestamp,
            "frame_id":  frame.metadata.frame_id,
            "pose":      pose_opt,
            "vitals":    state.vitals.latest(),
            "frame": {
                "n_links": frame.n_links(),
                "n_subcarriers": frame.n_subcarriers(),
                "rms_per_link": frame.amplitude.iter()
                    .map(|r| (r.iter().map(|x| x*x).sum::<f32>() / r.len() as f32).sqrt())
                    .collect::<Vec<_>>(),
            }
        });

        let text = match serde_json::to_string(&payload) {
            Ok(s) => s,
            Err(e) => { error!("json error: {e}"); continue; }
        };

        if sender.send(Message::Text(text.into())).await.is_err() {
            debug!("WebSocket client disconnected");
            break;
        }
    }

    recv_task.abort();
    info!("WebSocket session ended");
}
