//! Krauss car-following model (SUMO-faithful CPU reference implementation).
//!
//! Computes safe following speed and applies stochastic dawdle deceleration.
//! This serves as the CPU test oracle for GPU shader validation.
//!
//! Reference: Krauss (1998), SUMO MSCFModel_Krauss.cpp,
//!            <https://sumo.dlr.de/docs/Car-Following-Models.html>

use rand::Rng;

/// Parameters for the Krauss car-following model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KraussParams {
    /// Maximum acceleration (m/s^2).
    pub accel: f64,
    /// Maximum deceleration (m/s^2, positive value).
    pub decel: f64,
    /// Driver imperfection / dawdle parameter \[0.0, 1.0\].
    pub sigma: f64,
    /// Reaction time / driver tau (s). Typically 1.0.
    pub tau: f64,
    /// Maximum speed (m/s).
    pub max_speed: f64,
    /// Minimum gap at standstill (m).
    pub min_gap: f64,
}

impl KraussParams {
    /// SUMO default passenger car Krauss parameters.
    pub fn sumo_default() -> Self {
        Self {
            accel: 2.6,
            decel: 4.5,
            sigma: 0.5,
            tau: 1.0,
            max_speed: 13.89, // 50 km/h
            min_gap: 2.5,
        }
    }
}

/// Compute safe following speed (vsafe).
///
/// Ensures the follower can always stop before hitting the leader,
/// assuming the leader brakes maximally.
///
/// Formula (SUMO Krauss):
/// ```text
/// v_safe = v_leader + (gap - v_leader * tau) / ((v_leader + v_follower) / (2*b) + tau)
/// ```
///
/// # Arguments
/// * `params` - Krauss model parameters.
/// * `gap` - Net distance gap to leader (m).
/// * `leader_speed` - Leader's current speed (m/s).
/// * `own_speed` - Own current speed (m/s).
///
/// # Returns
/// Safe speed in m/s, clamped to >= 0.
pub fn krauss_safe_speed(
    params: &KraussParams,
    gap: f64,
    leader_speed: f64,
    own_speed: f64,
) -> f64 {
    let tau = params.tau;
    let b = params.decel;

    let denominator = (leader_speed + own_speed) / (2.0 * b) + tau;
    let numerator = gap - leader_speed * tau;
    let v_safe = leader_speed + numerator / denominator;

    v_safe.max(0.0)
}

/// Apply Krauss dawdle: random deceleration proportional to sigma.
///
/// When speed is low (below `accel`), dawdle is proportional to current
/// speed to avoid overshooting into negative velocity. Otherwise,
/// dawdle is proportional to `accel`.
///
/// # Arguments
/// * `speed` - Current speed after safe-speed and desired-speed clipping (m/s).
/// * `params` - Krauss model parameters.
/// * `rng` - Random number generator (seeded for reproducibility).
///
/// # Returns
/// Dawdled speed in m/s, clamped to >= 0.
pub fn krauss_dawdle(speed: f64, params: &KraussParams, rng: &mut impl Rng) -> f64 {
    let random: f64 = rng.r#gen(); // [0, 1)
    let dawdle_amount = if speed < params.accel {
        params.sigma * speed * random
    } else {
        params.sigma * params.accel * random
    };
    (speed - dawdle_amount).max(0.0)
}

/// Full Krauss velocity update for one timestep.
///
/// Computes the new velocity by taking the minimum of the desired
/// speed (current + acceleration) and the safe following speed,
/// then applying dawdle deceleration.
///
/// # Arguments
/// * `params` - Krauss model parameters.
/// * `own_speed` - Current speed (m/s, >= 0).
/// * `gap` - Net gap to leader (m).
/// * `leader_speed` - Leader's current speed (m/s).
/// * `dt` - Timestep duration (s).
/// * `rng` - Random number generator.
///
/// # Returns
/// `(v_new, dx)` where `v_new >= 0` and `dx >= 0`.
pub fn krauss_update(
    params: &KraussParams,
    own_speed: f64,
    gap: f64,
    leader_speed: f64,
    dt: f64,
    rng: &mut impl Rng,
) -> (f64, f64) {
    let v_safe = krauss_safe_speed(params, gap, leader_speed, own_speed);
    let v_desired = (own_speed + params.accel * dt).min(params.max_speed);
    let v_next = v_desired.min(v_safe);
    let v_dawdled = krauss_dawdle(v_next, params, rng);
    let v_final = v_dawdled.max(0.0);
    let dx = v_final * dt;
    (v_final, dx)
}
