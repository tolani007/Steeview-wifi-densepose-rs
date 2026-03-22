//! Fresnel zone geometry for estimating human-reflector distance.

use std::f32::consts::PI;

/// Compute Fresnel zone radius at midpoint between TX and RX.
///
/// r = sqrt(λ·d/2) for first Fresnel zone
pub fn fresnel_radius_m(freq_hz: f32, distance_m: f32) -> f32 {
    let lambda = 3e8 / freq_hz;
    (lambda * distance_m / 2.0).sqrt()
}

/// Estimate the signal attenuation due to a human body in the Fresnel zone.
/// Returns a scale factor in [0, 1] (0 = fully blocked, 1 = clear path).
pub fn fresnel_obstruction_factor(
    reflector_x: f32,
    reflector_y: f32,
    tx_x: f32,
    tx_y: f32,
    rx_x: f32,
    rx_y: f32,
    freq_hz: f32,
) -> f32 {
    // Distance from reflector to the TX-RX line segment
    let len_sq = (rx_x - tx_x).powi(2) + (rx_y - tx_y).powi(2);
    if len_sq < 1e-10 { return 0.0; }

    let t = ((reflector_x - tx_x) * (rx_x - tx_x) + (reflector_y - tx_y) * (rx_y - tx_y)) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let proj_x = tx_x + t * (rx_x - tx_x);
    let proj_y = tx_y + t * (rx_y - tx_y);
    let perp_dist = ((reflector_x - proj_x).powi(2) + (reflector_y - proj_y).powi(2)).sqrt();

    let path_len = len_sq.sqrt();
    let r1 = fresnel_radius_m(freq_hz, path_len);

    // Normalized Fresnel clearance
    let clearance = (perp_dist / r1.max(0.01)).min(3.0);
    // Smooth step: full block at 0, fully clear beyond 1.4 r1
    (clearance / 1.4).min(1.0)
}

/// Phase shift (radians) from a reflector at given Fresnel depth.
pub fn fresnel_phase_shift(extra_path_m: f32, freq_hz: f32) -> f32 {
    let lambda = 3e8 / freq_hz;
    2.0 * PI * extra_path_m / lambda
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fresnel_radius_2ghz() {
        let r = fresnel_radius_m(2.4e9, 5.0);
        assert!(r > 0.2 && r < 1.0, "fresnel radius at 2.4GHz/5m: {r}");
    }

    #[test]
    fn test_obstruction_on_path() {
        // Reflector directly on TX-RX midpoint — should be heavily blocked
        let factor = fresnel_obstruction_factor(2.5, 0.0, 0.0, 0.0, 5.0, 0.0, 2.4e9);
        assert!(factor < 0.5, "blocked path factor should be low: {factor}");
    }

    #[test]
    fn test_obstruction_far_away() {
        // Reflector 10m perpendicular — should be nearly clear
        let factor = fresnel_obstruction_factor(2.5, 10.0, 0.0, 0.0, 5.0, 0.0, 2.4e9);
        assert!(factor > 0.9, "distant reflector should be nearly clear: {factor}");
    }
}
