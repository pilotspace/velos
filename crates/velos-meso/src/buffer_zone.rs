//! Buffer zone IDM interpolation and velocity-matching insertion.
//!
//! The buffer zone is a 100m graduated transition region between mesoscopic and
//! microscopic simulation zones. IDM parameters are smoothly interpolated from
//! "relaxed" (meso-side) to "normal" (micro-side) using a C1-continuous smoothstep
//! function, preventing phantom braking at zone boundaries.

use velos_vehicle::idm::IdmParams;

/// Default buffer zone length in meters.
pub const DEFAULT_BUFFER_LENGTH: f64 = 100.0;

/// Maximum speed difference (m/s) allowed for micro zone insertion.
const MAX_SPEED_DIFF_FOR_INSERT: f64 = 2.0;

/// C1-continuous smoothstep function: `3x^2 - 2x^3`.
///
/// Maps [0, 1] -> [0, 1] with zero derivatives at both endpoints,
/// ensuring smooth transitions without discontinuities.
///
/// Input is clamped to [0, 1].
pub fn smoothstep(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Interpolate IDM parameters between relaxed (meso) and normal (micro) values.
///
/// Uses [`smoothstep`] for C1-continuous interpolation across the buffer zone.
///
/// # Arguments
/// * `relaxed` - IDM parameters at the meso boundary (distance = 0)
/// * `normal` - IDM parameters at the micro boundary (distance = buffer_length)
/// * `distance_into_buffer` - Distance from the meso boundary (meters)
/// * `buffer_length` - Total buffer zone length (meters)
///
/// # Returns
/// Interpolated IDM parameters at the given buffer position.
pub fn interpolate_idm_params(
    relaxed: &IdmParams,
    normal: &IdmParams,
    distance_into_buffer: f64,
    buffer_length: f64,
) -> IdmParams {
    let x = (distance_into_buffer / buffer_length).clamp(0.0, 1.0);
    let t = smoothstep(x);

    let lerp = |a: f64, b: f64| a + (b - a) * t;

    IdmParams {
        v0: lerp(relaxed.v0, normal.v0),
        s0: lerp(relaxed.s0, normal.s0),
        t_headway: lerp(relaxed.t_headway, normal.t_headway),
        a: lerp(relaxed.a, normal.a),
        b: lerp(relaxed.b, normal.b),
        delta: lerp(relaxed.delta, normal.delta),
    }
}

/// Compute velocity-matching insertion speed.
///
/// Returns the minimum of the meso exit speed and the last micro vehicle speed,
/// preventing speed mismatch at the boundary (RESEARCH.md Pitfall 4).
pub fn velocity_matching_speed(meso_exit_speed: f64, last_micro_vehicle_speed: f64) -> f64 {
    meso_exit_speed.min(last_micro_vehicle_speed)
}

/// Buffer zone configuration for meso-micro transitions.
///
/// Contains the relaxed and normal IDM parameter sets and the buffer length.
#[derive(Debug, Clone)]
pub struct BufferZone {
    /// Buffer zone length in meters (default 100m).
    pub buffer_length: f64,
    /// Relaxed IDM parameters used at the meso boundary.
    pub relaxed_params: IdmParams,
    /// Normal IDM parameters used at the micro boundary.
    pub normal_params: IdmParams,
}

impl BufferZone {
    /// Create a new buffer zone with the given parameters.
    pub fn new(relaxed_params: IdmParams, normal_params: IdmParams) -> Self {
        Self {
            buffer_length: DEFAULT_BUFFER_LENGTH,
            relaxed_params,
            normal_params,
        }
    }

    /// Create a buffer zone with a custom length.
    pub fn with_length(mut self, length: f64) -> Self {
        self.buffer_length = length;
        self
    }

    /// Get interpolated IDM parameters at a position within the buffer.
    pub fn params_at(&self, distance_into_buffer: f64) -> IdmParams {
        interpolate_idm_params(
            &self.relaxed_params,
            &self.normal_params,
            distance_into_buffer,
            self.buffer_length,
        )
    }

    /// Check whether an agent at the given buffer position with the given speed
    /// difference is ready to be inserted into the micro zone.
    ///
    /// Insertion is allowed when:
    /// - The agent has traversed the full buffer (distance >= buffer_length)
    /// - The speed difference to the micro lane is within tolerance (<=2 m/s)
    pub fn should_insert(distance_into_buffer: f64, speed_diff: f64) -> bool {
        distance_into_buffer >= DEFAULT_BUFFER_LENGTH
            && speed_diff <= MAX_SPEED_DIFF_FOR_INSERT
    }
}

/// Default relaxed IDM parameters for the meso side of the buffer.
///
/// More lenient than normal micro parameters:
/// - Higher time headway (3.0s vs ~1.6s)
/// - Lower acceleration (0.5 m/s^2 vs ~1.0 m/s^2)
/// - Lower comfortable deceleration (1.5 m/s^2 vs ~2.0 m/s^2)
pub fn default_relaxed_params(normal: &IdmParams) -> IdmParams {
    IdmParams {
        v0: normal.v0,       // same desired speed
        s0: normal.s0,       // same minimum gap
        t_headway: 3.0,      // more lenient headway
        a: 0.5,              // gentler acceleration
        b: 1.5,              // gentler deceleration
        delta: normal.delta, // same exponent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothstep_monotonically_increasing() {
        let mut prev = 0.0;
        for i in 0..=100 {
            let x = i as f64 / 100.0;
            let y = smoothstep(x);
            assert!(y >= prev, "smoothstep not monotonic at x={x}");
            prev = y;
        }
    }

    #[test]
    fn default_relaxed_params_preserves_speed_and_gap() {
        let normal = IdmParams {
            v0: 13.89,
            s0: 2.0,
            t_headway: 1.6,
            a: 1.0,
            b: 2.0,
            delta: 4.0,
        };
        let relaxed = default_relaxed_params(&normal);
        assert!((relaxed.v0 - normal.v0).abs() < 1e-9);
        assert!((relaxed.s0 - normal.s0).abs() < 1e-9);
        assert!((relaxed.t_headway - 3.0).abs() < 1e-9);
        assert!((relaxed.a - 0.5).abs() < 1e-9);
    }
}
