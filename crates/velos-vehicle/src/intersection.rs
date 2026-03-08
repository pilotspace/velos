//! Intersection gap acceptance for unsignalized intersections.
//!
//! Implements vehicle-type-dependent TTC thresholds with:
//! - Size intimidation: larger approaching vehicles increase required gap
//! - Wait-time modifier: longer wait reduces threshold (first-come priority)
//! - Max-wait forced acceptance: after 5s, threshold halved to break deadlock
//!
//! Models the "organized chaos" at HCMC unsignalized intersections where
//! motorbikes negotiate gaps aggressively while cars yield to larger vehicles.

use crate::types::VehicleType;

/// Per-agent intersection state for gap acceptance decisions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntersectionState {
    /// Time spent waiting at the intersection (seconds).
    pub wait_time: f64,
    /// Arrival sequence number (lower = arrived earlier).
    pub arrival_order: u32,
}

/// Maximum wait time before forced acceptance (seconds).
const MAX_WAIT_TIME: f64 = 5.0;

/// Forced acceptance multiplier applied after max wait.
const FORCED_ACCEPTANCE_FACTOR: f64 = 0.5;

/// Wait-time reduction rate per second of waiting.
/// Threshold reduced by 10% per second of waiting (up to 50% at 5s).
const WAIT_REDUCTION_RATE: f64 = 0.1;

/// Compute the size intimidation factor for an approaching vehicle.
///
/// Larger vehicles increase the required gap (more cautious).
/// Smaller vehicles decrease it (more aggressive gap acceptance).
fn size_factor(approaching: VehicleType) -> f64 {
    match approaching {
        VehicleType::Truck | VehicleType::Bus => 1.3,
        VehicleType::Emergency => 2.0,
        VehicleType::Motorbike | VehicleType::Bicycle => 0.8,
        VehicleType::Car => 1.0,
        VehicleType::Pedestrian => 0.5,
    }
}

/// Determine if a vehicle should proceed through an unsignalized intersection.
///
/// Uses vehicle-type-dependent TTC thresholds modified by:
/// - Size intimidation: larger approaching vehicles increase required TTC
/// - First-come priority: longer wait reduces effective threshold
/// - Max-wait forced acceptance: after 5s, threshold halved to break deadlock
///
/// # Arguments
/// * `_own_type` - vehicle type of the deciding agent (reserved for future per-type rules)
/// * `other_type` - vehicle type of the approaching/conflicting agent
/// * `ttc` - time-to-collision with the conflicting agent (seconds)
/// * `own_ttc_threshold` - base gap acceptance threshold from config (seconds)
/// * `state` - intersection waiting state
///
/// # Returns
/// `true` if the vehicle should proceed (gap is acceptable).
pub fn intersection_gap_acceptance(
    _own_type: VehicleType,
    other_type: VehicleType,
    ttc: f64,
    own_ttc_threshold: f64,
    state: &IntersectionState,
) -> bool {
    let sf = size_factor(other_type);

    let wait_modifier = if state.wait_time >= MAX_WAIT_TIME {
        // Forced acceptance: halve the threshold to break deadlock
        FORCED_ACCEPTANCE_FACTOR
    } else {
        // Gradual reduction: 10% per second of waiting, min 0.5
        1.0 - WAIT_REDUCTION_RATE * state.wait_time.min(MAX_WAIT_TIME)
    };

    let effective_threshold = own_ttc_threshold * sf * wait_modifier;

    // Gap is acceptable if TTC exceeds the effective threshold
    ttc > effective_threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_factor_emergency_is_highest() {
        assert!((size_factor(VehicleType::Emergency) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn size_factor_motorbike_is_reduced() {
        assert!((size_factor(VehicleType::Motorbike) - 0.8).abs() < 1e-10);
    }

    #[test]
    fn size_factor_car_is_neutral() {
        assert!((size_factor(VehicleType::Car) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn size_factor_truck_bus_same() {
        assert!((size_factor(VehicleType::Truck) - size_factor(VehicleType::Bus)).abs() < 1e-10);
    }
}
