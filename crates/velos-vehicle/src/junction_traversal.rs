//! Junction traversal logic: Bezier curve advancement, conflict detection, and IDM yielding.
//!
//! Pure functions operating on junction data from `velos-net::junction`. Used by
//! `velos-gpu::sim_junction::step_junction_traversal()` in the frame pipeline.

use crate::idm::{idm_acceleration, IdmParams};
use crate::types::VehicleType;

/// A precomputed crossing point between two turn paths at a junction.
///
/// Mirrors `velos_net::junction::ConflictPoint` to avoid circular dependency
/// (velos-net depends on velos-vehicle). The sim_junction integration layer
/// converts between them.
#[derive(Debug, Clone, Copy)]
pub struct ConflictPoint {
    /// Index of the first turn in the junction's `turns` array.
    pub turn_a_idx: u16,
    /// Index of the second turn in the junction's `turns` array.
    pub turn_b_idx: u16,
    /// Bezier t-parameter on turn A at the crossing point.
    pub t_a: f32,
    /// Bezier t-parameter on turn B at the crossing point.
    pub t_b: f32,
}

/// Maximum allowed deceleration for junction yielding (m/s^2).
const MAX_JUNCTION_DECEL: f64 = -9.0;

/// Default distance-based proximity for conflict detection (metres).
/// Agents within this distance of a conflict point trigger yielding.
/// Replaces fixed t-proximity to prevent over-yielding on short curves.
pub const DEFAULT_CONFLICT_DISTANCE_M: f64 = 3.0;

/// Compute t-proximity from a distance threshold and arc length.
/// Ensures the conflict zone never spans more than 30% of the curve,
/// preventing vehicles from yielding for most of a short junction.
pub fn t_proximity_from_distance(distance_m: f64, arc_length: f64) -> f64 {
    let safe_arc = arc_length.max(1.0);
    let t_from_dist = distance_m / safe_arc;
    // Cap at 0.10 so the conflict zone (3x) never exceeds 30% of t-range
    t_from_dist.min(0.10)
}

/// Minimum crawl speed forced after deadlock timeout (m/s).
pub const MIN_CRAWL_SPEED: f64 = 1.0;

/// Result of checking conflicts for one agent in a junction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConflictCheckResult {
    /// Distance along curve to the conflict crossing point (metres).
    pub virtual_leader_gap: f64,
    /// Speed of the virtual leader at conflict point (typically 0 for stopped leader).
    pub virtual_leader_speed: f64,
}

/// Advance a Bezier curve parameter `t` forward based on speed and arc length.
///
/// Returns `(new_t, finished, overflow_m)`:
/// - `new_t`: clamped to [0, 1]
/// - `finished`: true when the agent reached or passed t=1.0
/// - `overflow_m`: distance in metres the agent travelled past the curve end
///   (used for smooth chaining into the next junction segment)
///
/// Uses approximately uniform speed: `dt_param = dt * speed / arc_length`.
pub fn advance_on_bezier(t: f64, speed: f64, arc_length: f64, dt: f64) -> (f64, bool, f64) {
    let safe_arc = arc_length.max(1.0);
    let dt_param = dt * speed / safe_arc;
    let raw_t = t + dt_param;
    let clamped_t = raw_t.min(1.0);
    let overflow_m = if raw_t > 1.0 {
        (raw_t - 1.0) * safe_arc
    } else {
        0.0
    };
    (clamped_t, clamped_t >= 1.0, overflow_m)
}

/// Priority ordering for tie-breaking at conflict points.
///
/// Higher value = higher priority. Emergency > Truck > Bus > Car > Motorbike.
/// Bicycle and Pedestrian included for completeness but rarely in junction traversal.
pub fn size_factor(vtype: VehicleType) -> u8 {
    match vtype {
        VehicleType::Emergency => 5,
        VehicleType::Truck => 4,
        VehicleType::Bus => 3,
        VehicleType::Car => 2,
        VehicleType::Motorbike | VehicleType::Bicycle => 1,
        VehicleType::Pedestrian => 0,
    }
}

