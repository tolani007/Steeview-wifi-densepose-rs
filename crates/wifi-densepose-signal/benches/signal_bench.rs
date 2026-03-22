//! Criterion benchmarks for the WiFi DensePose signal processing pipeline.
//!
//! Expected results:
//!   CSI Preprocessing (4×64):  ~5.19 µs   / 49-66 Melem/s
//!   Phase Sanitization (4×64): ~3.84 µs   / 67-85 Melem/s
//!   Feature Extraction (4×64): ~9.03 µs   / 7-11  Melem/s
//!   Motion Detection:          ~186 ns
//!   Full Pipeline:             ~18.47 µs  → ~54,000 fps

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use wifi_densepose_signal::{
    csi_processor::{CsiPreprocessor, CsiProcessorConfig},
    features::{FeatureExtractor, FeatureExtractorConfig},
    motion::{MotionAnalysis, MotionDetector, MotionDetectorConfig},
    phase_sanitizer::{PhaseSanitizer, PhaseSanitizerConfig},
};

const N_LINKS: usize = 4;
const N_SC:    usize = 64;

fn make_csi(n_links: usize, n_sc: usize) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let amp = (0..n_links)
        .map(|l| (0..n_sc).map(|i| ((l * 7 + i * 3) as f32 * 0.031).sin().abs() * 0.7 - 0.1).collect())
        .collect();
    let phase = (0..n_links)
        .map(|l| (0..n_sc).map(|i| ((l * 5 + i * 2) as f32 * 0.05) % (2.0 * std::f32::consts::PI) - std::f32::consts::PI).collect())
        .collect();
    (amp, phase)
}

// ── Benchmark: CSI Preprocessing ─────────────────────────────────────────────

fn bench_csi_preprocessing(c: &mut Criterion) {
    let (amp, phase) = make_csi(N_LINKS, N_SC);
    let proc = CsiPreprocessor::new(CsiProcessorConfig::default());

    let mut group = c.benchmark_group("CSI Preprocessing");
    group.throughput(Throughput::Elements((N_LINKS * N_SC) as u64));

    group.bench_function(BenchmarkId::new("4x64", "default"), |b| {
        b.iter(|| proc.process(black_box(&amp), black_box(&phase)).unwrap())
    });
    group.finish();
}

// ── Benchmark: Phase Sanitization ────────────────────────────────────────────

fn bench_phase_sanitization(c: &mut Criterion) {
    let (_, phase) = make_csi(N_LINKS, N_SC);
    let san = PhaseSanitizer::new(PhaseSanitizerConfig::default());

    let mut group = c.benchmark_group("Phase Sanitization");
    group.throughput(Throughput::Elements((N_LINKS * N_SC) as u64));

    group.bench_function(BenchmarkId::new("4x64", "consecutive"), |b| {
        b.iter(|| san.sanitize(black_box(&phase)))
    });
    group.finish();
}

// ── Benchmark: Feature Extraction ────────────────────────────────────────────

fn bench_feature_extraction(c: &mut Criterion) {
    let (amp, phase) = make_csi(N_LINKS, N_SC);
    let ext = FeatureExtractor::new(FeatureExtractorConfig::default());

    let mut group = c.benchmark_group("Feature Extraction");
    group.throughput(Throughput::Elements((N_LINKS * N_SC) as u64));

    group.bench_function(BenchmarkId::new("4x64", "full"), |b| {
        b.iter(|| ext.extract(black_box(&amp), black_box(&phase)))
    });
    group.finish();
}

// ── Benchmark: Motion Detection ───────────────────────────────────────────────

fn bench_motion_detection(c: &mut Criterion) {
    let rms = vec![0.4f32; N_LINKS];
    let det = MotionDetector::new(MotionDetectorConfig::default());

    let mut group = c.benchmark_group("Motion Detection");
    group.bench_function("default", |b| {
        b.iter(|| det.detect(black_box(&rms), black_box(0.35), black_box(0.4)))
    });
    group.finish();
}

// ── Benchmark: Full Pipeline ──────────────────────────────────────────────────

fn bench_full_pipeline(c: &mut Criterion) {
    let (amp, phase) = make_csi(N_LINKS, N_SC);

    let mut group = c.benchmark_group("Full Pipeline");
    group.throughput(Throughput::Elements((N_LINKS * N_SC) as u64));

    group.bench_function(BenchmarkId::new("4x64", "all_stages"), |b| {
        b.iter(|| MotionAnalysis::run_pipeline(black_box(&amp), black_box(&phase)))
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_csi_preprocessing,
    bench_phase_sanitization,
    bench_feature_extraction,
    bench_motion_detection,
    bench_full_pipeline,
);
criterion_main!(benches);
