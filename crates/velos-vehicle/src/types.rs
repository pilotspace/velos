//! Vehicle type definitions and default parameter sets.

use crate::idm::IdmParams;
use crate::mobil::MobilParams;

/// Classification of simulated vehicle/agent types.
///
/// Order must match velos-core VehicleType and WGSL constants in wave_front.wgsl.
/// GPU mapping: 0=Motorbike, 1=Car, 2=Bus, 3=Bicycle, 4=Truck, 5=Emergency, 6=Pedestrian.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VehicleType {
    /// Two-wheeled motorbike (dominant in HCMC, ~80% of traffic). GPU=0.
    Motorbike,
    /// Four-wheeled car (~15% of traffic). GPU=1.
    Car,
    /// Public transit bus. GPU=2.
    Bus,
    /// Bicycle (pedal-powered, uses sublane model with IDM). GPU=3.
    Bicycle,
    /// Heavy goods vehicle / truck. GPU=4.
    Truck,
    /// Emergency vehicle (ambulance, fire truck). GPU=5.
    Emergency,
    /// Pedestrian agent (~5% of traffic). GPU=6.
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
        VehicleType::Bus => IdmParams {
            v0: 11.1,       // 40 km/h desired speed (urban bus)
            s0: 3.0,        // 3m minimum gap (longer vehicle)
            t_headway: 1.5, // 1.5s time headway (cautious)
            a: 1.0,         // 1.0 m/s^2 max accel (heavy)
            b: 2.5,         // 2.5 m/s^2 comfortable decel
            delta: 4.0,
        },
        VehicleType::Bicycle => IdmParams {
            v0: 4.17,       // 15 km/h desired speed
            s0: 1.5,        // 1.5m minimum gap
            t_headway: 1.0, // 1.0s time headway
            a: 1.0,         // 1.0 m/s^2 max accel
            b: 3.0,         // 3.0 m/s^2 comfortable decel
            delta: 4.0,
        },
        VehicleType::Truck => IdmParams {
            v0: 25.0,       // 90 km/h desired speed
            s0: 4.0,        // 4m minimum gap (long vehicle)
            t_headway: 2.0, // 2.0s time headway (heavy, slow reaction)
            a: 1.0,         // 1.0 m/s^2 max accel (heavy)
            b: 2.5,         // 2.5 m/s^2 comfortable decel
            delta: 4.0,
        },
        VehicleType::Emergency => IdmParams {
            v0: 16.7,       // 60 km/h desired speed (sirens on)
            s0: 2.0,        // 2m minimum gap
            t_headway: 1.2, // 1.2s time headway (trained driver)
            a: 2.0,         // 2.0 m/s^2 max accel (powerful engine)
            b: 3.5,         // 3.5 m/s^2 comfortable decel (heavy braking OK)
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