/// Check if an agent must yield at a conflict point within a junction.
///
/// For each conflict involving `own_turn_idx`, checks if a foe agent is on the
/// other turn within `t_proximity` of the conflict's t-parameters. The agent
/// farther from the conflict point must yield; ties broken by vehicle type priority.
///
/// # Arguments
/// * `own_turn_idx` - Index of the agent's turn in the junction's turns array
/// * `own_t` - Agent's current Bezier t-parameter
/// * `own_type` - Agent's vehicle type (for tie-breaking)
/// * `agents_in_junction` - All agents in the same junction: (turn_idx, t, VehicleType)
/// * `conflicts` - Precomputed conflict points for this junction
/// * `arc_length` - Arc length of the agent's own turn (for gap distance conversion)
/// * `t_proximity` - How close to conflict point counts as "near" (default 0.15)
///
/// # Returns
/// `Some(ConflictCheckResult)` if the agent must yield, `None` if no conflict.
pub fn check_conflicts(
    own_turn_idx: u16,
    own_t: f64,
    own_type: VehicleType,
    agents_in_junction: &[(u16, f64, VehicleType)],
    conflicts: &[ConflictPoint],
    arc_length: f64,
    t_proximity: f64,
) -> Option<ConflictCheckResult> {
    let mut closest_result: Option<ConflictCheckResult> = None;
    let mut closest_gap = f64::MAX;

    for cp in conflicts {
        // Determine which side of the conflict this agent is on
        let (own_cross_t, foe_turn_idx, foe_cross_t): (f64, u16, f64) =
            if cp.turn_a_idx == own_turn_idx {
                (cp.t_a as f64, cp.turn_b_idx, cp.t_b as f64)
            } else if cp.turn_b_idx == own_turn_idx {
                (cp.t_b as f64, cp.turn_a_idx, cp.t_a as f64)
            } else {
                continue; // conflict doesn't involve our turn
            };

        // Check if we are near the conflict point
        let own_dist_to_cross: f64 = own_cross_t - own_t;
        if own_dist_to_cross < -t_proximity {
            // We already passed the conflict point -- no need to yield
            continue;
        }
        if own_dist_to_cross > t_proximity * 3.0 {
            // We're far from the conflict point -- skip early check
            continue;
        }

        // Find foe agents on the other turn
        for &(foe_idx, foe_t, foe_type) in agents_in_junction {
            if foe_idx != foe_turn_idx {
                continue;
            }

            // Check if foe has already passed the conflict point
            if foe_t > foe_cross_t + t_proximity {
                continue; // foe cleared the conflict zone
            }

            // Check if foe is near the conflict point
            let foe_dist_to_cross: f64 = (foe_t - foe_cross_t).abs();
            if foe_dist_to_cross > t_proximity * 3.0 && foe_t < foe_cross_t {
                continue; // foe is far from conflict and hasn't reached it
            }

            // Both agents near conflict -- determine priority
            let own_dist_abs = own_dist_to_cross.abs();
            let foe_dist_abs = foe_dist_to_cross;

            // Priority: closer to crossing point goes first
            let own_priority = size_factor(own_type);
            let foe_priority = size_factor(foe_type);

            let must_yield = if (own_dist_abs - foe_dist_abs).abs() < 0.01 {
                // Tie: use vehicle type priority
                own_priority < foe_priority
            } else {
                // Farther from crossing point yields
                own_dist_abs > foe_dist_abs
            };

            if must_yield {
                // Compute virtual leader gap: distance along our curve to conflict
                let gap = own_dist_to_cross.max(0.01) * arc_length;
                if gap < closest_gap {
                    closest_gap = gap;
                    closest_result = Some(ConflictCheckResult {
                        virtual_leader_gap: gap,
                        virtual_leader_speed: 0.0, // treat conflict point as stopped leader
                    });
                }
            }
        }
    }

    closest_result
}

