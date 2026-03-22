//! Database layer — SQLite (dev) / PostgreSQL (prod) via SQLx.
//! Stores CSI frames, pose events, and vital sign time series.

use wifi_densepose_core::{CsiFrame, PoseEstimate, error::{CoreResult, StorageError}};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// In-memory store (used when no DB URL configured or for testing)
// ---------------------------------------------------------------------------

use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

const DEFAULT_MAX_FRAMES: usize = 100_000;

/// Lightweight in-memory frame store.
#[derive(Debug, Clone)]
pub struct InMemoryStore {
    frames:     Arc<Mutex<VecDeque<CsiFrame>>>,
    poses:      Arc<Mutex<VecDeque<PoseEstimate>>>,
    max_frames: usize,
}

impl InMemoryStore {
    pub fn new(max_frames: usize) -> Self {
        Self {
            frames:     Arc::new(Mutex::new(VecDeque::new())),
            poses:      Arc::new(Mutex::new(VecDeque::new())),
            max_frames,
        }
    }

    pub fn store_frame(&self, frame: CsiFrame) -> CoreResult<()> {
        let mut q = self.frames.lock().unwrap();
        if q.len() >= self.max_frames { q.pop_front(); }
        q.push_back(frame);
        Ok(())
    }

    pub fn store_pose(&self, pose: PoseEstimate) -> CoreResult<()> {
        let mut q = self.poses.lock().unwrap();
        if q.len() >= self.max_frames { q.pop_front(); }
        q.push_back(pose);
        Ok(())
    }

    pub fn frame_count(&self) -> u64 {
        self.frames.lock().unwrap().len() as u64
    }

    pub fn latest_frame(&self) -> Option<CsiFrame> {
        self.frames.lock().unwrap().back().cloned()
    }

    pub fn latest_pose(&self) -> Option<PoseEstimate> {
        self.poses.lock().unwrap().back().cloned()
    }

    pub fn pose_count(&self) -> u64 {
        self.poses.lock().unwrap().len() as u64
    }
}

impl Default for InMemoryStore {
    fn default() -> Self { Self::new(DEFAULT_MAX_FRAMES) }
}

// ---------------------------------------------------------------------------
// Vital sign record (for time-series storage)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VitalSignRecord {
    pub timestamp:      f64,
    pub frame_id:       u64,
    pub person_id:      u32,
    pub breathing_bpm:  f32,
    pub heart_rate_bpm: f32,
    pub confidence:     f32,
    pub presence_score: f32,
}

/// Rolling buffer for vital sign records.
pub struct VitalSignStore {
    records: Arc<Mutex<VecDeque<VitalSignRecord>>>,
    max_len: usize,
}

impl VitalSignStore {
    pub fn new(max_len: usize) -> Self {
        Self { records: Arc::new(Mutex::new(VecDeque::new())), max_len }
    }

    pub fn push(&self, record: VitalSignRecord) {
        let mut q = self.records.lock().unwrap();
        if q.len() >= self.max_len { q.pop_front(); }
        q.push_back(record);
    }

    pub fn latest_n(&self, n: usize) -> Vec<VitalSignRecord> {
        let q = self.records.lock().unwrap();
        q.iter().rev().take(n).cloned().collect::<Vec<_>>().into_iter().rev().collect()
    }

    pub fn latest(&self) -> Option<VitalSignRecord> {
        self.records.lock().unwrap().back().cloned()
    }
}

impl Default for VitalSignStore {
    fn default() -> Self { Self::new(10_000) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wifi_densepose_core::{CsiMetadata, AntennaConfig, FrequencyBand};

    fn dummy_frame(id: u64) -> CsiFrame {
        CsiFrame {
            metadata: CsiMetadata::now(id, "test"),
            amplitude: vec![vec![0.5f32; 8]; 4],
            phase:     vec![vec![0.0f32; 8]; 4],
        }
    }

    #[test]
    fn test_store_and_retrieve_frame() {
        let store = InMemoryStore::new(100);
        store.store_frame(dummy_frame(1)).unwrap();
        assert_eq!(store.frame_count(), 1);
        let f = store.latest_frame().unwrap();
        assert_eq!(f.metadata.frame_id, 1);
    }

    #[test]
    fn test_max_frames_enforced() {
        let store = InMemoryStore::new(3);
        for i in 0..5 { store.store_frame(dummy_frame(i)).unwrap(); }
        assert_eq!(store.frame_count(), 3);
        let latest = store.latest_frame().unwrap();
        assert_eq!(latest.metadata.frame_id, 4);
    }

    #[test]
    fn test_vitals_rolling() {
        let vs = VitalSignStore::new(3);
        for i in 0..5u32 {
            vs.push(VitalSignRecord {
                timestamp: i as f64, frame_id: i as u64, person_id: 0,
                breathing_bpm: 15.0, heart_rate_bpm: 70.0, confidence: 0.9, presence_score: 0.95,
            });
        }
        let records = vs.latest_n(3);
        assert_eq!(records.len(), 3);
        assert_eq!(records.last().unwrap().frame_id, 4);
    }
}
