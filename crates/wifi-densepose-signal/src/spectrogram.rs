//! STFT spectrogram for vital sign visualization.

use rustfft::{FftPlanner, num_complex::Complex};

/// Compute a Short-Time Fourier Transform spectrogram.
///
/// Returns a 2-D matrix: rows = time frames, cols = frequency bins.
pub fn stft(
    signal: &[f32],
    window_size: usize,
    hop_size: usize,
) -> Vec<Vec<f32>> {
    if signal.is_empty() || window_size == 0 { return vec![]; }

    // Hanning window
    let window: Vec<f32> = (0..window_size)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (window_size - 1) as f32).cos()))
        .collect();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(window_size);
    let half = window_size / 2;

    let mut frames = Vec::new();
    let mut pos = 0usize;

    while pos + window_size <= signal.len() {
        let mut buf: Vec<Complex<f32>> = signal[pos..pos + window_size]
            .iter()
            .zip(window.iter())
            .map(|(&s, &w)| Complex::new(s * w, 0.0))
            .collect();

        fft.process(&mut buf);

        // Single-sided magnitude
        let mag: Vec<f32> = buf[..half].iter().map(|c| c.norm()).collect();
        frames.push(mag);
        pos += hop_size;
    }

    frames
}

/// Peak frequency in a spectrogram row (Hz).
pub fn peak_freq_hz(row: &[f32], sampling_rate: f32, fft_size: usize) -> f32 {
    if row.is_empty() { return 0.0; }
    let (peak_bin, _) = row.iter()
        .enumerate()
        .fold((0usize, 0.0f32), |(bi, bv), (i, &v)| if v > bv { (i, v) } else { (bi, bv) });
    peak_bin as f32 * (sampling_rate / fft_size as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stft_produces_frames() {
        let signal: Vec<f32> = (0..256).map(|i| (i as f32 * 0.1).sin()).collect();
        let frames = stft(&signal, 64, 32);
        assert!(!frames.is_empty(), "stft should produce at least one frame");
        assert_eq!(frames[0].len(), 32, "each frame should have window/2 bins");
    }

    #[test]
    fn test_stft_empty() {
        let frames = stft(&[], 64, 32);
        assert!(frames.is_empty());
    }
}