/// Compute IDM-based deceleration when yielding at a conflict point.
///
/// Treats the conflict crossing point as a virtual stationary leader.
/// Returns acceleration clamped to `[MAX_JUNCTION_DECEL, idm.a]`.
pub fn yield_deceleration(
    own_speed: f64,
    virtual_leader_gap: f64,
    virtual_leader_speed: f64,
    idm: &IdmParams,
) -> f64 {
    let delta_v = own_speed - virtual_leader_speed;
    let accel = idm_acceleration(idm, own_speed, virtual_leader_gap, delta_v);
    accel.clamp(MAX_JUNCTION_DECEL, idm.a)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- advance_on_bezier tests ----

    #[test]
    fn advance_basic_proportional() {
        // speed=10, arc_length=20, dt=0.1 -> dt_param = 0.1*10/20 = 0.05
        let (new_t, finished, overflow) = advance_on_bezier(0.0, 10.0, 20.0, 0.1);
        assert!((new_t - 0.05).abs() < 1e-10);
        assert!(!finished);
        assert!((overflow - 0.0).abs() < 1e-10);
    }

    #[test]
    fn advance_clamps_to_one() {
        let (new_t, finished, overflow) = advance_on_bezier(0.95, 10.0, 10.0, 0.1);
        // dt_param = 0.1*10/10 = 0.1, raw_t = 1.05 -> clamped to 1.0
        // overflow = 0.05 * 10.0 = 0.5m
        assert!((new_t - 1.0).abs() < 1e-10);
        assert!(finished);
        assert!((overflow - 0.5).abs() < 0.01);
    }

    #[test]
    fn advance_finished_at_exactly_one() {
        let (new_t, finished, overflow) = advance_on_bezier(0.9, 10.0, 10.0, 0.1);
        assert!((new_t - 1.0).abs() < 1e-10);
        assert!(finished);
        assert!((overflow - 0.0).abs() < 1e-10);
    }

    #[test]
    fn advance_zero_speed_no_movement() {
        let (new_t, finished, overflow) = advance_on_bezier(0.5, 0.0, 20.0, 0.1);
        assert!((new_t - 0.5).abs() < 1e-10);
        assert!(!finished);
        assert!((overflow - 0.0).abs() < 1e-10);
    }

    #[test]
    fn advance_small_arc_length_uses_floor() {
        // arc_length < 1.0 is clamped to 1.0 to avoid division by tiny value
        let (new_t, _, _) = advance_on_bezier(0.0, 10.0, 0.5, 0.1);
        // dt_param = 0.1*10/1.0 = 1.0, clamped to 1.0
        assert!((new_t - 1.0).abs() < 1e-10);
    }

    #[test]
    fn advance_overflow_distance_correct() {
        // speed=20, arc_length=10, dt=0.1 -> dt_param = 0.2, raw_t = 0.8 + 0.2 = 1.0 (exact)
        let (_, _, overflow) = advance_on_bezier(0.8, 20.0, 10.0, 0.1);
        assert!((overflow - 0.0).abs() < 1e-10, "exact finish should have zero overflow");

        // speed=20, arc_length=10, dt=0.2 -> dt_param = 0.4, raw_t = 0.8 + 0.4 = 1.2
        // overflow = 0.2 * 10.0 = 2.0m
        let (_, finished, overflow) = advance_on_bezier(0.8, 20.0, 10.0, 0.2);
        assert!(finished);
        assert!((overflow - 2.0).abs() < 0.01, "overflow should be 2.0m, got {}", overflow);
    }

    // ---- size_factor tests ----

    #[test]
    fn size_factor_ordering() {
        assert!(size_factor(VehicleType::Emergency) > size_factor(VehicleType::Truck));
        assert!(size_factor(VehicleType::Truck) > size_factor(VehicleType::Bus));
        assert!(size_factor(VehicleType::Bus) > size_factor(VehicleType::Car));
        assert!(size_factor(VehicleType::Car) > size_factor(VehicleType::Motorbike));
        assert_eq!(size_factor(VehicleType::Motorbike), size_factor(VehicleType::Bicycle));
    }

    #[test]
    fn size_factor_emergency_is_highest() {
        assert_eq!(size_factor(VehicleType::Emergency), 5);
    }

    // ---- check_conflicts tests ----

    fn make_crossing_conflict() -> Vec<ConflictPoint> {
        vec![ConflictPoint {
            turn_a_idx: 0,
            turn_b_idx: 1,
            t_a: 0.5,
            t_b: 0.5,
        }]
    }

    #[test]
    fn conflict_two_agents_crossing_near_conflict_point() {
        let conflicts = make_crossing_conflict();
        // Agent 0 on turn 0, t=0.4 (closer to conflict at 0.5 -> dist 0.1)
        // Agent 1 on turn 1, t=0.3 (farther from conflict at 0.5 -> dist 0.2)
        let agents = vec![(1u16, 0.3, VehicleType::Car)];
        let result = check_conflicts(0, 0.4, VehicleType::Car, &agents, &conflicts, 20.0, 0.15);
        // Agent 0 is closer (dist=0.1 vs 0.2), so agent 0 has priority -> no yield
        assert!(result.is_none());
    }

    #[test]
    fn conflict_farther_agent_yields() {
        let conflicts = make_crossing_conflict();
        // Agent on turn 0, t=0.3 (farther: dist=0.2)
        // Foe on turn 1, t=0.4 (closer: dist=0.1)
        let agents = vec![(1u16, 0.4, VehicleType::Car)];
        let result = check_conflicts(0, 0.3, VehicleType::Car, &agents, &conflicts, 20.0, 0.15);
        assert!(result.is_some());
        let r = result.unwrap();
        // Gap = (0.5 - 0.3) * 20.0 = 4.0m
        assert!((r.virtual_leader_gap - 4.0).abs() < 0.1);
        assert!((r.virtual_leader_speed - 0.0).abs() < 1e-10);
    }

    #[test]
    fn conflict_foe_past_conflict_no_yield() {
        let conflicts = make_crossing_conflict();
        // Foe already past conflict point (t=0.7, conflict at 0.5, margin 0.15)
        let agents = vec![(1u16, 0.7, VehicleType::Car)];
        let result = check_conflicts(0, 0.3, VehicleType::Car, &agents, &conflicts, 20.0, 0.15);
        assert!(result.is_none());
    }

    #[test]
    fn conflict_non_crossing_turns_no_yield() {
        let conflicts = make_crossing_conflict(); // only turns 0 and 1
        // Agent on turn 2, foe on turn 3 -- no conflict defined
        let agents = vec![(3u16, 0.5, VehicleType::Car)];
        let result = check_conflicts(2, 0.5, VehicleType::Car, &agents, &conflicts, 20.0, 0.15);
        assert!(result.is_none());
    }

    #[test]
    fn conflict_tiebreak_by_vehicle_type() {
        let conflicts = make_crossing_conflict();
        // Both at same distance from conflict point
        // Motorbike (priority=1) yields to Car (priority=2)
        let agents = vec![(1u16, 0.4, VehicleType::Car)];
        let result = check_conflicts(
            0, 0.4, VehicleType::Motorbike, &agents, &conflicts, 20.0, 0.15,
        );
        assert!(result.is_some(), "motorbike should yield to car on tie");
    }

    #[test]
    fn conflict_tiebreak_higher_priority_does_not_yield() {
        let conflicts = make_crossing_conflict();
        // Both at same distance, Emergency (5) vs Car (2)
        let agents = vec![(1u16, 0.4, VehicleType::Car)];
        let result = check_conflicts(
            0, 0.4, VehicleType::Emergency, &agents, &conflicts, 20.0, 0.15,
        );
        assert!(result.is_none(), "emergency should not yield to car on tie");
    }

    #[test]
    fn conflict_agent_past_own_conflict_no_yield() {
        let conflicts = make_crossing_conflict();
        // Agent already past its own conflict point (t=0.7 > conflict 0.5+0.15)
        let agents = vec![(1u16, 0.4, VehicleType::Car)];
        let result = check_conflicts(0, 0.7, VehicleType::Car, &agents, &conflicts, 20.0, 0.15);
        assert!(result.is_none());
    }

    // ---- yield_deceleration tests ----

    #[test]
    fn yield_deceleration_produces_negative_accel() {
        let idm = IdmParams {
            v0: 13.89,
            s0: 2.0,
            t_headway: 1.5,
            a: 1.0,
            b: 2.0,
            delta: 4.0,
        };
        let accel = yield_deceleration(10.0, 3.0, 0.0, &idm);
        assert!(accel < 0.0, "should decelerate when close to virtual leader");
    }

    #[test]
    fn yield_deceleration_clamped_to_max_decel() {
        let idm = IdmParams {
            v0: 13.89,
            s0: 2.0,
            t_headway: 1.5,
            a: 1.0,
            b: 2.0,
            delta: 4.0,
        };
        let accel = yield_deceleration(20.0, 0.5, 0.0, &idm);
        assert!(accel >= MAX_JUNCTION_DECEL, "should be clamped to max decel");
    }

    #[test]
    fn yield_deceleration_priority_agent_no_decel_needed() {
        // If we don't call yield_deceleration (priority agent), test that
        // the function at least returns a reasonable value for large gap
        let idm = IdmParams {
            v0: 13.89,
            s0: 2.0,
            t_headway: 1.5,
            a: 1.0,
            b: 2.0,
            delta: 4.0,
        };
        let accel = yield_deceleration(10.0, 100.0, 10.0, &idm);
        // With large gap and same speed, should be near free acceleration
        assert!(accel > 0.0, "with large gap should accelerate");
    }
}
