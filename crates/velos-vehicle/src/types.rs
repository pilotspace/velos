//! Vehicle type definitions and default parameter sets.

use crate::idm::IdmParams;
use crate::mobil::MobilParams;

/// Classification of simulated vehicle/agent types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VehicleType {
    /// Two-wheeled motorbike (dominant in HCMC, ~80% of traffic).
    Motorbike,
    /// Four-wheeled car (~15% of traffic).
    Car,
    /// Pedestrian agent (~5% of traffic).
    Pedestrian,
}

/// Return the default IDM parameters for a given vehicle type.
///
/// Values sourced from architecture doc `02-agent-models.md` and IDM literature.
pub fn default_idm_params(vehicle_type: VehicleType) -> IdmParams {
    match vehicle_type {
        VehicleType::Car => IdmParams {
            v0: 13.9,       // 50 km/h desired speed
            s0: 2.0,        // 2m minimum gap
            t_headway: 1.5, // 1.5s time headway
            a: 1.0,         // 1.0 m/s^2 max accel
            b: 2.0,         // 2.0 m/s^2 comfortable decel
            delta: 4.0,     // acceleration exponent
        },
        VehicleType::Motorbike => IdmParams {
            v0: 11.1,       // 40 km/h desired speed
            s0: 1.0,        // 1m minimum gap (smaller vehicle)
            t_headway: 1.0, // 1.0s time headway (more aggressive)
            a: 2.0,         // 2.0 m/s^2 max accel (lighter)
            b: 3.0,         // 3.0 m/s^2 comfortable decel
            delta: 4.0,
        },
        VehicleType::Pedestrian => IdmParams {
            v0: 1.4,        // 5 km/h walking speed
            s0: 0.5,        // 0.5m personal space
            t_headway: 0.5, // 0.5s reaction time
            a: 0.5,         // 0.5 m/s^2 gentle accel
            b: 1.0,         // 1.0 m/s^2 comfortable decel
            delta: 4.0,
        },
    }
}

/// Return the default MOBIL lane-change parameters for HCMC traffic.
///
/// Politeness 0.3 = moderate altruism, typical for mixed Asian traffic.
pub fn default_mobil_params() -> MobilParams {
    MobilParams {
        politeness: 0.3,
        threshold: 0.2,     // m/s^2 minimum incentive
        safe_decel: -4.0,   // m/s^2 safety limit for new follower
        right_bias: 0.1,    // m/s^2 preference for right lane
    }
}
