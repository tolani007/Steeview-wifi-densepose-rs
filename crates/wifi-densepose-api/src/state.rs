//! Shared API application state.

use std::sync::Arc;
use tokio::sync::RwLock;
use wifi_densepose_core::PoseEstimate;
use wifi_densepose_db::{InMemoryStore, VitalSignStore};
use wifi_densepose_hardware::{CsiSender, CsiReceiver};
use wifi_densepose_nn::PoseEstimator;

/// Thread-safe shared state for all API handlers.
#[derive(Clone)]
pub struct AppState {
    pub frames:        Arc<InMemoryStore>,
    pub vitals:        Arc<VitalSignStore>,
    pub latest_pose:   Arc<RwLock<Option<PoseEstimate>>>,
    pub estimator:     Arc<PoseEstimator>,
    pub version:       String,
    pub uptime_start:  std::time::Instant,
    pub hardware_mode: String,
    /// Broadcast channel — every WebSocket client subscribes here.
    /// When None, the WebSocket falls back to its own SimulatedAdapter.
    pub csi_tx:        Option<CsiSender>,
}

impl AppState {
    /// Simulated mode — each WebSocket client generates its own frames.
    pub fn new_simulated(n_persons: usize) -> Self {
        use wifi_densepose_nn::PoseInferenceConfig;
        let _ = n_persons; // exposed for future room-config use
        Self {
            frames:        Arc::new(InMemoryStore::new(50_000)),
            vitals:        Arc::new(VitalSignStore::new(10_000)),
            latest_pose:   Arc::new(RwLock::new(None)),
            estimator:     Arc::new(PoseEstimator::new(PoseInferenceConfig::default())),
            version:       "0.4.0".into(),
            uptime_start:  std::time::Instant::now(),
            hardware_mode: "simulation".into(),
            csi_tx:        None,
        }
    }

    /// UDP mode — frames are fed through the shared broadcast channel.
    pub fn new_udp(csi_tx: CsiSender) -> Self {
        use wifi_densepose_nn::PoseInferenceConfig;
        Self {
            frames:        Arc::new(InMemoryStore::new(50_000)),
            vitals:        Arc::new(VitalSignStore::new(10_000)),
            latest_pose:   Arc::new(RwLock::new(None)),
            estimator:     Arc::new(PoseEstimator::new(PoseInferenceConfig::default())),
            version:       "0.4.0".into(),
            uptime_start:  std::time::Instant::now(),
            hardware_mode: "udp".into(),
            csi_tx:        Some(csi_tx),
        }
    }

    /// Subscribe to the CSI broadcast channel (UDP mode).
    pub fn subscribe(&self) -> Option<CsiReceiver> {
        self.csi_tx.as_ref().map(|tx| tx.subscribe())
    }

    pub fn uptime_secs(&self) -> f64 {
        self.uptime_start.elapsed().as_secs_f64()
    }
}
