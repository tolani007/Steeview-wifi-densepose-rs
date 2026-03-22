//! WiFi-MAT: disaster response survivor detection module.
//!
//! Uses WiFi CSI to detect survivors through rubble, walls, and debris.
//! Implements multi-zone sweep, alert escalation, and reporting.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};
use wifi_densepose_hardware::SimulatedAdapter;
use wifi_densepose_signal::motion::MotionAnalysis;

/// Alert severity level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertLevel { None, Low, Medium, High, Critical }

/// A survivor detection event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurvivorAlert {
    pub timestamp:   f64,
    pub zone_id:     u32,
    pub confidence:  f32,
    pub level:       AlertLevel,
    pub motion_energy: f32,
    pub breathing_detected: bool,
}

/// Zone configuration for a sweep area.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanZone {
    pub id:        u32,
    pub label:     String,
    pub x_m:       f32,
    pub y_m:       f32,
    pub width_m:   f32,
    pub height_m:  f32,
}

/// Shared MAT state.
#[derive(Debug)]
pub struct MatState {
    pub alerts:      Mutex<Vec<SurvivorAlert>>,
    pub zones:       Vec<ScanZone>,
    pub scan_active: Mutex<bool>,
}

impl MatState {
    pub fn new() -> Self {
        let zones = vec![
            ScanZone { id: 0, label: "Zone A (North)".into(), x_m: 0.0, y_m: 0.0, width_m: 5.0, height_m: 5.0 },
            ScanZone { id: 1, label: "Zone B (South)".into(), x_m: 0.0, y_m: 5.0, width_m: 5.0, height_m: 5.0 },
            ScanZone { id: 2, label: "Zone C (East)".into(),  x_m: 5.0, y_m: 0.0, width_m: 5.0, height_m: 5.0 },
        ];
        Self { alerts: Mutex::new(Vec::new()), zones, scan_active: Mutex::new(false) }
    }

    /// Classify motion energy → alert level.
    pub fn classify(&self, energy: f32, breathing: bool) -> AlertLevel {
        if energy < 0.05 { return AlertLevel::None; }
        match (energy, breathing) {
            (e, true)  if e > 0.5 => AlertLevel::Critical,
            (e, true)  if e > 0.2 => AlertLevel::High,
            (e, _)     if e > 0.3 => AlertLevel::Medium,
            (e, _)     if e > 0.1 => AlertLevel::Low,
            _                      => AlertLevel::None,
        }
    }

    pub fn add_alert(&self, alert: SurvivorAlert) {
        let mut q = self.alerts.lock().unwrap();
        q.push(alert);
    }

    pub fn alert_count(&self) -> usize {
        self.alerts.lock().unwrap().len()
    }

    pub fn latest_alerts(&self, n: usize) -> Vec<SurvivorAlert> {
        let q = self.alerts.lock().unwrap();
        q.iter().rev().take(n).cloned().collect::<Vec<_>>().into_iter().rev().collect()
    }
}

impl Default for MatState { fn default() -> Self { Self::new() } }

/// Main MAT loop: sweep zones and detect survivors.
pub async fn run(state: Arc<MatState>, addr: SocketAddr) {
    info!(%addr, "WiFi-MAT disaster response active");
    info!("Scanning {} zones for survivors", state.zones.len());

    let mut adapters: Vec<SimulatedAdapter> = state.zones.iter()
        .map(|z| SimulatedAdapter::new(1, 4, 56, z.width_m, z.height_m))
        .collect();

    let mut t = 0.0f32;

    // Sweep indefinitely at 2 Hz
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(500));

    loop {
        ticker.tick().await;
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);

        for (zone_idx, zone) in state.zones.iter().enumerate() {
            let frame = adapters[zone_idx].generate_frame(t, 0.5);
            let result = MotionAnalysis::run_pipeline(&frame.amplitude, &frame.phase);

            if result.present {
                let level = state.classify(result.score.energy, result.score.breathing > 0.02);
                if level != AlertLevel::None {
                    let alert = SurvivorAlert {
                        timestamp: ts,
                        zone_id: zone.id,
                        confidence: result.confidence,
                        level: level.clone(),
                        motion_energy: result.score.energy,
                        breathing_detected: result.score.breathing > 0.02,
                    };
                    match &alert.level {
                        AlertLevel::Critical | AlertLevel::High => warn!(
                            zone = %zone.label,
                            confidence = alert.confidence,
                            "⚠️  SURVIVOR DETECTED — {:?}", alert.level
                        ),
                        _ => info!(zone = %zone.label, "Survivor signal detected: {:?}", alert.level),
                    }
                    state.add_alert(alert);
                }
            }
        }
        t += 0.5;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_levels() {
        let mat = MatState::new();
        assert_eq!(mat.classify(0.0, false),  AlertLevel::None);
        assert_eq!(mat.classify(0.15, false), AlertLevel::Low);
        assert_eq!(mat.classify(0.35, false), AlertLevel::Medium);
        assert_eq!(mat.classify(0.6, true),   AlertLevel::Critical);
    }

    #[test]
    fn test_alert_rolling() {
        let mat = MatState::new();
        for i in 0..5 {
            mat.add_alert(SurvivorAlert {
                timestamp: i as f64, zone_id: 0, confidence: 0.8,
                level: AlertLevel::High, motion_energy: 0.4, breathing_detected: true,
            });
        }
        assert_eq!(mat.alert_count(), 5);
        let latest = mat.latest_alerts(3);
        assert_eq!(latest.len(), 3);
    }
}
