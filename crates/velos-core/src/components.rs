//! ECS component definitions for agent state.
//! All fields use f64 for CPU-side precision.
//! GPU-side types use fixed-point i32 for cross-GPU determinism.

/// World-space position of an agent in metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    /// East-west coordinate (metres).
    pub x: f64,
    /// North-south coordinate (metres).
    pub y: f64,
}

/// Kinematic state of an agent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kinematics {
    /// Velocity in the x direction (m/s).
    pub vx: f64,
    /// Velocity in the y direction (m/s).
    pub vy: f64,
    /// Scalar speed magnitude (m/s).
    pub speed: f64,
    /// Heading angle in radians (0 = east, CCW positive).
    pub heading: f64,
}

/// Vehicle type tag for an agent in the ECS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VehicleType {
    Motorbike,
    Car,
    Pedestrian,
}

/// Agent's position along a road edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadPosition {
    /// Index of the current edge in the road graph.
    pub edge_index: u32,
    /// Current lane (0-based from right).
    pub lane: u8,
    /// Distance along edge from start node (metres).
    pub offset_m: f64,
}

/// Agent's assigned route as a sequence of node indices.
#[derive(Debug, Clone)]
pub struct Route {
    /// Sequence of node indices forming the path.
    pub path: Vec<u32>,
    /// Current index into `path` (the node we are heading toward).
    pub current_step: usize,
}

/// Lateral offset for motorbike sublane positioning.
///
/// Only attached to motorbike agents. Tracks continuous lateral position
/// across the road width (measured from right edge in metres).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LateralOffset {
    /// Current lateral offset from road right edge (metres).
    pub lateral_offset: f64,
    /// Target lateral position from gap-seeking or swarming (metres).
    pub desired_lateral: f64,
}

/// Agent timing state for gridlock detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaitState {
    /// Simulation time when speed first hit zero.
    pub stopped_since: f64,
    /// True if the agent is waiting at a red signal (not gridlock).
    pub at_red_signal: bool,
}

/// Active lane-change state for cars executing a MOBIL-triggered lane change.
/// Attached when lane change starts, removed when drift completes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaneChangeState {
    /// Target lane index (0-based from right).
    pub target_lane: u8,
    /// Time remaining for the drift (seconds). Starts at 2.0, counts down.
    pub time_remaining: f64,
    /// Simulation time when lane change started (for cooldown).
    pub started_at: f64,
}

/// Tracks the simulation time when the last lane change completed (for cooldown).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LastLaneChange {
    /// Simulation time when the last lane change finished.
    pub completed_at: f64,
}

/// Car-following model selector for per-agent runtime switching.
///
/// The GPU shader branches on this tag to execute IDM or Krauss
/// car-following logic. Stored as `u8` for compact representation
/// in ECS; widened to `u32` in [`GpuAgentState`] for GPU alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CarFollowingModel {
    /// Intelligent Driver Model (Treiber et al. 2000).
    Idm = 0,
    /// Krauss model with safe-speed and dawdle (SUMO default).
    Krauss = 1,
}

/// GPU-side agent state packed for compute shader buffers.
///
/// All position and speed fields use fixed-point integer representation
/// for cross-GPU determinism. Layout is `#[repr(C)]` with 32 bytes total
/// for cache-aligned GPU access.
///
/// Field formats:
/// - `position`: Q16.16 fixed-point (metres along edge)
/// - `lateral`: Q8.8 fixed-point stored in i32 (metres from road right edge)
/// - `speed`: Q12.20 fixed-point (m/s)
/// - `acceleration`: Q12.20 fixed-point (m/s^2)
/// - `cf_model`: 0 = IDM, 1 = Krauss (matches [`CarFollowingModel`] discriminant)
/// - `rng_state`: PCG hash state for Krauss dawdle stochastic component
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuAgentState {
    /// Index of the current edge in the road graph.
    pub edge_id: u32,
    /// Lane index within the edge (0-based from right).
    pub lane_idx: u32,
    /// Longitudinal position along edge (Q16.16 fixed-point, metres).
    pub position: i32,
    /// Lateral offset from road right edge (Q8.8 in i32, metres).
    pub lateral: i32,
    /// Current speed (Q12.20 fixed-point, m/s).
    pub speed: i32,
    /// Current acceleration (Q12.20 fixed-point, m/s^2).
    pub acceleration: i32,
    /// Car-following model tag (0 = IDM, 1 = Krauss).
    pub cf_model: u32,
    /// RNG state for stochastic models (PCG hash seed).
    pub rng_state: u32,
}
