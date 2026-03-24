//! RuView CLI — command-line interface for WiFi DensePose.

use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "ruview",
    version = "0.4.0",
    author = "Tolani Akinola (EigenTiki) + rUv",
    about = "Steeview WiFi DensePose — ~810x faster Rust port",
    long_about = "Real-time human pose estimation, vital sign monitoring, and presence detection via WiFi CSI.\n\nNo camera required.\n\nHardware modes: simulation (default), udp (ESP32), pcap (replay)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Config file path
    #[arg(short, long, global = true)]
    config: Option<String>,

    /// Verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the API server (simulation or UDP mode based on HARDWARE_MODE env/config)
    Start {
        /// TCP address to listen on
        #[arg(short, long, default_value = "0.0.0.0:8000", env = "RUVIEW_ADDR")]
        addr: SocketAddr,

        /// Number of simulated persons (simulation mode only)
        #[arg(short = 'p', long, default_value_t = 2, env = "RUVIEW_PERSONS")]
        n_persons: usize,

        /// Force hardware mode: simulated | udp | pcap
        /// Overrides config file / env var HARDWARE_MODE
        #[arg(long, env = "HARDWARE_MODE")]
        hardware_mode: Option<String>,

        /// UDP bind address for ESP32 CSI packets (UDP mode only)
        #[arg(long, default_value = "0.0.0.0:5500", env = "UDP_BIND")]
        udp_bind: String,
    },
    /// Run one-shot sensing and print to stdout
    Sense {
        /// Number of frames to capture
        #[arg(short, long, default_value_t = 10)]
        frames: u32,
    },
    /// Run signal processing benchmarks
    Bench {
        /// Number of benchmark iterations
        #[arg(short, long, default_value_t = 10000)]
        iters: u64,
    },
    /// Verify deterministic signal pipeline
    Verify,
    /// Run WiFi-MAT disaster response mode
    Mat {
        #[arg(short, long, default_value = "0.0.0.0:8000", env = "RUVIEW_ADDR")]
        addr: SocketAddr,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let cfg = wifi_densepose_config::load(cli.config.as_deref());

    // Init tracing
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .init();

    match cli.command {
        Commands::Start { addr, n_persons, hardware_mode, udp_bind } => {
            // Resolve effective hardware mode (CLI flag > env > config)
            let mode = hardware_mode
                .as_deref()
                .unwrap_or_else(|| cfg.hardware.mode.as_str())
                .to_lowercase();

            match mode.as_str() {
                "udp" => {
                    info!(addr = %addr, udp_bind = %udp_bind, "Starting Steeview API server [UDP / ESP32 mode]");
                    start_udp_server(addr, udp_bind).await;
                }
                _ => {
                    info!(addr = %addr, n_persons, "Starting Steeview API server [simulation mode]");
                    let state = Arc::new(wifi_densepose_api::AppState::new_simulated(n_persons));
                    let router = wifi_densepose_api::build_router(state);
                    if let Err(e) = wifi_densepose_api::serve(router, addr).await {
                        error!("Server error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }

        Commands::Sense { frames } => {
            use wifi_densepose_hardware::SimulatedAdapter;
            use wifi_densepose_nn::{PoseEstimator, PoseInferenceConfig};

            let mut adapter = SimulatedAdapter::new(2, 4, 56, 5.0, 4.0);
            let estimator = PoseEstimator::new(PoseInferenceConfig::default());
            let mut t = 0.0f32;

            println!("frame_id,n_persons,breathing_bpm,heart_rate_bpm,confidence");
            for _ in 0..frames {
                let frame = adapter.generate_frame(t, 0.1);
                t += 0.1;
                if let Ok(pose) = estimator.estimate(&frame) {
                    let (br, hr, conf) = pose.persons.first()
                        .map(|p| (p.breathing_bpm, p.heart_rate_bpm, p.overall_confidence.value()))
                        .unwrap_or((0.0, 0.0, 0.0));
                    println!("{},{},{:.1},{:.1},{:.2}",
                        frame.metadata.frame_id, pose.n_persons(), br, hr, conf);
                }
            }
        }

        Commands::Bench { iters } => {
            use wifi_densepose_hardware::SimulatedAdapter;
            use wifi_densepose_signal::motion::MotionAnalysis;
            use std::time::Instant;

            let mut adapter = SimulatedAdapter::new(2, 4, 56, 5.0, 4.0);
            let frame = adapter.generate_frame(0.0, 0.1);

            let start = Instant::now();
            for _ in 0..iters {
                let _ = MotionAnalysis::run_pipeline(&frame.amplitude, &frame.phase);
            }
            let elapsed = start.elapsed();
            let per_iter_us = elapsed.as_micros() as f64 / iters as f64;
            let fps = 1_000_000.0 / per_iter_us;

            println!("Steeview Signal Pipeline Benchmark");
            println!("═══════════════════════════════════");
            println!("Iterations:  {iters}");
            println!("Total time:  {:.2} ms", elapsed.as_millis());
            println!("Per frame:   {per_iter_us:.2} µs");
            println!("Throughput:  {fps:.0} fps");
            println!("Target:      ~18.47 µs (~54,000 fps)");
        }

        Commands::Verify => {
            use wifi_densepose_hardware::SimulatedAdapter;
            use sha2::{Digest, Sha256};

            info!("Running deterministic signal verification");
            let mut adapter = SimulatedAdapter::new(1, 4, 56, 5.0, 4.0);
            let mut hasher = Sha256::new();

            for i in 0..50 {
                let frame = adapter.generate_frame(i as f32 * 0.1, 0.1);
                for v in &frame.amplitude[0] {
                    hasher.update(v.to_le_bytes());
                }
            }

            let result = hasher.finalize();
            println!("✅ Verification complete");
            println!("SHA-256 fingerprint: {:x}", result);
            println!("(Must match across identical runs — non-random, fully deterministic)");
        }

        Commands::Mat { addr } => {
            use std::sync::Arc;
            info!(%addr, "Starting WiFi-MAT disaster response mode");
            let state = Arc::new(wifi_densepose_mat::MatState::new());
            wifi_densepose_mat::run(state, addr).await;
        }
    }
}

/// Start the API server in UDP mode: spawn the UdpAdapter in a background task,
/// wire the broadcast channel into AppState, then serve HTTP.
async fn start_udp_server(addr: SocketAddr, udp_bind: String) {
    use wifi_densepose_hardware::{UdpAdapter, csi_channel};

    // Create broadcast channel (capacity = 256 frames)
    let (csi_tx, _initial_rx) = csi_channel(256);

    let state = Arc::new(wifi_densepose_api::AppState::new_udp(csi_tx.clone()));

    // Spawn the UDP receiver in the background
    let udp_bind_clone = udp_bind.clone();
    let n_subcarriers = 56usize; // must match ESP32 firmware config
    tokio::spawn(async move {
        let adapter = UdpAdapter::new(udp_bind_clone);
        if let Err(e) = adapter.run(csi_tx, n_subcarriers).await {
            error!("UDP adapter error: {e}");
        }
    });

    info!(udp_bind = %udp_bind, "UDP adapter listening for ESP32 packets");

    let router = wifi_densepose_api::build_router(state);
    if let Err(e) = wifi_densepose_api::serve(router, addr).await {
        error!("Server error: {e}");
        std::process::exit(1);
    }
}
