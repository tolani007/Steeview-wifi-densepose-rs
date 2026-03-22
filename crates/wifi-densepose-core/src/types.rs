//! Core domain types: CSI frames, pose estimates, keypoints, signals.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Primitives ────────────────────────────────────────────────────────────────

pub type FrameId  = u64;
pub type DeviceId = String;
pub type Timestamp = f64;  // Unix seconds with sub-second precision

/// Confidence score in [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Confidence(f32);

impl Confidence {
    pub fn new(v: f32) -> Option<Self> {
        if (0.0..=1.0).contains(&v) { Some(Self(v)) } else { None }
    }
    pub fn new_clamped(v: f32) -> Self { Self(v.clamp(0.0, 1.0)) }
    pub fn value(self) -> f32 { self.0 }
    pub fn is_high(self) -> bool { self.0 >= 0.7 }
}

impl Default for Confidence {
    fn default() -> Self { Self(0.0) }
}

// ── CSI types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntennaConfig {
    pub n_tx: usize,
    pub n_rx: usize,
    pub n_subcarriers: usize,
}

impl Default for AntennaConfig {
    fn default() -> Self { Self { n_tx: 1, n_rx: 4, n_subcarriers: 56 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrequencyBand {
    pub channel_number: u8,
    pub center_freq_mhz: f32,
    pub bandwidth_mhz:   f32,
}

impl FrequencyBand {
    pub fn channel_1()  -> Self { Self { channel_number: 1,  center_freq_mhz: 2412.0, bandwidth_mhz: 20.0 } }
    pub fn channel_6()  -> Self { Self { channel_number: 6,  center_freq_mhz: 2437.0, bandwidth_mhz: 20.0 } }
    pub fn channel_11() -> Self { Self { channel_number: 11, center_freq_mhz: 2462.0, bandwidth_mhz: 20.0 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsiMetadata {
    pub timestamp:     Timestamp,
    pub frame_id:      FrameId,
    pub device_id:     DeviceId,
    pub antenna:       AntennaConfig,
    pub band:          FrequencyBand,
    pub rssi_dbm:      f32,
    pub noise_floor_dbm: f32,
}

impl CsiMetadata {
    pub fn now(frame_id: FrameId, device_id: impl Into<String>) -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        Self {
            timestamp: ts,
            frame_id,
            device_id: device_id.into(),
            antenna: AntennaConfig::default(),
            band: FrequencyBand::channel_1(),
            rssi_dbm: -65.0,
            noise_floor_dbm: -90.0,
        }
    }
}

/// Raw CSI frame from a sensor node. Amplitudes are per (link, subcarrier).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsiFrame {
    pub metadata:  CsiMetadata,
    /// Shape: [n_links, n_subcarriers] — amplitude in dB
    pub amplitude: Vec<Vec<f32>>,
    /// Shape: [n_links, n_subcarriers] — phase in radians
    pub phase:     Vec<Vec<f32>>,
}

impl CsiFrame {
    pub fn n_links(&self) -> usize       { self.amplitude.len() }
    pub fn n_subcarriers(&self) -> usize { self.amplitude.first().map(|r| r.len()).unwrap_or(0) }
}

// ── Signal types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalFeatures {
    pub mean_amplitude: f32,
    pub std_amplitude:  f32,
    pub mean_phase:     f32,
    pub std_phase:      f32,
    pub doppler_energy: f32,
    pub psd_peak_freq:  f32,
    pub breathing_hz:   f32,
    pub heartrate_hz:   f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedSignal {
    pub frame_id:  FrameId,
    pub timestamp: Timestamp,
    pub features:  SignalFeatures,
    /// Per-link RMS amplitude
    pub rms_per_link: Vec<f32>,
    /// Motion energy (0..1)
    pub motion_energy: f32,
    /// Human presence detected
    pub presence: bool,
    /// Presence confidence
    pub confidence: Confidence,
}

// ── Pose types ────────────────────────────────────────────────────────────────

/// COCO 17-keypoint skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum KeypointType {
    Nose = 0, LeftEye, RightEye, LeftEar, RightEar,
    LeftShoulder, RightShoulder, LeftElbow, RightElbow,
    LeftWrist, RightWrist, LeftHip, RightHip,
    LeftKnee, RightKnee, LeftAnkle, RightAnkle,
}

impl KeypointType {
    pub fn all() -> [Self; 17] {
        use KeypointType::*;
        [Nose, LeftEye, RightEye, LeftEar, RightEar,
         LeftShoulder, RightShoulder, LeftElbow, RightElbow,
         LeftWrist, RightWrist, LeftHip, RightHip,
         LeftKnee, RightKnee, LeftAnkle, RightAnkle]
    }

    pub fn skeleton_edges() -> &'static [(u8, u8)] {
        &[
            (0,1),(0,2),(1,3),(2,4),
            (5,6),(5,7),(7,9),(6,8),(8,10),
            (5,11),(6,12),(11,12),
            (11,13),(13,15),(12,14),(14,16),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keypoint {
    pub kp_type:    KeypointType,
    /// Normalized [0,1] along room width
    pub x:          f32,
    /// Normalized [0,1] along room height  
    pub y:          f32,
    pub confidence: Confidence,
}

impl Keypoint {
    pub fn new(kp_type: KeypointType, x: f32, y: f32, confidence: Confidence) -> Self {
        Self { kp_type, x, y, confidence }
    }
    pub fn is_visible(&self) -> bool {
        self.confidence.value() >= crate::DEFAULT_CONFIDENCE_THRESHOLD
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x:      f32,
    pub y:      f32,
    pub width:  f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonPose {
    pub person_id:      u32,
    pub keypoints:      Vec<Keypoint>,
    pub bounding_box:   BoundingBox,
    pub overall_confidence: Confidence,
    pub breathing_bpm:  f32,
    pub heart_rate_bpm: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoseEstimate {
    pub frame_id:   FrameId,
    pub timestamp:  Timestamp,
    pub persons:    Vec<PersonPose>,
    pub room_width_m:  f32,
    pub room_height_m: f32,
}

impl PoseEstimate {
    pub fn n_persons(&self) -> usize { self.persons.len() }
}
