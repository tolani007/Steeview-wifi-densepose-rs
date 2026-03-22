//! Simulated CSI hardware adapter — deterministic physics-based signal generator.
//! Produces realistic CSI frames matching the Python simulator's output.

use std::f32::consts::PI;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;
use wifi_densepose_core::{AntennaConfig, CsiFrame, CsiMetadata, FrequencyBand};

/// A single simulated person with random-walk position and vital signs.
struct SimPerson {
    id:         u32,
    x:          f32,
    y:          f32,
    vx:         f32,
    vy:         f32,
    br_hz:      f32, // breathing rate (Hz)
    hr_hz:      f32, // heart rate (Hz)
    br_phase:   f32,
    hr_phase:   f32,
    room_w:     f32,
    room_h:     f32,
}

impl SimPerson {
    fn new(id: u32, room_w: f32, room_h: f32, seed: u32) -> Self {
        let s = seed as f32;
        Self {
            id,
            x:         1.0 + (s * 0.37).sin().abs() * (room_w - 2.0),
            y:         1.0 + (s * 0.51).cos().abs() * (room_h - 2.0),
            vx:        0.03 * (s * 0.13).sin(),
            vy:        0.03 * (s * 0.17).cos(),
            br_hz:     0.2  + (s * 0.07).sin().abs() * 0.13,  // 12–20 bpm
            hr_hz:     0.97 + (s * 0.11).cos().abs() * 0.37,  // 58–80 bpm
            br_phase:  (s * 1.3) % (2.0 * PI),
            hr_phase:  (s * 2.1) % (2.0 * PI),
            room_w,
            room_h,
        }
    }

    fn step(&mut self, dt: f32) {
        // Random walk (deterministic drift)
        let noise_x = (self.x * 7.3 + self.y * 3.1).sin() * 0.005;
        let noise_y = (self.x * 5.7 + self.y * 8.9).cos() * 0.005;
        self.vx = (self.vx + noise_x).clamp(-0.15, 0.15);
        self.vy = (self.vy + noise_y).clamp(-0.15, 0.15);
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        // Bounce
        if self.x < 0.5 || self.x > self.room_w - 0.5 { self.vx *= -1.0; self.x = self.x.clamp(0.5, self.room_w - 0.5); }
        if self.y < 0.5 || self.y > self.room_h - 0.5 { self.vy *= -1.0; self.y = self.y.clamp(0.5, self.room_h - 0.5); }
        // Drift vitals slowly
        self.br_hz = (self.br_hz + (self.x * 0.001).sin() * 0.0001).clamp(0.2, 0.33);
        self.hr_hz = (self.hr_hz + (self.y * 0.001).cos() * 0.0002).clamp(0.92, 1.5);
    }
}

/// Simulated hardware adapter — generates CSI frames without real hardware.
pub struct SimulatedAdapter {
    n_nodes:       usize,
    n_subcarriers: usize,
    room_w:        f32,
    room_h:        f32,
    persons:       Vec<SimPerson>,
    frame_count:   Arc<AtomicU64>,
}

impl SimulatedAdapter {
    pub fn new(n_persons: usize, n_nodes: usize, n_subcarriers: usize,
               room_w: f32, room_h: f32) -> Self {
        let persons = (0..n_persons as u32)
            .map(|i| SimPerson::new(i, room_w, room_h, i * 7 + 3))
            .collect();
        info!(n_persons, n_nodes, n_subcarriers, "SimulatedAdapter initialized");
        Self { n_nodes, n_subcarriers, room_w, room_h, persons, frame_count: Arc::new(AtomicU64::new(0)) }
    }

    /// Generate one CSI frame with optional time advance.
    pub fn generate_frame(&mut self, t: f32, dt: f32) -> CsiFrame {
        let n_links = self.n_nodes * (self.n_nodes - 1); // directed links

        for p in &mut self.persons { p.step(dt); }

        let mut amplitude = vec![vec![0.0f32; self.n_subcarriers]; n_links];
        let mut phase     = vec![vec![0.0f32; self.n_subcarriers]; n_links];

        // Background RF environment signature (deterministic, seeded)
        for link in 0..n_links {
            for sc in 0..self.n_subcarriers {
                let bg = 0.3 + 0.4 * ((link * 17 + sc * 5) as f32 * 0.031).sin().abs();
                amplitude[link][sc] = bg;
                phase[link][sc]     = (link as f32 * 0.37 + sc as f32 * 0.08 + t * 0.1) % (2.0 * PI);
            }
        }

        // Person-induced disturbances
        for p in &self.persons {
            let dist = ((p.x - self.room_w / 2.0).powi(2) + (p.y - self.room_h / 2.0).powi(2)).sqrt();
            let fresnel_scale = 1.0 / (1.0 + dist);

            for link in 0..n_links {
                let link_off = link as f32 * 0.37 + p.id as f32 * 1.1;
                for sc in 0..self.n_subcarriers {
                    let sc_off = sc as f32 * 0.08;
                    let breath = 0.06 * fresnel_scale * (2.0 * PI * p.br_hz * t + p.br_phase + sc_off + link_off).sin();
                    let heart  = 0.018 * fresnel_scale * (2.0 * PI * p.hr_hz * t + p.hr_phase + sc_off * 2.0 + link_off).sin();
                    let scatter = 0.04 * fresnel_scale * (2.0 * PI * 0.5 * t + link_off + sc_off).sin();
                    amplitude[link][sc] = (amplitude[link][sc] + breath + heart + scatter).clamp(0.0, 1.0);
                }
            }
        }

        let frame_id = self.frame_count.fetch_add(1, Ordering::Relaxed);
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);

        CsiFrame {
            metadata: CsiMetadata {
                timestamp: ts,
                frame_id,
                device_id: "sim-node-0".into(),
                antenna: AntennaConfig { n_tx: 1, n_rx: self.n_nodes, n_subcarriers: self.n_subcarriers },
                band: FrequencyBand::channel_1(),
                rssi_dbm: -58.0,
                noise_floor_dbm: -90.0,
            },
            amplitude,
            phase,
        }
    }

    pub fn frame_count(&self) -> u64 { self.frame_count.load(Ordering::Relaxed) }
    pub fn n_persons(&self) -> usize { self.persons.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_frame_dimensions() {
        let mut adapter = SimulatedAdapter::new(2, 4, 56, 5.0, 4.0);
        let frame = adapter.generate_frame(0.0, 0.1);
        assert_eq!(frame.amplitude.len(), 12); // 4*3 links
        assert_eq!(frame.amplitude[0].len(), 56);
        assert_eq!(frame.phase.len(), 12);
    }

    #[test]
    fn test_frame_count_increments() {
        let mut adapter = SimulatedAdapter::new(1, 4, 56, 5.0, 4.0);
        adapter.generate_frame(0.0, 0.1);
        adapter.generate_frame(0.1, 0.1);
        assert_eq!(adapter.frame_count(), 2);
    }
}
