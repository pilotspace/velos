//! Motorbike sublane lateral movement model.
//!
//! Implements continuous lateral positioning for motorbikes in HCMC mixed traffic.
//! Motorbikes seek lateral gaps >= `min_filter_gap` and drift toward them at bounded speed.
//! At red lights, swarming behavior fills the full road width.
//!
//! All functions are pure (no ECS dependency) following the IDM/MOBIL pattern.
//!
//! Reference: VELOS architecture doc `02-agent-models.md` Section 1.

/// Parameters for the motorbike sublane model.
///
/// Default values locked per CONTEXT.md decisions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SublaneParams {
    /// Minimum lateral gap required to filter through (metres).
    pub min_filter_gap: f64,
    /// Maximum lateral drift speed (m/s).
    pub max_lateral_speed: f64,
    /// Half-width of the ego motorbike body (metres).
    pub half_width: f64,
    /// Lateral drift speed during red-light swarming (m/s).
    pub swarm_lateral_speed: f64,
}

impl Default for SublaneParams {
    fn default() -> Self {
        Self {
            min_filter_gap: 0.6,
            max_lateral_speed: 1.0,
            half_width: 0.25,
            swarm_lateral_speed: 0.8,
        }
    }
}

/// Information about a neighboring agent for lateral gap computation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeighborInfo {
    /// Lateral offset of neighbor from road right edge (metres).
    pub lateral_offset: f64,
    /// Longitudinal gap to this neighbor (metres, positive = ahead).
    pub longitudinal_gap: f64,
    /// Half-width of the neighbor body (metres).
    pub half_width: f64,
    /// Longitudinal speed of the neighbor (m/s).
    pub speed: f64,
}

/// Maximum longitudinal distance to consider a neighbor for lateral gap computation.
const LATERAL_SCAN_AHEAD: f64 = 10.0;


/// Compute desired lateral target position for a motorbike.
///
/// In normal mode, probes left and right for gaps >= `min_filter_gap`,
/// scores by available gap size, and returns the best lateral offset.
/// If no valid gap found, returns current position.
///
/// In red-light swarming mode, finds the largest lateral gap across the
/// full road width and returns its center (motorbikes spread to fill road).
///
/// Result is always clamped to `[half_width, road_width - half_width]`.
///
/// # Arguments
/// * `own_lateral` - current lateral offset from road right edge (m)
/// * `own_speed` - current longitudinal speed (m/s)
/// * `road_width` - total road width (m)
/// * `neighbors` - nearby agents within lateral scan range
/// * `at_red_light` - true if at a red light stop line (triggers swarming)
/// * `params` - sublane model parameters
pub fn compute_desired_lateral(
    own_lateral: f64,
    _own_speed: f64,
    road_width: f64,
    neighbors: &[NeighborInfo],
    at_red_light: bool,
    params: &SublaneParams,
) -> f64 {
    let min_lat = params.half_width;
    let max_lat = road_width - params.half_width;

    // Clamp current position to valid range
    let own_clamped = own_lateral.clamp(min_lat, max_lat);

    if neighbors.is_empty() && !at_red_light {
        return own_clamped;
    }

    // Filter to longitudinally relevant neighbors
    let relevant: Vec<&NeighborInfo> = neighbors
        .iter()
        .filter(|n| n.longitudinal_gap.abs() < LATERAL_SCAN_AHEAD)
        .collect();

    if at_red_light {
        return find_largest_gap_center(own_clamped, road_width, &relevant, params)
            .clamp(min_lat, max_lat);
    }

    // Normal mode: probe left and right from current position
    let probe_step = 0.3; // metres per probe step
    let mut best_lateral = own_clamped;
    let mut best_gap = lateral_gap_at(own_clamped, &relevant, params);

    // Probe rightward (decreasing lateral offset)
    let mut probe = own_clamped - probe_step;
    while probe >= min_lat {
        let gap = lateral_gap_at(probe, &relevant, params);
        if gap >= params.min_filter_gap && gap > best_gap {
            best_gap = gap;
            best_lateral = probe;
        }
        probe -= probe_step;
    }

    // Probe leftward (increasing lateral offset)
    probe = own_clamped + probe_step;
    while probe <= max_lat {
        let gap = lateral_gap_at(probe, &relevant, params);
        if gap >= params.min_filter_gap && gap > best_gap {
            best_gap = gap;
            best_lateral = probe;
        }
        probe += probe_step;
    }

    // Only move if we found a gap that meets the minimum threshold
    if best_gap >= params.min_filter_gap && (best_lateral - own_clamped).abs() > 0.01 {
        best_lateral.clamp(min_lat, max_lat)
    } else {
        own_clamped
    }
}

