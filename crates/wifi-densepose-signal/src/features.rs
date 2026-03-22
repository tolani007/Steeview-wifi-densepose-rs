//! Feature extraction from processed CSI data.
//!
//! Computes amplitude, phase, correlation, Doppler, and PSD features.
//! Target latency: ~9.03 µs for a 4×64 CSI matrix.

use rustfft::{FftPlanner, num_complex::Complex};
use serde::{Deserialize, Serialize};

/// All extracted features from a single CSI frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsiFeatures {
    pub amplitude: AmplitudeFeatures,
    pub phase:     PhaseFeatures,
    pub doppler:   DopplerFeatures,
    pub psd:       PowerSpectralDensity,
    pub correlation: CorrelationFeatures,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmplitudeFeatures {
    pub mean:     f32,
    pub std:      f32,
    pub max:      f32,
    pub min:      f32,
    pub rms:      f32,
    pub kurtosis: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseFeatures {
    pub mean: f32,
    pub std:  f32,
    pub range: f32,
    /// Phase velocity (mean first-order difference)
    pub velocity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DopplerFeatures {
    /// Dominant Doppler shift frequency (Hz)
    pub peak_freq_hz: f32,
    /// Energy in motion band [0.1, 3.0] Hz
    pub motion_energy: f32,
    /// Entropy of Doppler spectrum
    pub spectral_entropy: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerSpectralDensity {
    /// Peak PSD frequency (Hz)
    pub peak_hz: f32,
    /// PSD in breathing band [0.1, 0.5] Hz
    pub breathing_band_power: f32,
    /// PSD in cardiac band [0.8, 2.0] Hz
    pub cardiac_band_power: f32,
    /// Estimated breathing rate (Hz)
    pub breathing_hz: f32,
    /// Estimated heart rate (Hz)
    pub heart_rate_hz: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationFeatures {
    /// Mean cross-link correlation coefficient
    pub mean_cross_link: f32,
    /// Temporal autocorrelation at lag 1
    pub autocorr_lag1: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureExtractorConfig {
    /// Sampling rate (Hz) — frames per second
    pub sampling_rate: f32,
    /// FFT size for PSD estimation
    pub fft_size: usize,
}

impl Default for FeatureExtractorConfig {
    fn default() -> Self {
        Self { sampling_rate: 100.0, fft_size: 256 }
    }
}

/// Thread-safe, stateless feature extractor.
pub struct FeatureExtractor {
    config: FeatureExtractorConfig,
}

impl FeatureExtractor {
    pub fn new(config: FeatureExtractorConfig) -> Self { Self { config } }

    /// Extract all features from a processed CSI matrix.
    pub fn extract(
        &self,
        amplitude: &[Vec<f32>],
        phase: &[Vec<f32>],
    ) -> CsiFeatures {
        // Flatten amplitude across all links
        let amp_flat: Vec<f32> = amplitude.iter().flat_map(|r| r.iter().cloned()).collect();
        let phase_flat: Vec<f32> = phase.iter().flat_map(|r| r.iter().cloned()).collect();

        let amp_feats  = self.amplitude_features(&amp_flat);
        let phase_feats = self.phase_features(&phase_flat);
        let doppler    = self.doppler_features(&amp_flat);
        let psd        = self.psd_features(&amp_flat);
        let corr       = self.correlation_features(amplitude);

        CsiFeatures { amplitude: amp_feats, phase: phase_feats, doppler, psd, correlation: corr }
    }

    fn amplitude_features(&self, data: &[f32]) -> AmplitudeFeatures {
        if data.is_empty() {
            return AmplitudeFeatures { mean: 0.0, std: 0.0, max: 0.0, min: 0.0, rms: 0.0, kurtosis: 0.0 };
        }
        let n = data.len() as f32;
        let mean = data.iter().sum::<f32>() / n;
        let var  = data.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n;
        let std  = var.sqrt();
        let rms  = (data.iter().map(|x| x * x).sum::<f32>() / n).sqrt();
        let max  = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let min  = data.iter().cloned().fold(f32::INFINITY, f32::min);
        let kurtosis = if std > 1e-10 {
            data.iter().map(|x| ((x - mean) / std).powi(4)).sum::<f32>() / n - 3.0
        } else { 0.0 };
        AmplitudeFeatures { mean, std, max, min, rms, kurtosis }
    }

    fn phase_features(&self, data: &[f32]) -> PhaseFeatures {
        if data.is_empty() {
            return PhaseFeatures { mean: 0.0, std: 0.0, range: 0.0, velocity: 0.0 };
        }
        let n = data.len() as f32;
        let mean = data.iter().sum::<f32>() / n;
        let std  = (data.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n).sqrt();
        let max  = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let min  = data.iter().cloned().fold(f32::INFINITY, f32::min);
        let velocity = if data.len() > 1 {
            data.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / (data.len() - 1) as f32
        } else { 0.0 };
        PhaseFeatures { mean, std, range: max - min, velocity }
    }

    fn doppler_features(&self, data: &[f32]) -> DopplerFeatures {
        let spectrum = self.compute_fft_magnitude(data);
        let n = spectrum.len();
        if n == 0 {
            return DopplerFeatures { peak_freq_hz: 0.0, motion_energy: 0.0, spectral_entropy: 0.0 };
        }

        let freq_res = self.config.sampling_rate / n as f32;

        // Total energy
        let total: f32 = spectrum.iter().sum::<f32>().max(1e-10);

        // Motion band [0.1, 3.0] Hz
        let motion_lo = (0.1 / freq_res) as usize;
        let motion_hi = ((3.0 / freq_res) as usize).min(n);
        let motion_energy: f32 = spectrum[motion_lo..motion_hi].iter().sum::<f32>() / total;

        // Peak
        let (peak_bin, _) = spectrum.iter()
            .enumerate()
            .fold((0, 0.0f32), |(bi, bv), (i, &v)| if v > bv { (i, v) } else { (bi, bv) });
        let peak_freq_hz = peak_bin as f32 * freq_res;

        // Spectral entropy
        let entropy = spectrum.iter()
            .map(|&s| { let p = s / total; if p > 1e-10 { -p * p.ln() } else { 0.0 } })
            .sum::<f32>();

        DopplerFeatures { peak_freq_hz, motion_energy, spectral_entropy: entropy }
    }

    fn psd_features(&self, data: &[f32]) -> PowerSpectralDensity {
        let spectrum = self.compute_fft_magnitude(data);
        let n = spectrum.len();
        if n == 0 {
            return PowerSpectralDensity { peak_hz: 0.0, breathing_band_power: 0.0, cardiac_band_power: 0.0, breathing_hz: 0.0, heart_rate_hz: 0.0 };
        }

        let freq_res = self.config.sampling_rate / n as f32;
        let total: f32 = spectrum.iter().sum::<f32>().max(1e-10);

        // Breathing band [0.1, 0.5] Hz
        let b_lo = (0.1 / freq_res) as usize;
        let b_hi = ((0.5 / freq_res) as usize).min(n);
        let breathing_band_power = spectrum[b_lo..b_hi].iter().sum::<f32>() / total;

        let breathing_hz = if b_hi > b_lo {
            let peak = spectrum[b_lo..b_hi].iter().cloned()
                .enumerate().fold((0usize, 0.0f32), |(bi, bv), (i, v)| if v > bv { (i, v) } else { (bi, bv) });
            (b_lo + peak.0) as f32 * freq_res
        } else { 0.0 };

        // Cardiac band [0.8, 2.0] Hz
        let c_lo = (0.8 / freq_res) as usize;
        let c_hi = ((2.0 / freq_res) as usize).min(n);
        let cardiac_band_power = spectrum[c_lo..c_hi].iter().sum::<f32>() / total;

        let heart_rate_hz = if c_hi > c_lo {
            let peak = spectrum[c_lo..c_hi].iter().cloned()
                .enumerate().fold((0usize, 0.0f32), |(bi, bv), (i, v)| if v > bv { (i, v) } else { (bi, bv) });
            (c_lo + peak.0) as f32 * freq_res
        } else { 0.0 };

        let (peak_bin, _) = spectrum.iter()
            .enumerate().fold((0usize, 0.0f32), |(bi, bv), (i, &v)| if v > bv { (i, v) } else { (bi, bv) });
        let peak_hz = peak_bin as f32 * freq_res;

        PowerSpectralDensity { peak_hz, breathing_band_power, cardiac_band_power, breathing_hz, heart_rate_hz }
    }

    fn correlation_features(&self, amplitude: &[Vec<f32>]) -> CorrelationFeatures {
        if amplitude.len() < 2 {
            return CorrelationFeatures { mean_cross_link: 0.0, autocorr_lag1: 0.0 };
        }

        let mut cross_sum = 0.0f32;
        let mut cross_count = 0usize;

        for i in 0..amplitude.len() {
            for j in (i + 1)..amplitude.len() {
                let r = pearson_r(&amplitude[i], &amplitude[j]);
                cross_sum += r;
                cross_count += 1;
            }
        }
        let mean_cross_link = if cross_count > 0 { cross_sum / cross_count as f32 } else { 0.0 };

        // Temporal autocorrelation at lag 1 (using first link)
        let first = &amplitude[0];
        let autocorr_lag1 = if first.len() > 1 {
            pearson_r(&first[..first.len() - 1], &first[1..])
        } else { 0.0 };

        CorrelationFeatures { mean_cross_link, autocorr_lag1 }
    }

    /// Compute single-sided FFT magnitude spectrum using rustfft.
    fn compute_fft_magnitude(&self, data: &[f32]) -> Vec<f32> {
        let fft_size = self.config.fft_size.min(data.len()).max(1);
        let n = fft_size;

        // Zero-pad or truncate to fft_size
        let mut buffer: Vec<Complex<f32>> = (0..n)
            .map(|i| Complex::new(if i < data.len() { data[i] } else { 0.0 }, 0.0))
            .collect();

        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);
        fft.process(&mut buffer);

        // Single-sided magnitude
        let half = n / 2;
        buffer[..half].iter().map(|c| c.norm()).collect()
    }
}

fn pearson_r(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n < 2 { return 0.0; }
    let n_f = n as f32;
    let ma = a[..n].iter().sum::<f32>() / n_f;
    let mb = b[..n].iter().sum::<f32>() / n_f;
    let num: f32 = a[..n].iter().zip(b[..n].iter()).map(|(&x, &y)| (x - ma) * (y - mb)).sum();
    let da: f32 = a[..n].iter().map(|&x| (x - ma).powi(2)).sum::<f32>().sqrt();
    let db: f32 = b[..n].iter().map(|&y| (y - mb).powi(2)).sum::<f32>().sqrt();
    let denom = da * db;
    if denom < 1e-10 { 0.0 } else { (num / denom).clamp(-1.0, 1.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_amp(n_links: usize, n_sc: usize) -> Vec<Vec<f32>> {
        (0..n_links)
            .map(|l| (0..n_sc).map(|i| ((l + i) as f32 * 0.05).sin().abs()).collect())
            .collect()
    }

    #[test]
    fn test_extract_returns_features() {
        let ex = FeatureExtractor::new(FeatureExtractorConfig::default());
        let amp = make_amp(4, 64);
        let phase = make_amp(4, 64);
        let f = ex.extract(&amp, &phase);
        assert!(f.amplitude.mean >= 0.0);
        assert!(f.amplitude.rms >= 0.0);
        assert!(f.psd.breathing_hz >= 0.0);
    }

    #[test]
    fn test_pearson_identical() {
        let a: Vec<f32> = (0..32).map(|i| i as f32).collect();
        let r = pearson_r(&a, &a);
        assert!((r - 1.0).abs() < 1e-5, "identical vectors: r={r}");
    }

    #[test]
    fn test_pearson_opposite() {
        let a: Vec<f32> = (0..32).map(|i| i as f32).collect();
        let b: Vec<f32> = a.iter().map(|x| -*x).collect();
        let r = pearson_r(&a, &b);
        assert!((r + 1.0).abs() < 1e-5, "opposite vectors: r={r}");
    }
}
