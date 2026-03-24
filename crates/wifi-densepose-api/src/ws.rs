//! WebSocket handler for real-time 10 Hz sensing stream.
//!
//! In simulation mode (state.csi_tx == None) each client runs its own SimulatedAdapter.
//! In UDP mode (state.csi_tx == Some) the client subscribes to the shared broadcast channel
//! that is fed by the UdpAdapter running in the background.

use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
    extract::ws::{Message, WebSocket},
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info};
use wifi_densepose_hardware::SimulatedAdapter;
use wifi_densepose_db::VitalSignRecord;
use crate::state::AppState;

/// WebSocket upgrade handler.
pub async fn sensing_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Per-client WebSocket handler — dispatches to the correct data source.
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    if state.csi_tx.is_some() {
        handle_socket_udp(socket, state).await;
    } else {
        handle_socket_sim(socket, state).await;
    }
}

// ─── Simulation mode ──────────────────────────────────────────────────────────

/// Streams 10 Hz using a per-client SimulatedAdapter (no hardware required).
async fn handle_socket_sim(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut ticker = interval(Duration::from_millis(100)); // 10 Hz
    let mut sim = SimulatedAdapter::new(2, 4, 56, 5.0, 4.0);
    let mut t = 0.0f32;

    info!("WebSocket client connected (simulation mode)");

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if matches!(msg, Message::Close(_)) { break; }
        }
    });

    loop {
        ticker.tick().await;
        let frame = sim.generate_frame(t, 0.1);
        t += 0.1;

        if let Some(text) = process_and_serialize(&state, frame).await {
            if sender.send(Message::Text(text.into())).await.is_err() {
                debug!("WebSocket client disconnected");
                break;
            }
        }
    }

    recv_task.abort();
    info!("WebSocket simulation session ended");
}

// ─── UDP / Real-hardware mode ─────────────────────────────────────────────────

/// Streams frames arriving from the UdpAdapter broadcast channel.
async fn handle_socket_udp(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe before doing anything — never miss a frame.
    let mut csi_rx = match state.subscribe() {
        Some(rx) => rx,
        None => { return; }
    };

    info!("WebSocket client connected (UDP / real-hardware mode)");

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if matches!(msg, Message::Close(_)) { break; }
        }
    });

    loop {
        match csi_rx.recv().await {
            Ok(frame) => {
                if let Some(text) = process_and_serialize(&state, (*frame).clone()).await {
                    if sender.send(Message::Text(text.into())).await.is_err() {
                        debug!("WebSocket client disconnected");
                        break;
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                // Client is too slow — drop skipped frames, continue
                debug!("WebSocket lagged by {n} frames");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                info!("CSI broadcast channel closed — UDP adapter stopped");
                break;
            }
        }
    }

    recv_task.abort();
    info!("WebSocket UDP session ended");
}

// ─── Shared processing ────────────────────────────────────────────────────────

/// Run pose estimation + vitals storage + JSON serialisation.
/// Returns None only on serialisation error.
async fn process_and_serialize(
    state: &Arc<AppState>,
    frame: wifi_densepose_core::CsiFrame,
) -> Option<String> {
    let pose_opt = state.estimator.estimate(&frame).ok();

    if let Some(ref pose) = pose_opt {
        *state.latest_pose.write().await = Some(pose.clone());

        if let Some(p) = pose.persons.first() {
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

    let _ = state.frames.store_frame(frame.clone());

    let rms: Vec<f32> = frame.amplitude.iter()
        .map(|r| (r.iter().map(|x| x * x).sum::<f32>() / r.len() as f32).sqrt())
        .collect();

    let payload = serde_json::json!({
        "type":      "sensing",
        "timestamp": frame.metadata.timestamp,
        "frame_id":  frame.metadata.frame_id,
        "hardware":  state.hardware_mode,
        "pose":      pose_opt,
        "vitals":    state.vitals.latest(),
        "frame": {
            "n_links":       frame.n_links(),
            "n_subcarriers": frame.n_subcarriers(),
            "rms_per_link":  rms,
        }
    });

    match serde_json::to_string(&payload) {
        Ok(s) => Some(s),
        Err(e) => { error!("json error: {e}"); None }
    }
}
