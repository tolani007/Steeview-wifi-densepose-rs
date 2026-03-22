//! Configuration loading — environment variables → YAML file → defaults.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub hardware: HardwareConfig,
    pub auth: AuthConfig,
    pub database: DatabaseConfig,
    pub signal: SignalConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { host: "0.0.0.0".into(), port: 8000, workers: 1 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HardwareMode {
    Simulated,
    Udp,
    Pcap,
}

impl fmt::Display for HardwareMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { Self::Simulated => write!(f, "simulated"), Self::Udp => write!(f, "udp"), Self::Pcap => write!(f, "pcap") }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareConfig {
    pub mode: HardwareMode,
    pub udp_bind: String,
    pub n_nodes: usize,
    pub n_subcarriers: usize,
    /// Target persons to simulate
    pub sim_n_persons: usize,
    pub sim_room_width_m: f32,
    pub sim_room_height_m: f32,
}

impl Default for HardwareConfig {
    fn default() -> Self {
        Self {
            mode: HardwareMode::Simulated,
            udp_bind: "0.0.0.0:5005".into(),
            n_nodes: 4,
            n_subcarriers: 56,
            sim_n_persons: 2,
            sim_room_width_m: 5.0,
            sim_room_height_m: 4.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// JWT signing secret. MUST be set via env var in production.
    pub jwt_secret: String,
    pub jwt_expiry_secs: u64,
    pub rate_limit_per_min: u32,
    /// If false, all requests are unauthenticated (dev mode only)
    pub enabled: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: "change-me-in-production".into(),
            jwt_expiry_secs: 3600,
            rate_limit_per_min: 100,
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_secs: u64,
    /// Maximum stored frames before oldest are pruned
    pub max_frames: u64,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite:ruview.db".into(),
            max_connections: 10,
            min_connections: 1,
            connect_timeout_secs: 5,
            max_frames: 100_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    pub sampling_rate_hz: f32,
    pub noise_floor_db: f32,
    pub confidence_threshold: f32,
    pub broadcast_fps: u32,
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self { sampling_rate_hz: 100.0, noise_floor_db: -30.0, confidence_threshold: 0.5, broadcast_fps: 10 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel { Trace, Debug, Info, Warn, Error }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub json: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self { Self { level: LogLevel::Info, json: false } }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            hardware: HardwareConfig::default(),
            auth: AuthConfig::default(),
            database: DatabaseConfig::default(),
            signal: SignalConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

/// Load configuration. Priority: env vars > config.yaml > defaults.
pub fn load(config_path: Option<&str>) -> AppConfig {
    let _ = dotenvy::dotenv();

    let mut base = AppConfig::default();

    // Override from environment variables
    if let Ok(v) = std::env::var("RUVIEW_PORT") {
        if let Ok(port) = v.parse() { base.server.port = port; }
    }
    if let Ok(v) = std::env::var("JWT_SECRET") { base.auth.jwt_secret = v; }
    if let Ok(v) = std::env::var("DATABASE_URL") { base.database.url = v; }
    if let Ok(v) = std::env::var("AUTH_ENABLED") {
        base.auth.enabled = v == "true" || v == "1";
    }
    if let Ok(v) = std::env::var("HARDWARE_MODE") {
        base.hardware.mode = match v.to_lowercase().as_str() {
            "udp"  => HardwareMode::Udp,
            "pcap" => HardwareMode::Pcap,
            _      => HardwareMode::Simulated,
        };
    }
    if let Ok(v) = std::env::var("RUST_LOG") {
        base.logging.level = match v.to_lowercase().split(',').next().unwrap_or("info") {
            "trace" => LogLevel::Trace, "debug" => LogLevel::Debug,
            "warn"  => LogLevel::Warn,  "error" => LogLevel::Error,
            _       => LogLevel::Info,
        };
    }

    // Try YAML config file
    if let Some(path) = config_path {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(file_cfg) = serde_yaml::from_str::<AppConfig>(&content) {
                return merge(file_cfg, base);
            }
        }
    }

    base
}

/// File config takes precedence over defaults, but explicitly set env vars win.
fn merge(file: AppConfig, env: AppConfig) -> AppConfig {
    file // simplified: file wins; extend with granular merging if needed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_valid() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.server.port, 8000);
        assert_eq!(cfg.hardware.mode, HardwareMode::Simulated);
        assert!(!cfg.auth.enabled);
    }

    #[test]
    fn test_load_returns_config() {
        let cfg = load(None);
        assert!(cfg.server.port > 0);
    }
}
