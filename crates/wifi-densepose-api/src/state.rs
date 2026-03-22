//! Shared API application state.

use std::sync::Arc;
use tokio::sync::RwLock;
use wifi_densepose_core::{CsiFrame, PoseEstimate};
use wifi_densepose_db::{InMemoryStore, VitalSignStore};
use wifi_densepose_hardware::SimulatedAdapter;
use wifi_densepose_nn::PoseEstimator;

/// Thread-safe shared state for all API handlers.
#[derive(Clone)]
pub struct AppState {
    pub frames:         Arc<InMemoryStore>,
    pub vitals:         Arc<VitalSignStore>,
    pub latest_pose:    Arc<RwLock<Option<PoseEstimate>>>,
    pub estimator:      Arc<PoseEstimator>,
    pub version:        String,
    pub uptime_start:   std::time::Instant,
    pub hardware_mode:  String,
}

impl AppState {
    pub fn new_simulated(n_persons: usize) -> Self {
        use wifi_densepose_nn::PoseInferenceConfig;
        Self {
            frames:        Arc::new(InMemoryStore::new(50_000)),
            vitals:        Arc::new(VitalSignStore::new(10_000)),
            latest_pose:   Arc::new(RwLock::new(None)),
            estimator:     Arc::new(PoseEstimator::new(PoseInferenceConfig::default())),
            version:       "0.3.0".into(),
            uptime_start:  std::time::Instant::now(),
            hardware_mode: "simulation".into(),
        }
    }

    pub fn uptime_secs(&self) -> f64 {
        self.uptime_start.elapsed().as_secs_f64()
    }
}
