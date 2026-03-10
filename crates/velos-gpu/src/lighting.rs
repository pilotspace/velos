//! Time-of-day lighting system with keyframe interpolation.
//!
//! Provides a `LightingUniform` struct (48 bytes, GPU-aligned) that holds
//! sun direction, sun color, ambient color, and ambient intensity. The
//! `compute_lighting` function interpolates between dawn/noon/sunset/night
//! keyframes based on simulation elapsed time.

use bytemuck::{Pod, Zeroable};

/// GPU-aligned lighting uniform (48 bytes).
///
/// Layout: sun_direction(12) + pad(4) + sun_color(12) + pad(4) + ambient_color(12) + ambient_intensity(4) = 48.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct LightingUniform {
    /// Normalized direction toward the sun.
    pub sun_direction: [f32; 3],
    pub _pad0: f32,
    /// Sun light color (RGB).
    pub sun_color: [f32; 3],
    pub _pad1: f32,
    /// Ambient light color (RGB).
    pub ambient_color: [f32; 3],
    /// Ambient light intensity multiplier.
    pub ambient_intensity: f32,
}

/// A lighting keyframe at a specific time of day.
struct LightingKeyframe {
    /// Time of day in seconds [0, 86400).
    time: f32,
    /// Sun direction (will be normalized after interpolation).
    sun_direction: [f32; 3],
    /// Sun color RGB.
    sun_color: [f32; 3],
    /// Ambient color RGB.
    ambient_color: [f32; 3],
    /// Ambient intensity.
    ambient_intensity: f32,
}

/// Four keyframes defining the day/night lighting cycle.
const KEYFRAMES: [LightingKeyframe; 4] = [
    // Night (0:00 / midnight)
    LightingKeyframe {
        time: 0.0,
        sun_direction: [0.0, -1.0, 0.0],
        sun_color: [0.1, 0.1, 0.2],
        ambient_color: [0.2, 0.2, 0.4],
        ambient_intensity: 0.15,
    },
    // Dawn (6:00)
    LightingKeyframe {
        time: 21600.0,
        sun_direction: [0.5, 0.3, 0.5],
        sun_color: [1.0, 0.7, 0.4],
        ambient_color: [0.8, 0.6, 0.4],
        ambient_intensity: 0.3,
    },
    // Noon (12:00)
    LightingKeyframe {
        time: 43200.0,
        sun_direction: [0.0, -1.0, 0.2],
        sun_color: [1.0, 1.0, 0.95],
        ambient_color: [0.9, 0.9, 1.0],
        ambient_intensity: 0.6,
    },
    // Sunset (18:00)
    LightingKeyframe {
        time: 64800.0,
        sun_direction: [-0.5, 0.3, -0.5],
        sun_color: [1.0, 0.5, 0.2],
        ambient_color: [0.8, 0.5, 0.3],
        ambient_intensity: 0.25,
    },
];

/// Linearly interpolate between two f32 values.
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Linearly interpolate between two [f32; 3] arrays.
fn lerp_vec3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        lerp_f32(a[0], b[0], t),
        lerp_f32(a[1], b[1], t),
        lerp_f32(a[2], b[2], t),
    ]
}

