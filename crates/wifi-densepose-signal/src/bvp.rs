//! Bandpass vital processor (BVP): extract breathing and heart rate from CSI.

use std::f32::consts::PI;

/// Butterworth-style IIR bandpass filter (2nd order biquad).
#[derive(Debug, Clone)]
pub struct BandpassFilter {
    // Biquad coefficients
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    // State
    x1: f32, x2: f32,
    y1: f32, y2: f32,
}

impl BandpassFilter {
    /// Design a simple bandpass filter for [lo_hz, hi_hz] at sample_rate_hz.
    pub fn new(lo_hz: f32, hi_hz: f32, sample_rate_hz: f32) -> Self {
        let w0 = 2.0 * PI * ((lo_hz * hi_hz).sqrt()) / sample_rate_hz;
        let bw = 2.0 * PI * (hi_hz - lo_hz) / sample_rate_hz;
        let q = w0 / bw.max(0.001);

        // RBJ bandpass (constant skirt gain)
        let sin_w0 = w0.sin();
        let cos_w0 = w0.cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 =  sin_w0 / 2.0;
        let b1 =  0.0;
        let b2 = -sin_w0 / 2.0;
        let a0 =  1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 =  1.0 - alpha;

        Self {
            b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
            a1: a1 / a0, a2: a2 / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    /// Filter a single sample (stateful).
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
               - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1; self.x1 = x;
        self.y2 = self.y1; self.y1 = y;
        y
    }

    /// Filter a signal buffer in place.
    pub fn apply(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() { *s = self.process(*s); }
    }
}

/// Vital sign extractor: breathing rate and heart rate from CSI amplitude.
pub struct BvpExtractor {
    pub sample_rate_hz: f32,
}

impl BvpExtractor {
    pub fn new(sample_rate_hz: f32) -> Self { Self { sample_rate_hz } }

    /// Extract breathing rate (bpm) from a CSI amplitude time series.
    pub fn breathing_rate_bpm(&self, signal: &[f32]) -> f32 {
        self.dominant_freq_hz(signal, 0.1, 0.5) * 60.0
    }

    /// Extract heart rate (bpm).
    pub fn heart_rate_bpm(&self, signal: &[f32]) -> f32 {
        self.dominant_freq_hz(signal, 0.8, 2.0) * 60.0
    }

    /// Find dominant frequency in [lo, hi] Hz using a narrow bandpass + zero-crossing.
    fn dominant_freq_hz(&self, signal: &[f32], lo_hz: f32, hi_hz: f32) -> f32 {
        if signal.len() < 4 { return 0.0; }
        let mut filtered = signal.to_vec();
        let mut bp = BandpassFilter::new(lo_hz, hi_hz, self.sample_rate_hz);
        bp.apply(&mut filtered);

        // Count zero crossings (upward) → frequency estimate
        let crossings: usize = filtered.windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();

        let duration_s = signal.len() as f32 / self.sample_rate_hz;
        if duration_s < 0.1 { return lo_hz; }
        (crossings as f32 / duration_s).clamp(lo_hz, hi_hz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine_wave(freq_hz: f32, n: usize, fs: f32) -> Vec<f32> {
        (0..n).map(|i| (2.0 * PI * freq_hz * i as f32 / fs).sin()).collect()
    }

    #[test]
    fn test_bandpass_passes_center() {
        let mut f = BandpassFilter::new(0.2, 0.4, 100.0);
        let mut sig = sine_wave(0.3, 500, 100.0);
        let orig_rms = (sig.iter().map(|x| x * x).sum::<f32>() / sig.len() as f32).sqrt();
        f.apply(&mut sig);
        // Warmup affects start — check only latter half
        let out_rms = (sig[250..].iter().map(|x| x * x).sum::<f32>() / 250.0).sqrt();
        assert!(out_rms > orig_rms * 0.3, "center freq should pass: out_rms={out_rms}");
    }

    #[test]
    fn test_breathing_rate() {
        let fs = 100.0;
        let bvp = BvpExtractor::new(fs);
        // 0.25 Hz breathing ≈ 15 bpm
        let sig = sine_wave(0.25, 800, fs);
        let bpm = bvp.breathing_rate_bpm(&sig);
        assert!(bpm > 5.0 && bpm < 35.0, "breathing bpm out of range: {bpm}");
    }
}
