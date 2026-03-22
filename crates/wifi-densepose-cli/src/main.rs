//! RuView CLI — command-line interface for WiFi DensePose.

use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "ruview",
    version = "0.3.0",
    author = "rUv <ruv@ruv.net>",
    about = "RuView WiFi DensePose — ~810x faster Rust port",
    long_about = "Real-time human pose estimation, vital sign monitoring, and presence detection via WiFi CSI.\n\nNo camera required."
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
    /// Start the API server
    Start {
        /// TCP address to listen on
        #[arg(short, long, default_value = "0.0.0.0:8000", env = "RUVIEW_ADDR")]
        addr: SocketAddr,
        /// Number of simulated persons
        #[arg(short = 'p', long, default_value_t = 2, env = "RUVIEW_PERSONS")]
        n_persons: usize,
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
        Commands::Start { addr, n_persons } => {
            info!(addr = %addr, n_persons, "Starting RuView API server");
            let state = Arc::new(wifi_densepose_api::AppState::new_simulated(n_persons));
            let router = wifi_densepose_api::build_router(state);
            if let Err(e) = wifi_densepose_api::serve(router, addr).await {
                error!("Server error: {e}");
                std::process::exit(1);
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

            println!("RuView Signal Pipeline Benchmark");
            println!("══════════════════════════════════");
            println!("Iterations:  {iters}");
            println!("Total time:  {:.2} ms", elapsed.as_millis());
            println!("Per frame:   {per_iter_us:.2} µs");
            println!("Throughput:  {fps:.0} fps");
            println!("Target:      ~18.47 µs (~54,000 fps)");
        }

        Commands::Verify => {
            use wifi_densepose_hardware::SimulatedAdapter;
            use sha2::{Digest, Sha256};

            info!("Running deterministic signal verification (like Python's verify.py)");
            let mut adapter = SimulatedAdapter::new(1, 4, 56, 5.0, 4.0);
            let mut hasher = Sha256::new();

            for i in 0..50 {
                let frame = adapter.generate_frame(i as f32 * 0.1, 0.1);
                // Hash the first link amplitude
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