/// Normalize a [f32; 3] vector. Returns unit vector or [0, -1, 0] if degenerate.
fn normalize_vec3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-6 {
        return [0.0, -1.0, 0.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

/// Compute the lighting uniform for a given simulation elapsed time.
///
/// Interpolates between four keyframes (night, dawn, noon, sunset) based
/// on time-of-day derived from `sim_elapsed_seconds % 86400`.
pub fn compute_lighting(sim_elapsed_seconds: f64) -> LightingUniform {
    const DAY_SECONDS: f64 = 86400.0;
    let tod = (sim_elapsed_seconds % DAY_SECONDS) as f32;
    // Ensure positive modulo
    let tod = if tod < 0.0 { tod + DAY_SECONDS as f32 } else { tod };

    // Find surrounding keyframes. Keyframes wrap: after sunset comes night again.
    let (kf_a, kf_b, t) = {
        let kfs = &KEYFRAMES;
        let n = kfs.len();
        // Find the keyframe pair that brackets `tod`
        let mut idx = n - 1; // default: last keyframe (sunset->night wrap)
        for (i, kf) in kfs.iter().enumerate().take(n) {
            if tod < kf.time {
                idx = if i == 0 { n - 1 } else { i - 1 };
                break;
            }
        }
        let next = (idx + 1) % n;
        let a_time = kfs[idx].time;
        let b_time = if next == 0 {
            DAY_SECONDS as f32
        } else {
            kfs[next].time
        };
        let span = b_time - a_time;
        let local_t = if span > 0.0 {
            (tod - a_time) / span
        } else {
            0.0
        };
        (&kfs[idx], &kfs[next], local_t.clamp(0.0, 1.0))
    };

    let sun_direction = normalize_vec3(lerp_vec3(kf_a.sun_direction, kf_b.sun_direction, t));
    let sun_color = lerp_vec3(kf_a.sun_color, kf_b.sun_color, t);
    let ambient_color = lerp_vec3(kf_a.ambient_color, kf_b.ambient_color, t);
    let ambient_intensity = lerp_f32(kf_a.ambient_intensity, kf_b.ambient_intensity, t);

    LightingUniform {
        sun_direction,
        _pad0: 0.0,
        sun_color,
        _pad1: 0.0,
        ambient_color,
        ambient_intensity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lighting_uniform_size() {
        assert_eq!(
            std::mem::size_of::<LightingUniform>(),
            48,
            "LightingUniform must be 48 bytes for GPU alignment"
        );
    }

    #[test]
    fn test_midnight_low_ambient() {
        let l = compute_lighting(0.0);
        // Night: ambient_intensity should be low (around 0.15)
        assert!(
            l.ambient_intensity < 0.2,
            "Midnight ambient_intensity={} should be < 0.2",
            l.ambient_intensity
        );
        // Cool blue tint: ambient blue channel > red channel
        assert!(
            l.ambient_color[2] > l.ambient_color[0],
            "Night ambient should have blue > red: {:?}",
            l.ambient_color
        );
    }

    #[test]
    fn test_noon_high_ambient() {
        let l = compute_lighting(43200.0); // 12:00
        // Noon: high ambient intensity (around 0.6)
        assert!(
            l.ambient_intensity > 0.5,
            "Noon ambient_intensity={} should be > 0.5",
            l.ambient_intensity
        );
        // Bright white sun
        assert!(
            l.sun_color[0] > 0.9 && l.sun_color[1] > 0.9,
            "Noon sun should be bright white: {:?}",
            l.sun_color
        );
    }

    #[test]
    fn test_noon_sun_direction_downward() {
        let l = compute_lighting(43200.0);
        // Sun at noon should point roughly downward (-Y dominant)
        assert!(
            l.sun_direction[1] < -0.5,
            "Noon sun_direction Y={} should be < -0.5 (downward)",
            l.sun_direction[1]
        );
    }

    #[test]
    fn test_smooth_interpolation() {
        // Check that lighting changes smoothly (no discontinuities)
        let step = 600.0; // 10-minute steps
        let mut prev = compute_lighting(0.0);
        for i in 1..144 {
            // 144 * 600 = 86400
            let t = i as f64 * step;
            let curr = compute_lighting(t);
            // Ambient intensity should not jump more than 0.15 in 10 minutes
            let delta = (curr.ambient_intensity - prev.ambient_intensity).abs();
            assert!(
                delta < 0.15,
                "Discontinuity at t={}: intensity jumped by {} (prev={}, curr={})",
                t,
                delta,
                prev.ambient_intensity,
                curr.ambient_intensity
            );
            prev = curr;
        }
    }

    #[test]
    fn test_sun_direction_normalized() {
        for &t in &[0.0, 21600.0, 43200.0, 64800.0, 86399.0] {
            let l = compute_lighting(t);
            let len = (l.sun_direction[0].powi(2)
                + l.sun_direction[1].powi(2)
                + l.sun_direction[2].powi(2))
            .sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "Sun direction at t={} not normalized: len={}",
                t,
                len
            );
        }
    }
}