/// Compute the available lateral gap at a probe position.
///
/// For each neighbor within longitudinal range, computes the clearance
/// between the ego body edge and the neighbor body edge. Returns the
/// minimum clearance (bottleneck gap).
///
/// If no neighbors are relevant, returns `f64::MAX`.
pub fn lateral_gap_at(
    probe_lateral: f64,
    neighbors: &[&NeighborInfo],
    params: &SublaneParams,
) -> f64 {
    if neighbors.is_empty() {
        return f64::MAX;
    }

    let mut min_gap = f64::MAX;
    for n in neighbors {
        // Distance between centers minus both half-widths = clearance
        let center_dist = (probe_lateral - n.lateral_offset).abs();
        let clearance = center_dist - params.half_width - n.half_width;
        if clearance < min_gap {
            min_gap = clearance;
        }
    }
    min_gap
}

/// Find the center of the largest lateral gap across the road width.
///
/// Used during red-light swarming: motorbikes spread across the full road.
/// Scans the road width at evenly-spaced probe positions and returns the
/// center of the widest contiguous gap.
fn find_largest_gap_center(
    own_lateral: f64,
    road_width: f64,
    neighbors: &[&NeighborInfo],
    params: &SublaneParams,
) -> f64 {
    let min_lat = params.half_width;
    let max_lat = road_width - params.half_width;

    if neighbors.is_empty() {
        return own_lateral;
    }

    // Collect obstacle edges (neighbor positions +/- their half-width + ego half-width)
    let mut edges: Vec<(f64, f64)> = neighbors
        .iter()
        .map(|n| {
            let lo = (n.lateral_offset - n.half_width - params.half_width).max(min_lat);
            let hi = (n.lateral_offset + n.half_width + params.half_width).min(max_lat);
            (lo, hi)
        })
        .collect();
    edges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Find the largest gap between obstacles
    let mut best_center = own_lateral;
    let mut best_width = 0.0_f64;

    // Gap before first obstacle
    if let Some(first) = edges.first() {
        let gap_width = first.0 - min_lat;
        if gap_width > best_width {
            best_width = gap_width;
            best_center = min_lat + gap_width / 2.0;
        }
    }

    // Gaps between obstacles
    for i in 1..edges.len() {
        let gap_start = edges[i - 1].1;
        let gap_end = edges[i].0;
        let gap_width = gap_end - gap_start;
        if gap_width > best_width {
            best_width = gap_width;
            best_center = gap_start + gap_width / 2.0;
        }
    }

    // Gap after last obstacle
    if let Some(last) = edges.last() {
        let gap_width = max_lat - last.1;
        if gap_width > best_width {
            best_center = last.1 + gap_width / 2.0;
        }
    }

    best_center
}

/// Apply lateral drift toward desired position, clamped by max speed.
///
/// Displacement per step = `(desired - current)` clamped to `[-max_speed*dt, max_speed*dt]`.
/// This forward-Euler with clamp ensures dt-consistency: the same total displacement
/// occurs regardless of timestep subdivision.
///
/// # Arguments
/// * `current` - current lateral offset (m)
/// * `desired` - target lateral offset (m)
/// * `max_speed` - maximum lateral drift speed (m/s)
/// * `dt` - timestep (s)
///
/// # Returns
/// New lateral offset after drift.
pub fn apply_lateral_drift(current: f64, desired: f64, max_speed: f64, dt: f64) -> f64 {
    let diff = desired - current;
    let max_disp = max_speed * dt;
    let displacement = diff.clamp(-max_disp, max_disp);
    current + displacement
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_match_context_md() {
        let p = SublaneParams::default();
        assert!((p.min_filter_gap - 0.6).abs() < 1e-10);
        assert!((p.max_lateral_speed - 1.0).abs() < 1e-10);
        assert!((p.half_width - 0.25).abs() < 1e-10);
        assert!((p.swarm_lateral_speed - 0.8).abs() < 1e-10);
    }

    #[test]
    fn lateral_gap_no_neighbors() {
        let params = SublaneParams::default();
        let gap = lateral_gap_at(1.0, &[], &params);
        assert_eq!(gap, f64::MAX);
    }

    #[test]
    fn apply_drift_exact_arrival() {
        // When desired == current, no movement
        let result = apply_lateral_drift(2.0, 2.0, 1.0, 0.1);
        assert!((result - 2.0).abs() < 1e-10);
    }

    #[test]
    fn apply_drift_small_diff_not_clamped() {
        // Diff smaller than max displacement
        let result = apply_lateral_drift(2.0, 2.05, 1.0, 0.1);
        assert!((result - 2.05).abs() < 1e-10);
    }
}
