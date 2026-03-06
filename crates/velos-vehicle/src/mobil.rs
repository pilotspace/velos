//! MOBIL lane-change decision model.
//!
//! Evaluates whether a lane change is beneficial (incentive criterion)
//! and safe (safety criterion) based on IDM accelerations in current
//! and target lanes.
//!
//! Reference: Kesting, Treiber, Helbing (2007),
//!            <https://www.mtreiber.de/publications/MOBIL_TRB.pdf>

/// Parameters for the MOBIL lane-change model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MobilParams {
    /// Politeness factor (0 = selfish, 1 = fully altruistic). HCMC default: 0.3.
    pub politeness: f64,
    /// Minimum acceleration advantage threshold (m/s^2).
    pub threshold: f64,
    /// Maximum safe deceleration for new follower (m/s^2, negative value).
    pub safe_decel: f64,
    /// Right-lane bias (m/s^2, positive = prefer right).
    pub right_bias: f64,
}

/// Context for evaluating a single lane-change decision.
///
/// All acceleration values come from IDM evaluations in current vs target lane.
#[derive(Debug, Clone, Copy)]
pub struct LaneChangeContext {
    /// Subject's IDM acceleration with current-lane leader.
    pub accel_current: f64,
    /// Subject's IDM acceleration with target-lane leader.
    pub accel_target: f64,
    /// New follower's IDM acceleration if subject changes lane.
    pub accel_new_follower: f64,
    /// Old follower's current IDM acceleration (before lane change).
    pub accel_old_follower: f64,
    /// Whether this is a change to the right lane.
    pub is_right: bool,
}

/// Evaluate whether a lane change should be performed.
///
/// Returns `true` if the lane change is both safe and provides
/// sufficient incentive (accounting for politeness and lane bias).
pub fn mobil_decision(params: &MobilParams, ctx: &LaneChangeContext) -> bool {
    // Safety criterion: new follower must not brake harder than safe_decel
    if ctx.accel_new_follower < params.safe_decel {
        return false;
    }

    // Incentive criterion
    let own_advantage = ctx.accel_target - ctx.accel_current;
    let follower_disadvantage = ctx.accel_old_follower - ctx.accel_new_follower;
    let bias = if ctx.is_right {
        params.right_bias
    } else {
        -params.right_bias
    };

    let incentive = own_advantage - params.politeness * follower_disadvantage + bias;

    incentive > params.threshold
}
