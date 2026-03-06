//! Intelligent Driver Model (IDM) car-following acceleration.
//!
//! Computes longitudinal acceleration based on gap to leader, own speed,
//! and relative speed. Includes a ballistic stopping guard to prevent
//! negative velocity after Euler integration.
//!
//! Reference: Treiber, Hennecke, Helbing (2000),
//!            <https://traffic-simulation.de/info/info_IDM.html>

/// Parameters for the IDM car-following model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IdmParams {
    /// Desired speed in free traffic (m/s).
    pub v0: f64,
    /// Minimum gap to leader at standstill (m).
    pub s0: f64,
    /// Desired time headway (s).
    pub t_headway: f64,
    /// Maximum acceleration (m/s^2).
    pub a: f64,
    /// Comfortable braking deceleration, positive value (m/s^2).
    pub b: f64,
    /// Free acceleration exponent (typically 4.0).
    pub delta: f64,
}

/// Maximum allowed deceleration (hard physical limit, m/s^2).
const MAX_DECEL: f64 = -9.0;

/// Compute IDM acceleration given current state and leader info.
///
/// # Arguments
/// * `params` - IDM model parameters
/// * `v` - current speed of subject vehicle (m/s, >= 0)
/// * `gap` - net distance gap to leader (m, > 0 expected)
/// * `delta_v` - speed difference `v_subject - v_leader` (m/s)
///
/// # Returns
/// Acceleration in m/s^2, clamped to `[MAX_DECEL, params.a]`.
pub fn idm_acceleration(params: &IdmParams, v: f64, gap: f64, delta_v: f64) -> f64 {
    // Free-road acceleration term: 1 - (v/v0)^delta
    // Compute (v/v0)^4 via multiplication for numerical stability
    let v_ratio = v / params.v0;
    let v_ratio_sq = v_ratio * v_ratio;
    let free_term = 1.0 - v_ratio_sq * v_ratio_sq;

    // Desired dynamical gap s*
    // v_eff kickstart: use at least 0.1 m/s to avoid zero-speed issues
    let v_eff = v.max(0.1);
    let s_star = params.s0
        + v_eff * params.t_headway
        + (v * delta_v) / (2.0 * (params.a * params.b).sqrt());

    // Interaction (braking) term
    // gap_eff: floor at 0.01 to avoid division by zero
    let gap_eff = gap.max(0.01);
    let gap_ratio = s_star / gap_eff;
    let interaction = gap_ratio * gap_ratio;

    // IDM acceleration
    let accel = params.a * (free_term - interaction);

    // Clamp to physical limits
    accel.clamp(MAX_DECEL, params.a)
}

/// Integrate velocity with a ballistic stopping guard.
///
/// Prevents negative velocity after Euler integration. When `v + accel * dt < 0`,
/// the vehicle decelerates to zero and stops within the timestep.
///
/// # Arguments
/// * `v` - current speed (m/s, >= 0)
/// * `accel` - acceleration from IDM (m/s^2)
/// * `dt` - timestep duration (s)
///
/// # Returns
/// `(v_new, dx)` where `v_new >= 0` and `dx >= 0`.
pub fn integrate_with_stopping_guard(v: f64, accel: f64, dt: f64) -> (f64, f64) {
    let v_new = v + accel * dt;
    if v_new < 0.0 {
        // Vehicle would go negative -- stop at zero
        let t_stop = (-v / accel).min(dt);
        let dx = v * t_stop + 0.5 * accel * t_stop * t_stop;
        (0.0, dx.max(0.0))
    } else {
        let dx = v * dt + 0.5 * accel * dt * dt;
        (v_new, dx)
    }
}
